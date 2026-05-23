use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};
use rusqlite::Connection;
use std::{collections::BTreeMap, io};

use crate::db::ops;
use crate::models::Snippet;
use super::TerminalGuard;

// Tokyo Night palette
const TN_BLUE: Color   = Color::Rgb(122, 162, 247); // #7AA2F7 — active border, title, highlight
const TN_CYAN: Color   = Color::Rgb(125, 207, 255); // #7DCFFF — preview header
const TN_ORANGE: Color = Color::Rgb(255, 158, 100); // #FF9E64 — pipe |
const TN_YELLOW: Color = Color::Rgb(224, 175, 104); // #E0AF68 — search active
const TN_GREEN: Color  = Color::Rgb(158, 206, 106); // #9ECE6A — active query indicator
const TN_RED: Color    = Color::Rgb(247, 118, 142); // #F7768E — delete
const TN_DIM: Color    = Color::Rgb(59,  66,  97);  // #3B4261 — inactive borders
const TN_MUTED: Color  = Color::Rgb(86,  95,  137); // #565F89 — status, labels, line numbers

#[derive(Clone, PartialEq)]
enum GroupFilter {
    All,
    Ungrouped,
    Named(String),
}

impl GroupFilter {
    fn label(&self) -> &str {
        match self {
            Self::All => "[ALL]",
            Self::Ungrouped => "[UNGROUPED]",
            Self::Named(g) => g.as_str(),
        }
    }

    fn matches(&self, s: &Snippet) -> bool {
        match self {
            Self::All => true,
            Self::Ungrouped => s.group_name.is_empty(),
            Self::Named(g) => &s.group_name == g,
        }
    }
}

#[derive(PartialEq)]
enum Focus {
    Groups,
    Snippets,
    Search,
}

