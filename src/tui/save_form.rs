use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
    Terminal,
};
use std::io;

use super::TerminalGuard;
use super::colors::{TN_BLUE, TN_DIM, TN_GREEN, TN_MUTED, TN_RED, TN_YELLOW};
use crate::cli::RESERVED;

const MAX_NAME_LEN: usize = 200;
const MAX_CMD_LEN: usize = 10_000;

fn active_colors(active: bool) -> (Color, Color) {
    if active { (TN_YELLOW, TN_YELLOW) } else { (TN_DIM, TN_MUTED) }
}

#[derive(Debug)]
pub struct SaveFormResult {
    pub group: String,
    pub name: String,
    pub commands: Vec<String>,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub(crate) enum Field {
    Group,
    Name,
    Commands,
}

impl Field {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Group    => Self::Name,
            Self::Name     => Self::Commands,
            Self::Commands => Self::Group,
        }
    }

    pub(crate) fn prev(self) -> Self {
        match self {
            Self::Group    => Self::Commands,
            Self::Name     => Self::Group,
            Self::Commands => Self::Name,
        }
    }
}

pub(crate) struct FormState {
    pub(crate) group: String,
    pub(crate) name: String,
    pub(crate) commands: Vec<String>,
    pub(crate) cmd_sel: usize,
    pub(crate) focus: Field,
    pub(crate) error: Option<String>,
}

impl FormState {
    pub(crate) fn new(prefill_command: Option<String>) -> Self {
        Self {
            group: String::new(),
            name: String::new(),
            commands: vec![prefill_command.unwrap_or_default()],
            cmd_sel: 0,
            focus: Field::Group,
            error: None,
        }
    }

    pub(crate) fn current_cmd_mut(&mut self) -> &mut String {
        &mut self.commands[self.cmd_sel]
    }

    pub(crate) fn active_field_mut(&mut self) -> Option<&mut String> {
        match self.focus {
            Field::Group    => Some(&mut self.group),
            Field::Name     => Some(&mut self.name),
            Field::Commands => None,
        }
    }

    pub(crate) fn add_command(&mut self) {
        self.cmd_sel += 1;
        self.commands.insert(self.cmd_sel, String::new());
    }

    pub(crate) fn remove_command(&mut self) {
        if self.commands.len() > 1 {
            self.commands.remove(self.cmd_sel);
            if self.cmd_sel >= self.commands.len() {
                self.cmd_sel = self.commands.len() - 1;
            }
        }
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        let name = self.name.trim();
        let group = self.group.trim();

        if name.is_empty() {
            return Err("Name cannot be empty".into());
        }
        if name.len() > MAX_NAME_LEN {
            return Err(format!("Name must be {} characters or fewer", MAX_NAME_LEN));
        }
        if RESERVED.contains(&name) {
            return Err(format!("'{}' is a reserved word", name));
        }
        if !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return Err("Name: letters, numbers, - and _ only".into());
        }
        if !group.is_empty() {
            if group.len() > MAX_NAME_LEN {
                return Err(format!("Group must be {} characters or fewer", MAX_NAME_LEN));
            }
            if RESERVED.contains(&group) {
                return Err(format!("'{}' is a reserved word", group));
            }
            if !group.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
                return Err("Group: letters, numbers, - and _ only".into());
            }
        }
        let non_empty: Vec<_> = self.commands.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        if non_empty.is_empty() {
            return Err("At least one command is required".into());
        }
        for cmd in &non_empty {
            if cmd.len() > MAX_CMD_LEN {
                return Err(format!("Command exceeds {} character limit", MAX_CMD_LEN));
            }
        }
        Ok(())
    }

    pub(crate) fn submit(&self) -> SaveFormResult {
        let commands = self.commands
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        SaveFormResult {
            group: self.group.trim().to_string(),
            name: self.name.trim().to_string(),
            commands,
        }
    }
}

