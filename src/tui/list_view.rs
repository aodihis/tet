use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
    Terminal,
};
use rusqlite::Connection;
use std::{collections::BTreeMap, io};

use crate::db::ops;
use crate::models::Snippet;
use crate::tui::save_form::{self, FormState};
use super::TerminalGuard;
use super::colors::{TN_BLUE, TN_CYAN, TN_DIM, TN_GREEN, TN_MUTED, TN_ORANGE, TN_RED, TN_YELLOW};

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
    Preview,
    Search,
}

pub fn run(conn: &Connection, snippets: Vec<Snippet>, prefill_query: Option<String>) -> Result<Option<Snippet>> {
    let _guard = TerminalGuard::new()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let matcher = SkimMatcherV2::default();
    let mut all = snippets;
    let mut composites: Vec<String> = all.iter().map(composite).collect();
    let mut query = prefill_query.unwrap_or_default();
    let mut focus = Focus::Snippets;

    let (mut groups, mut group_sel, mut visible) =
        refresh(&all, &composites, &query, &matcher, &GroupFilter::All);
    let mut group_list_state = ListState::default();
    let mut snippet_sel: usize = 0;
    let mut snippet_list_state = ListState::default();

    // index into `visible` captured at 'd' press to prevent wrong-item deletion
    let mut pending_delete: Option<usize> = None;
    let mut pending_group_delete = false;
    let mut group_total = count_in_group(&all, &groups[group_sel].0);
    let mut notice: Option<String> = None;
    let mut preview_cmd_sel: usize = 0;

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
                let text_w = cols[2].width.saturating_sub(2 + 6) as usize;
                let sel_cmd = preview_cmd_sel.min(s.commands.len().saturating_sub(1));
                for (i, cmd) in s.commands.iter().enumerate() {
                    let highlighted = focus == Focus::Preview && i == sel_cmd;
                    lines.extend(command_lines(i + 1, cmd, text_w, highlighted));
                }
                lines
            } else {
                vec![Line::from(Span::styled(
                    " No snippet selected",
                    Style::default().fg(TN_MUTED),
                ))]
            };
            f.render_widget(
                Paragraph::new(preview_lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(pane_border(focus == Focus::Preview))
                            .title(Span::styled(
                                " PREVIEW ",
                                Style::default().add_modifier(Modifier::BOLD),
                            )),
                    ),
                cols[2],
            );

            // ── Status bar ────────────────────────────────────────────────────
            let status: String = if let Some(ref msg) = notice {
                format!("  {}", msg)
            } else if pending_delete.is_some() || pending_group_delete {
                "  Y  confirm".to_string()
            } else if focus == Focus::Search {
                "  Type to filter  [Enter/Esc] done".to_string()
            } else if !query.is_empty() {
                "  ↑↓ nav  ↵ run  d delete  e edit  / edit search  Esc clear  q quit".to_string()
            } else if focus == Focus::Groups {
                "  ↑↓ nav  d delete group  → enter  Tab switch  q quit".to_string()
            } else if focus == Focus::Preview {
                "  ↑↓ select command  Ctrl+C copy  ← back  q quit".to_string()
            } else {
                "  ↑↓ nav  ↵ run  d delete  e edit  → preview  / search  Tab switch  q quit".to_string()
            };
            f.render_widget(
                Paragraph::new(Span::styled(status, Style::default().fg(TN_MUTED))),
                rows[2],
            );

            // ── Delete confirmation popup ─────────────────────────────────────
            if pending_delete.is_some() || pending_group_delete {
                let popup_area = centered_rect(50, 12, area);
                f.render_widget(Clear, popup_area);

                let (subject, detail) = if let Some(vi) = pending_delete {
                    let name = visible.get(vi)
                        .and_then(|&ai| all.get(ai))
                        .map(|s| s.name.as_str())
                        .unwrap_or("?");
                    (format!("  Delete snippet \"{}\"?", name), None)
                } else {
                    match &groups[group_sel].0 {
                        GroupFilter::All => (
                            "  Delete ALL snippets?".to_string(),
                            Some(format!("  {} snippets will be permanently removed.", group_total)),
                        ),
                        GroupFilter::Ungrouped => (
                            "  Delete all ungrouped snippets?".to_string(),
                            Some(format!("  {} snippets will be permanently removed.", group_total)),
                        ),
                        GroupFilter::Named(g) => (
                            format!("  Delete group \"{}\"?", g),
                            Some(format!("  {} snippets will be permanently removed.", group_total)),
                        ),
                    }
                };

                let mut popup_lines = vec![
                    Line::from(""),
                    Line::from(Span::styled(
                        subject,
                        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                    )),
                ];
                if let Some(d) = detail {
                    popup_lines.push(Line::from(Span::styled(
                        d,
                        Style::default().fg(TN_ORANGE),
                    )));
                }
                popup_lines.extend([
                    Line::from(Span::styled(
                        "  This action cannot be undone.",
                        Style::default().fg(TN_RED),
                    )),
                    Line::from(""),
                    Line::from(vec![
                        Span::raw("    "),
                        Span::styled(
                            "  Y  ",
                            Style::default().bg(TN_RED).fg(Color::White).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            "  Yes, delete forever",
                            Style::default().fg(TN_RED).add_modifier(Modifier::BOLD),
                        ),
                    ])
                ]);

                f.render_widget(
                    Paragraph::new(popup_lines).block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_type(BorderType::Double)
                            .border_style(Style::default().fg(TN_RED))
                            .title(Span::styled(
                                " ⚠  DELETE CONFIRMATION ",
                                Style::default()
                                    .fg(TN_RED)
                                    .add_modifier(Modifier::BOLD),
                            )),
                    ),
                    popup_area,
                );
            }
        })?;

        // ── Events ────────────────────────────────────────────────────────────
        let ev = event::read()?;
        if matches!(ev, Event::Resize(..)) {
            continue; // loop top redraws
        }
        notice = None;
        if let Event::Key(key) = ev {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            // Group delete confirmation
            if pending_group_delete {
                if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                    let filter = groups[group_sel].0.clone();
                    match &filter {
                        GroupFilter::All           => { ops::delete_all(conn)?; }
                        GroupFilter::Ungrouped     => { ops::delete_by_group(conn, "")?; }
                        GroupFilter::Named(g)      => { ops::delete_by_group(conn, g)?; }
                    }
                    all = ops::list_all(conn)?;
                    composites = all.iter().map(composite).collect();
                    (groups, group_sel, visible) = refresh(&all, &composites, &query, &matcher, &GroupFilter::All);
                    snippet_sel = 0;
                    group_total = count_in_group(&all, &groups[group_sel].0);
                }
                pending_group_delete = false;
                continue;
            }

            // Snippet delete confirmation
            if let Some(vi) = pending_delete {
                if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y'))
                    && let Some(&ai) = visible.get(vi)
                {
                    ops::delete_snippet(conn, all[ai].id)?;
                    all.remove(ai);
                    composites.remove(ai);
                    let current_filter = groups[group_sel].0.clone();
                    (groups, group_sel, visible) = refresh(&all, &composites, &query, &matcher, &current_filter);
                    snippet_sel = snippet_sel.min(visible.len().saturating_sub(1));
                    group_total = count_in_group(&all, &groups[group_sel].0);
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
                // Ctrl+C in Preview pane copies the selected command; elsewhere it quits
                (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) && focus == Focus::Preview => {
                    if let Some(&ai) = visible.get(snippet_sel) {
                        let cmds = &all[ai].commands;
                        let idx = preview_cmd_sel.min(cmds.len().saturating_sub(1));
                        let text = cmds[idx].clone();
                        match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(text)) {
                            Ok(()) => notice = Some(format!("Copied command {}.", idx + 1)),
                            Err(_)  => notice = Some("Copy failed.".into()),
                        }
                    }
                }
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
                (KeyCode::Esc, _) if focus == Focus::Preview => focus = Focus::Snippets,
                (KeyCode::Esc, _) if focus == Focus::Snippets => focus = Focus::Groups,
                (KeyCode::Char('/'), _) => focus = Focus::Search,
                (KeyCode::Tab, _) => {
                    focus = match focus {
                        Focus::Groups   => Focus::Snippets,
                        Focus::Snippets => Focus::Preview,
                        Focus::Preview  => Focus::Groups,
                        Focus::Search   => Focus::Snippets,
                    };
                }
                (KeyCode::Right, _) | (KeyCode::Enter, _) if focus == Focus::Groups => {
                    if !visible.is_empty() {
                        focus = Focus::Snippets;
                    }
                }
                (KeyCode::Right, _) if focus == Focus::Snippets => {
                    if !visible.is_empty() {
                        focus = Focus::Preview;
                    }
                }
                (KeyCode::Left, _) if focus == Focus::Preview => focus = Focus::Snippets,
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
                        preview_cmd_sel = 0;
                        group_total = count_in_group(&all, &groups[group_sel].0);
                    }
                }
                (KeyCode::Down, _) if focus == Focus::Groups => {
                    if group_sel + 1 < groups.len() {
                        group_sel += 1;
                        visible = compute_visible(&all, &composites, &groups[group_sel].0, &query, &matcher);
                        snippet_sel = 0;
                        preview_cmd_sel = 0;
                        group_total = count_in_group(&all, &groups[group_sel].0);
                    }
                }
                (KeyCode::Up, _) if focus == Focus::Snippets => {
                    snippet_sel = snippet_sel.saturating_sub(1);
                    preview_cmd_sel = 0;
                }
                (KeyCode::Down, _) if focus == Focus::Snippets => {
                    if snippet_sel + 1 < visible.len() {
                        snippet_sel += 1;
                        preview_cmd_sel = 0;
                    }
                }
                (KeyCode::Up, _) if focus == Focus::Preview => {
                    preview_cmd_sel = preview_cmd_sel.saturating_sub(1);
                }
                (KeyCode::Down, _) if focus == Focus::Preview => {
                    if let Some(&ai) = visible.get(snippet_sel)
                        && preview_cmd_sel + 1 < all[ai].commands.len()
                    {
                        preview_cmd_sel += 1;
                    }
                }
                (KeyCode::Char('e'), _) if focus == Focus::Snippets => {
                    if let Some(&ai) = visible.get(snippet_sel) {
                        let form_state = FormState::from_snippet(&all[ai]);
                        if let Some(result) = save_form::run_loop(&mut terminal, form_state, true)? {
                            let mut updated = all[ai].clone();
                            updated.name = result.name;
                            updated.group_name = result.group;
                            updated.commands = result.commands;
                            match ops::update_snippet(conn, &updated) {
                                Ok(()) => {
                                    all[ai] = updated;
                                    composites[ai] = composite(&all[ai]);
                                    let current_filter = groups[group_sel].0.clone();
                                    (groups, group_sel, visible) = refresh(&all, &composites, &query, &matcher, &current_filter);
                                    snippet_sel = snippet_sel.min(visible.len().saturating_sub(1));
                                    group_total = count_in_group(&all, &groups[group_sel].0);
                                    notice = Some("Saved.".into());
                                }
                                Err(e) => {
                                    notice = Some(format!("Error: {}", e));
                                }
                            }
                        }
                    }
                }
                (KeyCode::Char('d'), m)
                    if focus == Focus::Groups && !m.contains(KeyModifiers::CONTROL) =>
                {
                    pending_group_delete = true;
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

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
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

fn command_lines(num: usize, cmd: &str, text_w: usize, highlighted: bool) -> Vec<Line<'static>> {
    let prefix = if highlighted {
        format!("▶ {:>2}  ", num)
    } else {
        format!("  {:>2}  ", num)
    };
    let indent = " ".repeat(prefix.len());
    let text_w = text_w.max(1);
    let (num_color, text_color) = if highlighted {
        (TN_YELLOW, Color::White)
    } else {
        (TN_MUTED, Color::Reset)
    };

    let chars: Vec<char> = cmd.chars().collect();
    if chars.is_empty() {
        return vec![Line::from(Span::styled(prefix, Style::default().fg(num_color)))];
    }

    let mut result = Vec::new();
    let mut start = 0;
    let mut first = true;

    while start < chars.len() {
        let end = (start + text_w).min(chars.len());
        let chunk: String = chars[start..end].iter().collect();

        if first {
            let mut spans = vec![Span::styled(prefix.clone(), Style::default().fg(num_color))];
            for (i, part) in chunk.split(" | ").enumerate() {
                if i > 0 {
                    spans.push(Span::styled(" | ", Style::default().fg(TN_ORANGE)));
                }
                spans.push(Span::styled(part.to_string(), Style::default().fg(text_color)));
            }
            result.push(Line::from(spans));
            first = false;
        } else {
            result.push(Line::from(vec![
                Span::styled(indent.clone(), Style::default().fg(num_color)),
                Span::styled(chunk, Style::default().fg(text_color)),
            ]));
        }

        start = end;
    }
    result
}
