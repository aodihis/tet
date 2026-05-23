use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Terminal,
};
use std::io;

use super::TerminalGuard;
use super::colors::{TN_BLUE, TN_DIM, TN_GREEN, TN_MUTED, TN_RED, TN_YELLOW};
use crate::cli::RESERVED;

const MAX_GROUP_LEN: usize = 50;
const MAX_NAME_LEN: usize = 50;
const MAX_CMD_LEN: usize = 10_000;

fn active_colors(active: bool) -> (Color, Color) {
    if active { (TN_YELLOW, TN_YELLOW) } else { (TN_DIM, TN_MUTED) }
}

fn char_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map(|(b, _)| b).unwrap_or(s.len())
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
    pub(crate) group_cursor: usize,
    pub(crate) name: String,
    pub(crate) name_cursor: usize,
    pub(crate) commands: Vec<String>,
    pub(crate) cmd_cursors: Vec<usize>,
    pub(crate) cmd_sel: usize,
    pub(crate) focus: Field,
    pub(crate) error: Option<String>,
}

impl FormState {
    pub(crate) fn new(prefill_command: Option<String>) -> Self {
        let cmd = prefill_command.unwrap_or_default();
        let cursor = cmd.chars().count();
        Self {
            group: String::new(),
            group_cursor: 0,
            name: String::new(),
            name_cursor: 0,
            commands: vec![cmd],
            cmd_cursors: vec![cursor],
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
        self.cmd_cursors.insert(self.cmd_sel, 0);
    }

    pub(crate) fn remove_command(&mut self) {
        if self.commands.len() > 1 {
            self.commands.remove(self.cmd_sel);
            self.cmd_cursors.remove(self.cmd_sel);
            if self.cmd_sel >= self.commands.len() {
                self.cmd_sel = self.commands.len() - 1;
            }
        }
    }

    pub(crate) fn move_cursor_left(&mut self) -> bool {
        match self.focus {
            Field::Group => {
                if self.group_cursor > 0 { self.group_cursor -= 1; true } else { false }
            }
            Field::Name => {
                if self.name_cursor > 0 { self.name_cursor -= 1; true } else { false }
            }
            Field::Commands => {
                let cur = &mut self.cmd_cursors[self.cmd_sel];
                if *cur > 0 { *cur -= 1; true } else { false }
            }
        }
    }

    pub(crate) fn move_cursor_right(&mut self) -> bool {
        match self.focus {
            Field::Group => {
                let len = self.group.chars().count();
                if self.group_cursor < len { self.group_cursor += 1; true } else { false }
            }
            Field::Name => {
                let len = self.name.chars().count();
                if self.name_cursor < len { self.name_cursor += 1; true } else { false }
            }
            Field::Commands => {
                let len = self.commands[self.cmd_sel].chars().count();
                let cur = &mut self.cmd_cursors[self.cmd_sel];
                if *cur < len { *cur += 1; true } else { false }
            }
        }
    }

    pub(crate) fn insert_char(&mut self, c: char) {
        match self.focus {
            Field::Group => {
                if self.group.chars().count() < MAX_GROUP_LEN {
                    let byte = char_to_byte(&self.group, self.group_cursor);
                    self.group.insert(byte, c);
                    self.group_cursor += 1;
                }
            }
            Field::Name => {
                if self.name.chars().count() < MAX_NAME_LEN {
                    let byte = char_to_byte(&self.name, self.name_cursor);
                    self.name.insert(byte, c);
                    self.name_cursor += 1;
                }
            }
            Field::Commands => {
                let i = self.cmd_sel;
                if self.commands[i].chars().count() < MAX_CMD_LEN {
                    let byte = char_to_byte(&self.commands[i], self.cmd_cursors[i]);
                    self.commands[i].insert(byte, c);
                    self.cmd_cursors[i] += 1;
                }
            }
        }
    }

    pub(crate) fn delete_before_cursor(&mut self) -> bool {
        match self.focus {
            Field::Group => {
                if self.group_cursor > 0 {
                    self.group_cursor -= 1;
                    let byte = char_to_byte(&self.group, self.group_cursor);
                    self.group.remove(byte);
                    true
                } else { false }
            }
            Field::Name => {
                if self.name_cursor > 0 {
                    self.name_cursor -= 1;
                    let byte = char_to_byte(&self.name, self.name_cursor);
                    self.name.remove(byte);
                    true
                } else { false }
            }
            Field::Commands => {
                let i = self.cmd_sel;
                if self.cmd_cursors[i] > 0 {
                    self.cmd_cursors[i] -= 1;
                    let byte = char_to_byte(&self.commands[i], self.cmd_cursors[i]);
                    self.commands[i].remove(byte);
                    true
                } else { false }
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
            if group.len() > MAX_GROUP_LEN {
                return Err(format!("Group must be {} characters or fewer", MAX_GROUP_LEN));
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

            KeyCode::Left => {
                if !state.move_cursor_left() { needs_draw = false; }
            }
            KeyCode::Right => {
                if !state.move_cursor_right() { needs_draw = false; }
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

            KeyCode::Enter => {
                match state.validate() {
                    Ok(()) => return Ok(Some(state.submit())),
                    Err(e) => state.error = Some(e),
                }
            }

            KeyCode::Backspace => {
                if !state.delete_before_cursor() { needs_draw = false; }
            }

            KeyCode::Char(c) => {
                state.insert_char(c);
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

    // left col: Group + Name stacked; right col: Commands — use full available width
    let form_width = inner.width;
    let left_w = 28u16.min(form_width * 2 / 5);
    let right_w = form_width.saturating_sub(left_w + 2);

    let cmd_text_w = right_w.saturating_sub(4).max(1) as usize;
    let cmd_content_h: u16 = state.commands.iter()
        .map(|cmd| {
            let n = cmd.chars().count().max(1);
            ((n + cmd_text_w - 1) / cmd_text_w).max(1) as u16
        })
        .sum::<u16>()
        .clamp(1, 12);

    // col_h: both columns share the same height; at least tall enough for Group+gap+Name
    let col_h = (cmd_content_h + 2).max(7); // 7 = Group(3) + gap(1) + Name(3)
    let form_height = col_h + 1 + 2;         // col + gap + status
    let vpad = inner.height.saturating_sub(form_height) / 2;

    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(vpad),
            Constraint::Length(form_height),
            Constraint::Min(0),
        ])
        .split(inner);
    let form_area = vert[1];

    let form_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(col_h),
            Constraint::Length(1),
            Constraint::Min(2),
        ])
        .split(form_area);

    let form_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(left_w),
            Constraint::Length(2),
            Constraint::Length(right_w),
        ])
        .split(form_rows[0]);

    // Left column: Group at top, Name below, blank remainder
    let left_col = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(form_cols[0]);

    render_field(f, left_col[0], "Group", &state.group, state.focus == Field::Group, state.group_cursor, MAX_GROUP_LEN);
    render_field(f, left_col[2], "Name",  &state.name,  state.focus == Field::Name,  state.name_cursor,  MAX_NAME_LEN);
    render_commands(f, form_cols[2], state);
    render_status(f, form_rows[2], state);
}

fn render_field(f: &mut ratatui::Frame, area: Rect, label: &str, value: &str, active: bool, cursor_pos: usize, max_len: usize) {
    let (border_color, title_color) = active_colors(active);
    let inner_w = area.width.saturating_sub(2) as usize;
    let h_scroll = (cursor_pos + 2).saturating_sub(inner_w) as u16;

    let title = if active {
        format!(" {}  {}/{} ", label, value.chars().count(), max_len)
    } else {
        format!(" {} ", label)
    };

    let line = if active {
        let byte = char_to_byte(value, cursor_pos);
        Line::from(vec![
            Span::raw(format!(" {}", &value[..byte])),
            Span::styled("▌", Style::default().fg(TN_YELLOW)),
            Span::raw(value[byte..].to_owned()),
        ])
    } else {
        Line::from(format!(" {}", value))
    };

    let para = Paragraph::new(line)
        .scroll((0, h_scroll))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(if active { BorderType::Rounded } else { BorderType::Plain })
                .border_style(Style::default().fg(border_color))
                .title(Span::styled(
                    title,
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

    let text_w = inner.width.saturating_sub(2).max(1) as usize;

    // Visual row count per command
    let cmd_heights: Vec<usize> = state.commands.iter()
        .map(|cmd| {
            let n = cmd.chars().count().max(1);
            (n + text_w - 1) / text_w
        })
        .collect();

    // Scroll to keep selected command visible
    let sel_start: usize = cmd_heights[..state.cmd_sel].iter().sum();
    let avail_h = inner.height as usize;
    let total_h: usize = cmd_heights.iter().sum();
    let scroll: u16 = if total_h <= avail_h {
        0
    } else {
        sel_start.min(total_h.saturating_sub(avail_h)) as u16
    };

    let mut lines: Vec<Line> = Vec::new();
    for (i, cmd) in state.commands.iter().enumerate() {
        let is_sel = active && i == state.cmd_sel;
        let cmd_h = cmd_heights[i];
        let chars: Vec<char> = cmd.chars().collect();
        // Which wrapped row contains the cursor (clamped to last row)
        let cursor_row = if is_sel {
            (state.cmd_cursors[i] / text_w).min(cmd_h.saturating_sub(1))
        } else {
            usize::MAX
        };

        for row in 0..cmd_h {
            let row_start = row * text_w;
            let row_end = (row_start + text_w).min(chars.len());
            let prefix = if row == 0 && is_sel { "▶ " } else { "  " };

            let line = if row == cursor_row {
                let cursor_in_row = (state.cmd_cursors[i] - row_start).min(row_end - row_start);
                let before: String = chars[row_start..row_start + cursor_in_row].iter().collect();
                let after: String = chars[row_start + cursor_in_row..row_end].iter().collect();
                Line::from(vec![
                    Span::styled(prefix.to_string(), Style::default().fg(TN_YELLOW)),
                    Span::styled(before, Style::default().fg(Color::White)),
                    Span::styled("▌".to_string(), Style::default().fg(TN_YELLOW)),
                    Span::styled(after, Style::default().fg(Color::White)),
                ])
            } else {
                let chunk: String = chars[row_start..row_end].iter().collect();
                Line::from(vec![
                    Span::styled(prefix.to_string(), Style::default().fg(if is_sel { TN_YELLOW } else { TN_DIM })),
                    Span::styled(chunk, Style::default().fg(if is_sel { Color::White } else { TN_MUTED })),
                ])
            };
            lines.push(line);
        }
    }

    f.render_widget(Paragraph::new(lines).scroll((scroll, 0)), inner);
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
        let cmd_cursors = commands.iter().map(|s| s.chars().count()).collect();
        FormState {
            group: group.into(),
            group_cursor: group.chars().count(),
            name: name.into(),
            name_cursor: name.chars().count(),
            commands: commands.iter().map(|s| s.to_string()).collect(),
            cmd_cursors,
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
        assert_eq!(s.cmd_cursors, vec![0]);
        assert_eq!(s.focus, Field::Group);
    }

    #[test]
    fn new_with_prefill_sets_first_command() {
        let s = FormState::new(Some("ping 8.8.8.8".into()));
        assert_eq!(s.commands[0], "ping 8.8.8.8");
        assert_eq!(s.cmd_cursors[0], 12);
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
        assert_eq!(s.cmd_cursors[1], 0);
    }

    #[test]
    fn add_command_at_end_appends() {
        let mut s = state("", "snap", &["echo a"]);
        s.add_command();
        assert_eq!(s.commands.len(), 2);
        assert_eq!(s.cmd_cursors.len(), 2);
        assert_eq!(s.cmd_sel, 1);
    }

    #[test]
    fn remove_command_removes_current() {
        let mut s = state("", "snap", &["echo a", "echo b", "echo c"]);
        s.cmd_sel = 1;
        s.remove_command();
        assert_eq!(s.commands, vec!["echo a", "echo c"]);
        assert_eq!(s.cmd_cursors.len(), 2);
        assert_eq!(s.cmd_sel, 1);
    }

    #[test]
    fn remove_last_when_selected_clamps_index() {
        let mut s = state("", "snap", &["echo a", "echo b"]);
        s.cmd_sel = 1;
        s.remove_command();
        assert_eq!(s.commands, vec!["echo a"]);
        assert_eq!(s.cmd_cursors.len(), 1);
        assert_eq!(s.cmd_sel, 0);
    }

    #[test]
    fn remove_does_nothing_when_only_one_command() {
        let mut s = state("", "snap", &["echo a"]);
        s.remove_command();
        assert_eq!(s.commands.len(), 1);
        assert_eq!(s.cmd_cursors.len(), 1);
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

    // ── cursor operations ─────────────────────────────────────────────────────

    #[test]
    fn insert_char_advances_cursor() {
        let mut s = FormState::new(None);
        s.focus = Field::Commands;
        s.insert_char('a');
        s.insert_char('b');
        assert_eq!(s.commands[0], "ab");
        assert_eq!(s.cmd_cursors[0], 2);
    }

    #[test]
    fn insert_char_at_middle_inserts_correctly() {
        let mut s = FormState::new(Some("ac".into()));
        s.focus = Field::Commands;
        s.cmd_cursors[0] = 1; // between 'a' and 'c'
        s.insert_char('b');
        assert_eq!(s.commands[0], "abc");
        assert_eq!(s.cmd_cursors[0], 2);
    }

    #[test]
    fn delete_before_cursor_removes_char() {
        let mut s = FormState::new(Some("ab".into()));
        s.focus = Field::Commands;
        assert!(s.delete_before_cursor());
        assert_eq!(s.commands[0], "a");
        assert_eq!(s.cmd_cursors[0], 1);
    }

    #[test]
    fn delete_before_cursor_at_start_returns_false() {
        let mut s = FormState::new(None);
        s.focus = Field::Commands;
        assert!(!s.delete_before_cursor());
    }

    #[test]
    fn move_cursor_left_and_right() {
        let mut s = FormState::new(Some("abc".into()));
        s.focus = Field::Commands;
        assert!(s.move_cursor_left());
        assert_eq!(s.cmd_cursors[0], 2);
        assert!(s.move_cursor_right());
        assert_eq!(s.cmd_cursors[0], 3);
        assert!(!s.move_cursor_right()); // at end
    }

    #[test]
    fn cursor_left_at_start_returns_false() {
        let mut s = FormState::new(None);
        s.focus = Field::Group;
        assert!(!s.move_cursor_left());
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