pub fn run(conn: &Connection, snippets: Vec<Snippet>) -> Result<Option<Snippet>> {
    let _guard = TerminalGuard::new()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let matcher = SkimMatcherV2::default();
    let mut all = snippets;
    let mut composites: Vec<String> = all.iter().map(composite).collect();
    let mut query = String::new();
    let mut focus = Focus::Snippets;

    let init_indices: Vec<usize> = (0..all.len()).collect();
    let mut groups = build_groups_filtered(&all, &init_indices);
    let mut group_sel: usize = 0;
    let mut group_list_state = ListState::default();

    let mut visible = compute_visible(&all, &composites, &groups[group_sel].0, &query, &matcher);
    let mut snippet_sel: usize = 0;
    let mut snippet_list_state = ListState::default();

    // index into `visible` captured at 'd' press to prevent wrong-item deletion
    let mut pending_delete: Option<usize> = None;
    let mut group_total = count_in_group(&all, &groups[group_sel].0);

    loop {
        group_list_state.select(Some(group_sel));
        snippet_list_state.select(
            if visible.is_empty() || pending_delete.is_some() { None } else { Some(snippet_sel) },
        );

        terminal.draw(|f| {
            let area = f.area();

            // ── Outer border ──────────────────────────────────────────────────
            let count_str = format!(
                " {}  ·  {}/{} ",
                groups[group_sel].0.label(),
                visible.len(),
                group_total,
            );
            let outer = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(TN_DIM))
                .title(
                    Line::from(Span::styled(
                        " tet ",
                        Style::default().fg(TN_BLUE).add_modifier(Modifier::BOLD),
                    ))
                    .alignment(Alignment::Left),
                )
                .title(
                    Line::from(Span::styled(count_str, Style::default().fg(TN_MUTED)))
                        .alignment(Alignment::Right),
                );
            let inner = outer.inner(area);
            f.render_widget(outer, area);

            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Min(0),
                    Constraint::Length(1),
                ])
                .split(inner);

            // ── Search bar ────────────────────────────────────────────────────
            let (search_text, search_style) = if focus == Focus::Search {
                (format!("/ {}_", query), Style::default().fg(TN_YELLOW))
            } else if !query.is_empty() {
                (
                    format!("/ {}  [active — / to edit, Esc to clear]", query),
                    Style::default().fg(TN_GREEN),
                )
            } else {
                (
                    "/ fuzzy search…  (name · command · group)".to_string(),
                    Style::default().fg(TN_MUTED),
                )
            };
            f.render_widget(
                Paragraph::new(Span::styled(search_text, search_style)),
                rows[0],
            );

            // ── Three panes ───────────────────────────────────────────────────
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(20),
                    Constraint::Percentage(35),
                    Constraint::Fill(1),
                ])
                .split(rows[1]);

            // Groups pane
            let group_items: Vec<ListItem> = groups
                .iter()
                .map(|(filter, count)| {
                    ListItem::new(format!("{:<13}  {:>3}", filter.label(), count))
                })
                .collect();
            let groups_list = List::new(group_items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(pane_border(focus == Focus::Groups))
                        .title(Span::styled(" GROUPS ", Style::default().add_modifier(Modifier::BOLD))),
                )
                .highlight_style(Style::default().fg(TN_BLUE).add_modifier(Modifier::BOLD))
                .highlight_symbol("▶ ");
            f.render_stateful_widget(groups_list, cols[0], &mut group_list_state);

            // Snippets pane
            let snippet_items: Vec<ListItem> = visible
                .iter()
                .enumerate()
                .map(|(vi, &ai)| {
                    let style = if pending_delete == Some(vi) {
                        Style::default().fg(TN_RED).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    ListItem::new(Span::styled(all[ai].name.clone(), style))
                })
                .collect();
            let snippets_list = List::new(snippet_items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(pane_border(focus == Focus::Snippets))
                        .title(Span::styled(
                            format!(" SNIPPETS · {} ", visible.len()),
                            Style::default().add_modifier(Modifier::BOLD),
                        )),
                )
                .highlight_style(Style::default().fg(TN_BLUE).add_modifier(Modifier::BOLD))
                .highlight_symbol("▶ ");
            f.render_stateful_widget(snippets_list, cols[1], &mut snippet_list_state);

            // Preview pane
            let preview_lines: Vec<Line> = if let Some(&ai) = visible.get(snippet_sel) {
                let s = &all[ai];
                let header = if s.group_name.is_empty() {
                    s.name.clone()
                } else {
                    format!("{} › {}", s.group_name, s.name)
                };
                let line_word = if s.commands.len() == 1 { "line" } else { "lines" };
                let mut lines = vec![
                    Line::from(Span::styled(
                        format!(" {}", header),
                        Style::default().fg(TN_CYAN).add_modifier(Modifier::BOLD),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        format!(" sh · {} {}", s.commands.len(), line_word),
                        Style::default().fg(TN_MUTED),
                    )),
                    Line::from(""),
                ];
                for (i, cmd) in s.commands.iter().enumerate() {
                    lines.push(command_line(i + 1, cmd));
                }
                lines
            } else {
                vec![Line::from(Span::styled(
                    " No snippet selected",
                    Style::default().fg(TN_MUTED),
                ))]
            };
            f.render_widget(
                Paragraph::new(preview_lines).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(TN_DIM))
                        .title(Span::styled(
                            " PREVIEW ",
                            Style::default().add_modifier(Modifier::BOLD),
                        )),
                ),
                cols[2],
            );

            // ── Status bar ────────────────────────────────────────────────────
            let status = if pending_delete.is_some() {
                "  [y] Confirm delete  [n/Esc] Cancel"
            } else if focus == Focus::Search {
                "  Type to filter  [Enter/Esc] done"
            } else if !query.is_empty() {
                "  ↑↓ nav  ↵ run  d delete  / edit search  Esc clear search  q quit"
            } else {
                "  ↑↓ nav  ↵ run  d delete  / search  Tab switch pane  q quit"
            };
            f.render_widget(
                Paragraph::new(Span::styled(status, Style::default().fg(TN_MUTED))),
                rows[2],
            );
        })?;

        // ── Events ────────────────────────────────────────────────────────────
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            // Delete confirmation
            if let Some(vi) = pending_delete {
                if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                    if let Some(&ai) = visible.get(vi) {
                        ops::delete_snippet(conn, all[ai].id)?;
                        all.remove(ai);
                        composites.remove(ai);
                        let current_filter = groups[group_sel].0.clone();
                        (groups, group_sel, visible) = refresh(&all, &composites, &query, &matcher, &current_filter);
                        snippet_sel = snippet_sel.min(visible.len().saturating_sub(1));
                        group_total = count_in_group(&all, &groups[group_sel].0);
                    }
                }
                pending_delete = None;
                continue;
            }

            // Search mode
            if focus == Focus::Search {
                match key.code {
                    KeyCode::Esc => {
                        query.clear();
                        let current_filter = groups[group_sel].0.clone();
                        (groups, group_sel, visible) = refresh(&all, &composites, &query, &matcher, &current_filter);
                        snippet_sel = 0;
                        group_total = count_in_group(&all, &groups[group_sel].0);
                        focus = Focus::Snippets;
                    }
                    KeyCode::Enter => focus = Focus::Snippets,
                    KeyCode::Backspace => {
                        query.pop();
                        let current_filter = groups[group_sel].0.clone();
                        (groups, group_sel, visible) = refresh(&all, &composites, &query, &matcher, &current_filter);
                        snippet_sel = 0;
                        group_total = count_in_group(&all, &groups[group_sel].0);
                    }
                    KeyCode::Char(c) if query.len() < 200 => {
                        query.push(c);
                        let current_filter = groups[group_sel].0.clone();
                        (groups, group_sel, visible) = refresh(&all, &composites, &query, &matcher, &current_filter);
                        snippet_sel = 0;
                        group_total = count_in_group(&all, &groups[group_sel].0);
                    }
                    _ => {}
                }
                continue;
            }

            // Normal navigation
            match (key.code, key.modifiers) {
                (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => return Ok(None),
                (KeyCode::Char('q'), _) => return Ok(None),
                // Esc clears an active query before navigating or quitting
                (KeyCode::Esc, _) if !query.is_empty() => {
                    query.clear();
                    let current_filter = groups[group_sel].0.clone();
                    (groups, group_sel, visible) = refresh(&all, &composites, &query, &matcher, &current_filter);
                    snippet_sel = 0;
                    group_total = count_in_group(&all, &groups[group_sel].0);
                }
                (KeyCode::Esc, _) if focus == Focus::Groups => return Ok(None),
                (KeyCode::Esc, _) if focus == Focus::Snippets => focus = Focus::Groups,
                (KeyCode::Char('/'), _) => focus = Focus::Search,
                (KeyCode::Tab, _) => {
                    focus = if focus == Focus::Groups { Focus::Snippets } else { Focus::Groups };
                }
                (KeyCode::Right, _) | (KeyCode::Enter, _) if focus == Focus::Groups => {
                    if !visible.is_empty() {
                        focus = Focus::Snippets;
                    }
                }
                (KeyCode::Left, _) if focus == Focus::Snippets => focus = Focus::Groups,
                (KeyCode::Enter, _) if focus == Focus::Snippets => {
                    if let Some(&ai) = visible.get(snippet_sel) {
                        return Ok(Some(all.remove(ai)));
                    }
                }
                (KeyCode::Up, _) if focus == Focus::Groups => {
                    if group_sel > 0 {
                        group_sel -= 1;
                        visible = compute_visible(&all, &composites, &groups[group_sel].0, &query, &matcher);
                        snippet_sel = 0;
                        group_total = count_in_group(&all, &groups[group_sel].0);
                    }
                }
                (KeyCode::Down, _) if focus == Focus::Groups => {
                    if group_sel + 1 < groups.len() {
                        group_sel += 1;
                        visible = compute_visible(&all, &composites, &groups[group_sel].0, &query, &matcher);
                        snippet_sel = 0;
                        group_total = count_in_group(&all, &groups[group_sel].0);
                    }
                }
                (KeyCode::Up, _) if focus == Focus::Snippets => {
                    snippet_sel = snippet_sel.saturating_sub(1);
                }
                (KeyCode::Down, _) if focus == Focus::Snippets => {
                    if snippet_sel + 1 < visible.len() {
                        snippet_sel += 1;
                    }
                }
                (KeyCode::Char('d'), m)
                    if focus == Focus::Snippets && !m.contains(KeyModifiers::CONTROL) =>
                {
                    if !visible.is_empty() {
                        pending_delete = Some(snippet_sel);
                    }
                }
                _ => {}
            }
        }
    }
}