pub fn run(prefill: Option<String>) -> Result<Option<SaveFormResult>> {
    let _guard = TerminalGuard::new()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut state = FormState::new(prefill);
    let mut needs_draw = true;

    loop {
        if needs_draw {
            terminal.draw(|f| draw(f, &state))?;
            needs_draw = false;
        }

        let ev = event::read()?;
        if matches!(ev, Event::Resize(..)) {
            needs_draw = true;
            continue;
        }
        let Event::Key(key) = ev else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        state.error = None;
        needs_draw = true;

        match key.code {
            KeyCode::Esc => return Ok(None),

            KeyCode::Tab => {
                state.focus = state.focus.next();
            }
            KeyCode::BackTab => {
                state.focus = state.focus.prev();
            }

            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                match state.validate() {
                    Ok(()) => return Ok(Some(state.submit())),
                    Err(e) => state.error = Some(e),
                }
            }

            KeyCode::Up if state.focus == Field::Commands => {
                if state.cmd_sel > 0 { state.cmd_sel -= 1; } else { needs_draw = false; }
            }
            KeyCode::Down if state.focus == Field::Commands => {
                if state.cmd_sel + 1 < state.commands.len() { state.cmd_sel += 1; } else { needs_draw = false; }
            }

            // Ctrl+N — add new command line below current
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                state.focus = Field::Commands;
                state.add_command();
            }

            // Ctrl+D — remove current command line
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if state.focus == Field::Commands {
                    state.remove_command();
                }
            }

            // Enter: advance field (submit on Commands when it's the only/last interaction point)
            KeyCode::Enter => {
                match state.validate() {
                    Ok(()) => return Ok(Some(state.submit())),
                    Err(e) => state.error = Some(e),
                }
            }

            KeyCode::Backspace => {
                let changed = if state.focus == Field::Commands {
                    state.current_cmd_mut().pop().is_some()
                } else if let Some(f) = state.active_field_mut() {
                    f.pop().is_some()
                } else {
                    false
                };
                if !changed { needs_draw = false; }
            }

            KeyCode::Char(c) => {
                if state.focus == Field::Commands {
                    state.current_cmd_mut().push(c);
                } else if let Some(f) = state.active_field_mut() {
                    f.push(c);
                }
            }

            _ => { needs_draw = false; }
        }
    }
}

fn draw(f: &mut ratatui::Frame, state: &FormState) {
    let area = f.area();

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(TN_DIM))
        .title(Span::styled(
            " tet — save snippet ",
            Style::default().fg(TN_BLUE).add_modifier(Modifier::BOLD),
        ));
    f.render_widget(&outer, area);
    let inner = outer.inner(area);

    let form_width = 70u16.min(inner.width);
    // Inner text width per command: form minus 2 borders, 2-char prefix ("▶ "), 1 left pad
    let cmd_text_w = form_width.saturating_sub(5) as usize;
    // Sum visual rows each command will occupy when wrapped, cap total at 12
    let cmd_content_h: u16 = state.commands.iter()
        .map(|cmd| {
            let chars = cmd.chars().count().max(1);
            ((chars + cmd_text_w - 1) / cmd_text_w).max(1) as u16
        })
        .sum::<u16>()
        .clamp(1, 12);
    let cmd_box_h = cmd_content_h + 2; // borders
    let form_height = 3 + 1 + 3 + 1 + cmd_box_h + 1 + 2; // group+gap+name+gap+cmds+gap+status
    let vpad = inner.height.saturating_sub(form_height) / 2;

    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(vpad),
            Constraint::Length(form_height),
            Constraint::Min(0),
        ])
        .split(inner);
    let hpad = inner.width.saturating_sub(form_width) / 2;
    let horiz = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(hpad),
            Constraint::Length(form_width),
            Constraint::Min(0),
        ])
        .split(vert[1]);
    let col = horiz[1];

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),         // Group
            Constraint::Length(1),
            Constraint::Length(3),         // Name
            Constraint::Length(1),
            Constraint::Length(cmd_box_h), // Commands
            Constraint::Length(1),
            Constraint::Min(2),            // status
        ])
        .split(col);

    render_field(f, rows[0], "Group  (optional)", &state.group, state.focus == Field::Group);
    render_field(f, rows[2], "Name", &state.name, state.focus == Field::Name);
    render_commands(f, rows[4], state);
    render_status(f, rows[6], state);
}

