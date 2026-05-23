use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};
use rusqlite::Connection;
use std::io;

use crate::db::ops;
use crate::models::Snippet;

struct TerminalGuard;

impl TerminalGuard {
    fn new() -> Result<Self> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

pub fn run(conn: &Connection, snippets: Vec<Snippet>) -> Result<Option<Snippet>> {
    let _guard = TerminalGuard::new()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let matcher = SkimMatcherV2::default();
    let mut all = snippets;
    let mut query = String::new();
    let mut filtered = make_filtered(&all, &query, &matcher);
    let mut selected: usize = 0;
    let mut confirm_delete = false;
    let mut list_state = ListState::default();

    loop {
        let (items, display_selected) = build_display(&all, &filtered, selected, confirm_delete);
        list_state.select(if filtered.is_empty() { None } else { Some(display_selected) });

        terminal.draw(|f| {
            let area = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(0),
                    Constraint::Length(1),
                ])
                .split(area);

            let search = Paragraph::new(format!("{}_", query))
                .block(Block::default().borders(Borders::ALL).title(" Search "));
            f.render_widget(search, chunks[0]);

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(" tet "))
                .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                .highlight_symbol("> ");
            f.render_stateful_widget(list, chunks[1], &mut list_state);

            let hint = if confirm_delete {
                " [y] Confirm delete  [n/Esc] Cancel"
            } else {
                " [Enter] Run  [d] Delete  [Esc] Quit"
            };
            let status = Paragraph::new(hint).style(Style::default().fg(Color::DarkGray));
            f.render_widget(status, chunks[2]);
        })?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            if confirm_delete {
                if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                    if let Some(&idx) = filtered.get(selected) {
                        ops::delete_snippet(conn, all[idx].id)?;
                        all.remove(idx);
                        filtered = make_filtered(&all, &query, &matcher);
                        selected = selected.min(filtered.len().saturating_sub(1));
                    }
                }
                confirm_delete = false;
                continue;
            }

            match (key.code, key.modifiers) {
                (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => {
                    return Ok(None);
                }
                (KeyCode::Esc, _) => return Ok(None),
                (KeyCode::Enter, _) => {
                    if let Some(&idx) = filtered.get(selected) {
                        return Ok(Some(all.remove(idx)));
                    }
                }
                (KeyCode::Char('d'), m) if !m.contains(KeyModifiers::CONTROL) => {
                    if !filtered.is_empty() {
                        confirm_delete = true;
                    }
                }
                (KeyCode::Up, _) => {
                    selected = selected.saturating_sub(1);
                }
                (KeyCode::Down, _) => {
                    if selected + 1 < filtered.len() {
                        selected += 1;
                    }
                }
                (KeyCode::Backspace, _) => {
                    query.pop();
                    filtered = make_filtered(&all, &query, &matcher);
                    selected = selected.min(filtered.len().saturating_sub(1));
                }
                (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL) => {
                    query.push(c);
                    filtered = make_filtered(&all, &query, &matcher);
                    selected = 0;
                }
                _ => {}
            }
        }
    }
}

fn composite(s: &Snippet) -> String {
    if s.group_name.is_empty() {
        format!("{} {}", s.name, s.commands.join(" "))
    } else {
        format!("{} {} {}", s.group_name, s.name, s.commands.join(" "))
    }
}

fn make_filtered(all: &[Snippet], query: &str, matcher: &SkimMatcherV2) -> Vec<usize> {
    if query.is_empty() {
        return (0..all.len()).collect();
    }
    let mut scored: Vec<(i64, usize)> = all
        .iter()
        .enumerate()
        .filter_map(|(i, s)| matcher.fuzzy_match(&composite(s), query).map(|score| (score, i)))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, i)| i).collect()
}

fn build_display(
    all: &[Snippet],
    filtered: &[usize],
    selected: usize,
    confirm_delete: bool,
) -> (Vec<ListItem<'static>>, usize) {
    let mut items: Vec<ListItem<'static>> = Vec::new();
    let mut display_selected = 0usize;
    let mut current_group: Option<String> = None;

    for (fi, &ai) in filtered.iter().enumerate() {
        let s = &all[ai];
        let grp = s.group_name.as_str();

        if current_group.as_deref() != Some(grp) {
            let label = if grp.is_empty() { "ungrouped" } else { grp };
            items.push(
                ListItem::new(format!("── {} ──", label))
                    .style(Style::default().fg(Color::Cyan)),
            );
            current_group = Some(grp.to_string());
        }

        if fi == selected {
            display_selected = items.len();
        }

        let cmds = s.commands.join(" | ");
        let style = if fi == selected && confirm_delete {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        items.push(ListItem::new(format!("  {:<20}  {}", s.name, cmds)).style(style));
    }

    (items, display_selected)
}