fn pane_border(active: bool) -> Style {
    if active {
        Style::default().fg(TN_BLUE)
    } else {
        Style::default().fg(TN_DIM)
    }
}

fn composite(s: &Snippet) -> String {
    if s.group_name.is_empty() {
        format!("{} {}", s.name, s.commands.join(" "))
    } else {
        format!("{} {} {}", s.group_name, s.name, s.commands.join(" "))
    }
}

// Only groups with at least one match appear; counts reflect matched items only.
fn build_groups_filtered(all: &[Snippet], indices: &[usize]) -> Vec<(GroupFilter, usize)> {
    let mut named: BTreeMap<String, usize> = BTreeMap::new();
    let mut ungrouped = 0usize;
    for &i in indices {
        let s = &all[i];
        if s.group_name.is_empty() {
            ungrouped += 1;
        } else {
            *named.entry(s.group_name.clone()).or_insert(0) += 1;
        }
    }
    let mut result = vec![(GroupFilter::All, indices.len())];
    if ungrouped > 0 {
        result.push((GroupFilter::Ungrouped, ungrouped));
    }
    for (name, count) in named {
        result.push((GroupFilter::Named(name), count));
    }
    result
}

// Rebuilds groups (filtered by query) and visible snippets together.
// Tries to keep the previously selected group; falls back to [ALL] if it disappears.
fn refresh(
    all: &[Snippet],
    composites: &[String],
    query: &str,
    matcher: &SkimMatcherV2,
    current_filter: &GroupFilter,
) -> (Vec<(GroupFilter, usize)>, usize, Vec<usize>) {
    let all_matching = compute_visible(all, composites, &GroupFilter::All, query, matcher);
    let groups = build_groups_filtered(all, &all_matching);
    let group_sel = groups
        .iter()
        .position(|(f, _)| f == current_filter)
        .unwrap_or(0);
    // Derive visible from all_matching to avoid a second full fuzzy scan
    let visible = match &groups[group_sel].0 {
        GroupFilter::All => all_matching,
        filter => all_matching.into_iter().filter(|&i| filter.matches(&all[i])).collect(),
    };
    (groups, group_sel, visible)
}

fn count_in_group(all: &[Snippet], filter: &GroupFilter) -> usize {
    all.iter().filter(|s| filter.matches(s)).count()
}

fn compute_visible(
    all: &[Snippet],
    composites: &[String],
    filter: &GroupFilter,
    query: &str,
    matcher: &SkimMatcherV2,
) -> Vec<usize> {
    let base: Vec<usize> = (0..all.len()).filter(|&i| filter.matches(&all[i])).collect();
    if query.is_empty() {
        return base;
    }
    let mut scored: Vec<(i64, usize)> = base
        .into_iter()
        .filter_map(|i| matcher.fuzzy_match(&composites[i], query).map(|score| (score, i)))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, i)| i).collect()
}

fn command_line(num: usize, cmd: &str) -> Line<'static> {
    let mut spans = vec![Span::styled(
        format!("  {:>2}  ", num),
        Style::default().fg(TN_MUTED),
    )];
    for (i, part) in cmd.split(" | ").enumerate() {
        if i > 0 {
            spans.push(Span::styled(" | ", Style::default().fg(TN_ORANGE)));
        }
        spans.push(Span::raw(part.to_string()));
    }
    Line::from(spans)
}
