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

    let mut groups = build_groups(&all);
    let mut group_sel: usize = 0;
    let mut group_list_state = ListState::default();

    let mut visible = compute_visible(&all, &composites, &groups[group_sel].0, &query, &matcher);
    let mut snippet_sel: usize = 0;
    let mut snippet_list_state = ListState::default();

    // index into `visible` captured at 'd' press to prevent wrong-item deletion
    let mut pending_delete: Option<usize> = None;

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
                all.len()
            );
            let outer = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(
                    Line::from(Span::styled(
                        " tet ",
                        Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD),
                    ))
                    .alignment(Alignment::Left),
                )
                .title(
                    Line::from(Span::styled(count_str, Style::default().fg(Color::DarkGray)))
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
                (format!("/ {}_", query), Style::default().fg(Color::Yellow))
            } else if !query.is_empty() {
                (
                    format!("/ {}  [active — / to edit, Esc to clear]", query),
                    Style::default().fg(Color::Green),
                )
            } else {
                (
                    "/ fuzzy search…  (name · command · group)".to_string(),
                    Style::default().fg(Color::DarkGray),
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
                .highlight_style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
                .highlight_symbol("▶ ");
            f.render_stateful_widget(groups_list, cols[0], &mut group_list_state);

            // Snippets pane
            let snippet_items: Vec<ListItem> = visible
                .iter()
                .enumerate()
                .map(|(vi, &ai)| {
                    let style = if pending_delete == Some(vi) {
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
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
                .highlight_style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
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
                        Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        format!(" sh · {} {}", s.commands.len(), line_word),
                        Style::default().fg(Color::DarkGray),
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
                    Style::default().fg(Color::DarkGray),
                ))]
            };
            f.render_widget(
                Paragraph::new(preview_lines).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::DarkGray))
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
            } else {
                "  ↑↓ nav  ↵ run  d delete  / search  Tab switch pane  q quit"
            };
            f.render_widget(
                Paragraph::new(Span::styled(status, Style::default().fg(Color::DarkGray))),
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
                        groups = build_groups(&all);
                        group_sel = group_sel.min(groups.len().saturating_sub(1));
                        visible = compute_visible(&all, &composites, &groups[group_sel].0, &query, &matcher);
                        snippet_sel = snippet_sel.min(visible.len().saturating_sub(1));
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
                        visible = compute_visible(&all, &composites, &groups[group_sel].0, &query, &matcher);
                        snippet_sel = 0;
                        focus = Focus::Snippets;
                    }
                    KeyCode::Enter => focus = Focus::Snippets,
                    KeyCode::Backspace => {
                        query.pop();
                        visible = compute_visible(&all, &composites, &groups[group_sel].0, &query, &matcher);
                        snippet_sel = 0;
                    }
                    KeyCode::Char(c) if query.len() < 200 => {
                        query.push(c);
                        visible = compute_visible(&all, &composites, &groups[group_sel].0, &query, &matcher);
                        snippet_sel = 0;
                    }
                    _ => {}
                }
                continue;
            }

            // Normal navigation
            match (key.code, key.modifiers) {
                (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => return Ok(None),
                (KeyCode::Char('q'), _) | (KeyCode::Esc, _) if focus == Focus::Groups => {
                    return Ok(None);
                }
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
                    }
                }
                (KeyCode::Down, _) if focus == Focus::Groups => {
                    if group_sel + 1 < groups.len() {
                        group_sel += 1;
                        visible = compute_visible(&all, &composites, &groups[group_sel].0, &query, &matcher);
                        snippet_sel = 0;
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
        Style::default().fg(Color::LightBlue)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn composite(s: &Snippet) -> String {
    if s.group_name.is_empty() {
        format!("{} {}", s.name, s.commands.join(" "))
    } else {
        format!("{} {} {}", s.group_name, s.name, s.commands.join(" "))
    }
}

fn build_groups(all: &[Snippet]) -> Vec<(GroupFilter, usize)> {
    let mut named: BTreeMap<String, usize> = BTreeMap::new();
    let mut ungrouped = 0usize;
    for s in all {
        if s.group_name.is_empty() {
            ungrouped += 1;
        } else {
            *named.entry(s.group_name.clone()).or_insert(0) += 1;
        }
    }
    let mut result = vec![(GroupFilter::All, all.len())];
    if ungrouped > 0 {
        result.push((GroupFilter::Ungrouped, ungrouped));
    }
    for (name, count) in named {
        result.push((GroupFilter::Named(name), count));
    }
    result
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
        Style::default().fg(Color::DarkGray),
    )];
    for (i, part) in cmd.split(" | ").enumerate() {
        if i > 0 {
            spans.push(Span::styled(" | ", Style::default().fg(Color::Yellow)));
        }
        spans.push(Span::raw(part.to_string()));
    }
    Line::from(spans)
}