fn render_field(f: &mut ratatui::Frame, area: Rect, label: &str, value: &str, active: bool) {
    let (border_color, title_color) = active_colors(active);
    let cursor = if active { "▌" } else { "" };

    let para = Paragraph::new(format!(" {}{}", value, cursor))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(if active { BorderType::Rounded } else { BorderType::Plain })
                .border_style(Style::default().fg(border_color))
                .title(Span::styled(
                    format!(" {} ", label),
                    Style::default().fg(title_color)
                        .add_modifier(if active { Modifier::BOLD } else { Modifier::empty() }),
                )),
        )
        .style(Style::default().fg(if active { Color::White } else { TN_MUTED }));

    f.render_widget(para, area);
}

fn render_commands(f: &mut ratatui::Frame, area: Rect, state: &FormState) {
    let active = state.focus == Field::Commands;
    let (border_color, title_color) = active_colors(active);

    let count = state.commands.len();
    let title_suffix = if count > 1 { format!(" ({} commands) ", count) } else { String::new() };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(if active { BorderType::Rounded } else { BorderType::Plain })
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            format!(" Commands{}", title_suffix),
            Style::default().fg(title_color)
                .add_modifier(if active { Modifier::BOLD } else { Modifier::empty() }),
        ));

    let inner = block.inner(area);
    f.render_widget(&block, area);

    let lines: Vec<Line> = state.commands.iter().enumerate().map(|(i, cmd)| {
        let is_sel = active && i == state.cmd_sel;
        let cursor = if is_sel { "▌" } else { "" };
        let prefix = if is_sel { "▶ " } else { "  " };
        Line::from(vec![
            Span::styled(prefix, Style::default().fg(if is_sel { TN_YELLOW } else { TN_DIM })),
            Span::styled(
                format!("{}{}", cmd, cursor),
                Style::default().fg(if is_sel { Color::White } else { TN_MUTED }),
            ),
        ])
    }).collect();

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn render_status(f: &mut ratatui::Frame, area: Rect, state: &FormState) {
    let line = if let Some(err) = &state.error {
        Line::from(vec![
            Span::styled(" ✖  ", Style::default().fg(TN_RED).add_modifier(Modifier::BOLD)),
            Span::styled(err.as_str(), Style::default().fg(TN_RED)),
        ])
    } else {
        let mut spans = vec![
            Span::styled("Tab", Style::default().fg(TN_GREEN).add_modifier(Modifier::BOLD)),
            Span::styled(" field  ", Style::default().fg(TN_MUTED)),
            Span::styled("Ctrl+S", Style::default().fg(TN_GREEN).add_modifier(Modifier::BOLD)),
            Span::styled(" save  ", Style::default().fg(TN_MUTED)),
            Span::styled("Esc", Style::default().fg(TN_MUTED).add_modifier(Modifier::BOLD)),
            Span::styled(" cancel", Style::default().fg(TN_MUTED)),
        ];
        if state.focus == Field::Commands {
            spans.extend([
                Span::styled("  ↑↓", Style::default().fg(TN_GREEN).add_modifier(Modifier::BOLD)),
                Span::styled(" select  ", Style::default().fg(TN_MUTED)),
                Span::styled("Ctrl+N", Style::default().fg(TN_GREEN).add_modifier(Modifier::BOLD)),
                Span::styled(" add  ", Style::default().fg(TN_MUTED)),
                Span::styled("Ctrl+D", Style::default().fg(TN_MUTED).add_modifier(Modifier::BOLD)),
                Span::styled(" remove", Style::default().fg(TN_MUTED)),
            ]);
        }
        Line::from(spans)
    };

    f.render_widget(Paragraph::new(line), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(group: &str, name: &str, commands: &[&str]) -> FormState {
        FormState {
            group: group.into(),
            name: name.into(),
            commands: commands.iter().map(|s| s.to_string()).collect(),
            cmd_sel: 0,
            focus: Field::Group,
            error: None,
        }
    }

    // ── Field cycling ─────────────────────────────────────────────────────────

    #[test]
    fn field_next_cycles_forward() {
        assert_eq!(Field::Group.next(),    Field::Name);
        assert_eq!(Field::Name.next(),     Field::Commands);
        assert_eq!(Field::Commands.next(), Field::Group);
    }

    #[test]
    fn field_prev_cycles_backward() {
        assert_eq!(Field::Commands.prev(), Field::Name);
        assert_eq!(Field::Name.prev(),     Field::Group);
        assert_eq!(Field::Group.prev(),    Field::Commands);
    }

    // ── FormState::new ────────────────────────────────────────────────────────

    #[test]
    fn new_without_prefill_starts_with_one_empty_command() {
        let s = FormState::new(None);
        assert_eq!(s.commands, vec![""]);
        assert_eq!(s.cmd_sel, 0);
        assert_eq!(s.focus, Field::Group);
    }

    #[test]
    fn new_with_prefill_sets_first_command() {
        let s = FormState::new(Some("ping 8.8.8.8".into()));
        assert_eq!(s.commands[0], "ping 8.8.8.8");
        assert!(s.group.is_empty());
        assert!(s.name.is_empty());
    }

    // ── add / remove commands ─────────────────────────────────────────────────

    #[test]
    fn add_command_inserts_after_current() {
        let mut s = state("", "snap", &["echo a", "echo c"]);
        s.cmd_sel = 0;
        s.add_command();
        assert_eq!(s.commands, vec!["echo a", "", "echo c"]);
        assert_eq!(s.cmd_sel, 1);
    }

    #[test]
    fn add_command_at_end_appends() {
        let mut s = state("", "snap", &["echo a"]);
        s.add_command();
        assert_eq!(s.commands.len(), 2);
        assert_eq!(s.cmd_sel, 1);
    }

    #[test]
    fn remove_command_removes_current() {
        let mut s = state("", "snap", &["echo a", "echo b", "echo c"]);
        s.cmd_sel = 1;
        s.remove_command();
        assert_eq!(s.commands, vec!["echo a", "echo c"]);
        assert_eq!(s.cmd_sel, 1);
    }

    #[test]
    fn remove_last_when_selected_clamps_index() {
        let mut s = state("", "snap", &["echo a", "echo b"]);
        s.cmd_sel = 1;
        s.remove_command();
        assert_eq!(s.commands, vec!["echo a"]);
        assert_eq!(s.cmd_sel, 0);
    }

    #[test]
    fn remove_does_nothing_when_only_one_command() {
        let mut s = state("", "snap", &["echo a"]);
        s.remove_command();
        assert_eq!(s.commands.len(), 1);
    }

    // ── active_field_mut ──────────────────────────────────────────────────────

    #[test]
    fn active_field_mut_group_focus() {
        let mut s = FormState::new(None);
        s.focus = Field::Group;
        s.active_field_mut().unwrap().push_str("home");
        assert_eq!(s.group, "home");
    }

    #[test]
    fn active_field_mut_name_focus() {
        let mut s = FormState::new(None);
        s.focus = Field::Name;
        s.active_field_mut().unwrap().push_str("ping");
        assert_eq!(s.name, "ping");
    }

    #[test]
    fn active_field_mut_commands_returns_none() {
        let mut s = FormState::new(None);
        s.focus = Field::Commands;
        assert!(s.active_field_mut().is_none());
    }

    // ── validate: valid inputs ────────────────────────────────────────────────

    #[test]
    fn validate_ok_ungrouped_single_command() {
        assert!(state("", "ping", &["ping 8.8.8.8"]).validate().is_ok());
    }

    #[test]
    fn validate_ok_with_group() {
        assert!(state("home", "dns", &["nslookup google.com"]).validate().is_ok());
    }

    #[test]
    fn validate_ok_multiple_commands() {
        assert!(state("", "deploy", &["cargo build", "cargo test", "cargo run"]).validate().is_ok());
    }

    #[test]
    fn validate_ok_name_with_hyphen_and_underscore() {
        assert!(state("", "my-cmd_1", &["echo hi"]).validate().is_ok());
    }

    #[test]
    fn validate_ok_group_with_hyphen_and_underscore() {
        assert!(state("my-group_1", "snap", &["echo hi"]).validate().is_ok());
    }

    #[test]
    fn validate_trims_whitespace_before_checking() {
        assert!(state("  home  ", "  ping  ", &["  ping 8.8.8.8  "]).validate().is_ok());
    }

    // ── validate: command errors ──────────────────────────────────────────────

    #[test]
    fn validate_error_all_commands_empty() {
        let err = state("", "ping", &[""]).validate().unwrap_err();
        assert!(err.contains("required"), "got: {err}");
    }

    #[test]
    fn validate_error_all_commands_whitespace_only() {
        let err = state("", "ping", &["  ", "\t"]).validate().unwrap_err();
        assert!(err.contains("required"), "got: {err}");
    }

    #[test]
    fn validate_ok_some_commands_blank_others_not() {
        // blank lines between real commands are filtered on submit — validate should pass
        assert!(state("", "ping", &["", "ping 8.8.8.8", ""]).validate().is_ok());
    }

    // ── validate: name errors ─────────────────────────────────────────────────

    #[test]
    fn validate_error_empty_name() {
        let err = state("", "", &["echo hi"]).validate().unwrap_err();
        assert!(err.contains("empty"), "got: {err}");
    }

    #[test]
    fn validate_error_name_with_space() {
        let err = state("", "my cmd", &["echo hi"]).validate().unwrap_err();
        assert!(err.contains("letters"), "got: {err}");
    }

    #[test]
    fn validate_error_name_with_slash() {
        let err = state("", "a/b", &["echo hi"]).validate().unwrap_err();
        assert!(err.contains("letters"), "got: {err}");
    }

    #[test]
    fn validate_error_reserved_name() {
        for &word in crate::cli::RESERVED {
            let err = state("", word, &["echo hi"]).validate().unwrap_err();
            assert!(err.contains("reserved"), "'{word}' should be rejected; got: {err}");
        }
    }

    // ── validate: group errors ────────────────────────────────────────────────

    #[test]
    fn validate_error_group_with_space() {
        let err = state("my group", "snap", &["echo hi"]).validate().unwrap_err();
        assert!(err.contains("letters"), "got: {err}");
    }

    #[test]
    fn validate_error_reserved_group() {
        let err = state("save", "snap", &["echo hi"]).validate().unwrap_err();
        assert!(err.contains("reserved"), "got: {err}");
    }

    // ── submit ────────────────────────────────────────────────────────────────

    #[test]
    fn submit_single_command() {
        let r = state("home", "ping", &["ping 8.8.8.8"]).submit();
        assert_eq!(r.commands, vec!["ping 8.8.8.8"]);
        assert_eq!(r.name, "ping");
        assert_eq!(r.group, "home");
    }

    #[test]
    fn submit_multiple_commands_preserved() {
        let r = state("", "deploy", &["cargo build", "cargo test"]).submit();
        assert_eq!(r.commands, vec!["cargo build", "cargo test"]);
    }

    #[test]
    fn submit_filters_blank_commands() {
        let r = state("", "multi", &["echo a", "", "echo b"]).submit();
        assert_eq!(r.commands, vec!["echo a", "echo b"]);
    }

    #[test]
    fn submit_trims_each_command() {
        let r = state("", "multi", &["  echo a  ", "  echo b  "]).submit();
        assert_eq!(r.commands, vec!["echo a", "echo b"]);
    }

    #[test]
    fn submit_trims_name_and_group() {
        let r = state("  home  ", "  ping  ", &["echo hi"]).submit();
        assert_eq!(r.name, "ping");
        assert_eq!(r.group, "home");
    }

    #[test]
    fn submit_empty_group_stays_empty() {
        let r = state("", "ping", &["echo hi"]).submit();
        assert_eq!(r.group, "");
    }
}
