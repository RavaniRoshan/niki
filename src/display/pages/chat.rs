//! Conversational chat view for the TUI.
//!
//! Renders the live pipeline as a conversation (one message per agent stage),
//! shows revision notes, and provides a functional input box. It also implements
//! the interactive copy/selection experience (secondary research task):
//!
//! - **Mouse drag → auto-copy on release** (cloud-code behaviour): selecting
//!   text with the mouse copies it to the system clipboard via OSC 52 the moment
//!   you let go — no extra keypress.
//! - **Keyboard copy-mode** (`v`): move with arrows, `Space` sets a mark, `y`
//!   yanks the region, `Esc` cancels.
//! - **Copy a single letter**: in copy-mode, `c` copies the char under the cursor.
//! - **Copy an entire message**: `y` (outside copy-mode) copies the full raw
//!   source of the focused message (not the wrapped view).
//!
//! Clipboard transport is OSC 52 with tmux/screen wrapping detection; this works
//! over SSH and in modern terminals.

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use std::io::Write;

use crate::artifacts::types::AgentRole;
use crate::display::pages::{AppState, Page};
use crate::display::theme;

/// One rendered chat row, with enough metadata to map screen coordinates back
/// to the original source text for accurate copying.
#[derive(Debug, Clone, Default)]
pub struct ChatLine {
    /// Visible text (may be a wrapped slice of the source).
    pub text: String,
    /// Index into the message source list this row belongs to (`usize::MAX` = chrome).
    pub msg_index: usize,
    /// Offset of `text` within that message's source string.
    pub char_start: usize,
    /// True if this row is part of the input box (not copyable as a message).
    pub is_input: bool,
}

fn role_label(role: AgentRole) -> &'static str {
    match role {
        AgentRole::Planner => "Planner",
        AgentRole::Coder => "Coder",
        AgentRole::Tester => "Tester",
        AgentRole::Reviewer => "Reviewer",
        AgentRole::Synthesizer => "Synthesizer",
        AgentRole::SecurityAuditor => "SecurityAuditor",
        AgentRole::Red => "Red",
    }
}

fn role_color(role: AgentRole) -> ratatui::style::Color {
    match role {
        AgentRole::Planner => theme::primary(),
        AgentRole::Coder => theme::accent(),
        AgentRole::Tester => theme::success(),
        AgentRole::Reviewer => theme::warning(),
        AgentRole::Synthesizer => theme::claude(),
        AgentRole::SecurityAuditor => theme::shell(),
        AgentRole::Red => theme::error(),
    }
}

fn role_icon(role: AgentRole) -> &'static str {
    match role {
        AgentRole::Planner => "◆",
        AgentRole::Coder => "◈",
        AgentRole::Tester => "◉",
        AgentRole::Reviewer => "✓",
        AgentRole::Synthesizer => "⚯",
        AgentRole::SecurityAuditor => "⛨",
        AgentRole::Red => "✗",
    }
}

pub struct ChatPage;

impl ChatPage {
    pub fn new() -> Self {
        Self
    }

    /// Build the list of copyable source strings (one per visible message),
    /// indexed by `msg_index` used in [`ChatLine`].
    fn source_texts(state: &AppState) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for (role, text) in state.chat_log.iter() {
            v.push(format!("{}: {}", role, text));
        }
        for s in &state.stages {
            let body = if s.status == crate::display::pages::StageStatus::Running
                && !s.stream.is_empty()
            {
                &s.stream
            } else if !s.summary.is_empty() {
                &s.summary.join("\n")
            } else {
                &s.full_transcript
            };
            v.push(format!("{}: {}", role_label(s.role), body));
        }
        v
    }

    /// Extract the selected substring given an anchor and head in rendered-line
    /// coordinates. Returns the text (may be empty).
    fn selected_text(state: &AppState, a: (usize, usize), b: (usize, usize)) -> String {
        let (r1, c1) = a;
        let (r2, c2) = b;
        let (start_row, start_col, end_row, end_col) = if (r1, c1) <= (r2, c2) {
            (r1, c1, r2, c2)
        } else {
            (r2, c2, r1, c1)
        };
        let mut out = String::new();
        for row in start_row..=end_row.min(state.chat_lines.len().saturating_sub(1)) {
            let line = &state.chat_lines[row];
            if line.is_input {
                continue;
            }
            let line_text = &line.text;
            let cstart = if row == start_row { start_col } else { 0 };
            let cend = if row == end_row {
                end_col.min(line_text.chars().count())
            } else {
                line_text.chars().count()
            };
            let slice: String = line_text
                .chars()
                .skip(cstart)
                .take(cend.saturating_sub(cstart))
                .collect();
            out.push_str(&slice);
            if row != end_row {
                out.push('\n');
            }
        }
        out.trim_end().to_string()
    }

    /// Copy a whole message (by `msg_index`) to the clipboard.
    fn copy_message(state: &mut AppState, msg_index: usize) {
        let sources = Self::source_texts(state);
        if let Some(text) = sources.get(msg_index) {
            copy_to_clipboard(text);
            state.chat_copied = Some(format!("copied message {}", msg_index + 1));
        }
    }

    /// Handle a mouse event aimed at the chat view.
    pub fn handle_mouse(state: &mut AppState, ev: MouseEvent, area: Rect) {
        if state.chat_copy_mode {
            return;
        }
        let row = ev.row.saturating_sub(area.y) as usize;
        let col = ev.column.saturating_sub(area.x) as usize;
        match ev.kind {
            MouseEventKind::Down(_) => {
                state.chat_sel_anchor = Some((row, col));
            }
            MouseEventKind::Drag(_) => {
                if let Some(anchor) = state.chat_sel_anchor {
                    let text = ChatPage::selected_text(state, anchor, (row, col));
                    if !text.is_empty() {
                        copy_to_clipboard(&text);
                        state.chat_copied = Some("copied selection".to_string());
                    }
                }
            }
            MouseEventKind::Up(_) => {
                if let Some(anchor) = state.chat_sel_anchor.take() {
                    let text = Self::selected_text(state, anchor, (row, col));
                    if !text.is_empty() {
                        copy_to_clipboard(&text);
                        state.chat_copied = Some("copied selection".to_string());
                    } else if let Some(line) = state.chat_lines.get(row) {
                        if line.msg_index != usize::MAX {
                            Self::copy_message(state, line.msg_index);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

impl Default for ChatPage {
    fn default() -> Self {
        Self::new()
    }
}

impl Page for ChatPage {
    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        let bg = theme::bg_color();
        frame.render_widget(
            ratatui::widgets::Block::default().style(Style::default().bg(bg)),
            area,
        );

        let width = area.width as usize;
        let mut lines: Vec<ChatLine> = Vec::new();

        push_line(
            &mut lines,
            "✦ Welcome to NIKI".to_string(),
            usize::MAX,
            0,
            false,
        );
        push_line(
            &mut lines,
            format!("  {}", state.description),
            usize::MAX,
            0,
            false,
        );
        push_line(
            &mut lines,
            format!("  Directory: {}", state.project_path.display()),
            usize::MAX,
            0,
            false,
        );
        if !state.branch_name.is_empty() {
            push_line(
                &mut lines,
                format!("  Branch: {}", state.branch_name),
                usize::MAX,
                0,
                false,
            );
        }
        push_line(&mut lines, String::new(), usize::MAX, 0, false);

        for (i, (role, text)) in state.chat_log.iter().enumerate() {
            push_line(&mut lines, format!("● {}: {}", role, text), i, 0, false);
        }

        let base = state.chat_log.len();
        for (i, s) in state.stages.iter().enumerate() {
            let msg_index = base + i;
            let _color = role_color(s.role);
            let status_glyph = match s.status {
                crate::display::pages::StageStatus::Running => "…",
                crate::display::pages::StageStatus::Done => "✓",
                crate::display::pages::StageStatus::Failed => "✗",
                crate::display::pages::StageStatus::Queued => "•",
            };
            push_line(
                &mut lines,
                format!(
                    " {} {} {}",
                    role_icon(s.role),
                    status_glyph,
                    role_label(s.role)
                ),
                msg_index,
                0,
                false,
            );

            let body = if s.status == crate::display::pages::StageStatus::Running
                && !s.stream.is_empty()
            {
                s.stream.clone()
            } else if !s.summary.is_empty() {
                s.summary.join("\n")
            } else {
                s.full_transcript.clone()
            };
            if body.is_empty() {
                push_line(
                    &mut lines,
                    "  (no output yet)".to_string(),
                    msg_index,
                    0,
                    false,
                );
            } else {
                for line in body.lines() {
                    push_line(&mut lines, format!("  {}", line), msg_index, 0, false);
                }
            }
            push_line(&mut lines, String::new(), usize::MAX, 0, false);
        }

        for (note, _color) in &state.notes {
            push_line(&mut lines, format!("  {}", note), usize::MAX, 0, false);
        }

        push_line(&mut lines, "─".repeat(width.min(200)), usize::MAX, 0, false);

        if state.finished {
            push_line(
                &mut lines,
                "● NIKI — pipeline finished. Review the branch.".to_string(),
                usize::MAX,
                0,
                false,
            );
        }

        let prompt = if state.chat_copy_mode { "COPY " } else { "> " };
        let before = &state.chat_input[..state.chat_cursor.min(state.chat_input.len())];
        let cursor_char = state
            .chat_input
            .chars()
            .nth(state.chat_cursor)
            .map(|c| c.to_string())
            .unwrap_or_else(|| " ".to_string());
        let after = &state.chat_input[state.chat_cursor.min(state.chat_input.len())..];
        let input_display = format!("{}{}{}{}", prompt, before, cursor_char, after);
        push_line(&mut lines, input_display, usize::MAX, 0, true);

        let hint = if state.chat_copy_mode {
            "[copy-mode] arrows move · Space mark · y yank · c char · Esc cancel"
        } else {
            "type + Enter to send · Tab pages · v copy-mode · y copy message · drag to select"
        };
        push_line(&mut lines, hint.to_string(), usize::MAX, 0, false);

        let total = lines.len();
        let visible = area.height as usize;
        let offset = if total > visible {
            total.saturating_sub(visible)
        } else {
            0
        };

        let mut rendered: Vec<Line> = Vec::with_capacity(visible);
        for line in lines.iter().skip(offset).take(visible) {
            let style = if line.is_input {
                Style::default().fg(theme::primary())
            } else {
                Style::default().fg(theme::fg_color())
            };
            rendered.push(Line::from(Span::styled(line.text.clone(), style)));
        }
        while rendered.len() < visible {
            rendered.push(Line::from(""));
        }

        frame.render_widget(Paragraph::new(rendered), area);
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut AppState) -> bool {
        cache_lines(state);

        if state.chat_copy_mode {
            match key.code {
                KeyCode::Esc => {
                    state.chat_copy_mode = false;
                    state.chat_sel_anchor = None;
                    return true;
                }
                KeyCode::Up => {
                    state.chat_cursor_pos.0 = state.chat_cursor_pos.0.saturating_sub(1);
                    return true;
                }
                KeyCode::Down => {
                    if state.chat_cursor_pos.0 + 1 < state.chat_lines.len() {
                        state.chat_cursor_pos.0 += 1;
                    }
                    return true;
                }
                KeyCode::Left => {
                    state.chat_cursor_pos.1 = state.chat_cursor_pos.1.saturating_sub(1);
                    return true;
                }
                KeyCode::Right => {
                    state.chat_cursor_pos.1 += 1;
                    return true;
                }
                KeyCode::Char(' ') => {
                    state.chat_sel_anchor = Some(state.chat_cursor_pos);
                    return true;
                }
                KeyCode::Char('y') => {
                    if let Some(anchor) = state.chat_sel_anchor.take() {
                        let text = ChatPage::selected_text(state, anchor, state.chat_cursor_pos);
                        if !text.is_empty() {
                            copy_to_clipboard(&text);
                            state.chat_copied = Some("copied selection".to_string());
                        }
                    }
                    state.chat_copy_mode = false;
                    return true;
                }
                KeyCode::Char('c') => {
                    let (r, c) = state.chat_cursor_pos;
                    if let Some(line) = state.chat_lines.get(r) {
                        let ch: String = line.text.chars().skip(c).take(1).collect();
                        if !ch.is_empty() {
                            copy_to_clipboard(&ch);
                            state.chat_copied = Some("copied char".to_string());
                        }
                    }
                    state.chat_copy_mode = false;
                    return true;
                }
                _ => return true,
            }
        }

        match key.code {
            KeyCode::Char('v') if key.modifiers == KeyModifiers::NONE => {
                state.chat_copy_mode = true;
                state.chat_cursor_pos = (0, 0);
                true
            }
            KeyCode::Char('y') if key.modifiers == KeyModifiers::NONE => {
                let idx = if state.chat_cursor_pos.0 < state.chat_lines.len() {
                    state.chat_lines[state.chat_cursor_pos.0].msg_index
                } else {
                    usize::MAX
                };
                if idx != usize::MAX {
                    ChatPage::copy_message(state, idx);
                }
                true
            }
            KeyCode::Enter => {
                if !state.chat_input.trim().is_empty() {
                    state
                        .chat_log
                        .push(("user".to_string(), state.chat_input.trim().to_string()));
                    state.chat_input.clear();
                    state.chat_cursor = 0;
                }
                true
            }
            KeyCode::Char(c) => {
                let idx = state.chat_cursor.min(state.chat_input.len());
                state.chat_input.insert(idx, c);
                state.chat_cursor += 1;
                true
            }
            KeyCode::Backspace => {
                if state.chat_cursor > 0 {
                    state.chat_cursor -= 1;
                    state.chat_input.remove(state.chat_cursor);
                }
                true
            }
            KeyCode::Delete => {
                if state.chat_cursor < state.chat_input.len() {
                    state.chat_input.remove(state.chat_cursor);
                }
                true
            }
            KeyCode::Left => {
                state.chat_cursor = state.chat_cursor.saturating_sub(1);
                true
            }
            KeyCode::Right => {
                if state.chat_cursor < state.chat_input.len() {
                    state.chat_cursor += 1;
                }
                true
            }
            KeyCode::Home => {
                state.chat_cursor = 0;
                true
            }
            KeyCode::End => {
                state.chat_cursor = state.chat_input.len();
                true
            }
            _ => false,
        }
    }

    fn title(&self) -> &str {
        "chat"
    }
}

fn push_line(
    lines: &mut Vec<ChatLine>,
    text: String,
    msg_index: usize,
    _char_start: usize,
    is_input: bool,
) {
    lines.push(ChatLine {
        text,
        msg_index,
        char_start: 0,
        is_input,
    });
}

/// Rebuild `state.chat_lines` from current state (called at the start of handle_key
/// so selection math uses coordinates that match the rendered view).
fn cache_lines(state: &mut AppState) {
    let mut lines: Vec<ChatLine> = Vec::new();
    push_line(
        &mut lines,
        "✦ Welcome to NIKI".to_string(),
        usize::MAX,
        0,
        false,
    );
    push_line(
        &mut lines,
        format!("  {}", state.description),
        usize::MAX,
        0,
        false,
    );
    push_line(
        &mut lines,
        format!("  Directory: {}", state.project_path.display()),
        usize::MAX,
        0,
        false,
    );
    if !state.branch_name.is_empty() {
        push_line(
            &mut lines,
            format!("  Branch: {}", state.branch_name),
            usize::MAX,
            0,
            false,
        );
    }
    push_line(&mut lines, String::new(), usize::MAX, 0, false);

    for (i, (role, text)) in state.chat_log.iter().enumerate() {
        push_line(&mut lines, format!("● {}: {}", role, text), i, 0, false);
    }

    let base = state.chat_log.len();
    for (i, s) in state.stages.iter().enumerate() {
        let msg_index = base + i;
        push_line(
            &mut lines,
            format!(
                " {} {} {}",
                role_icon(s.role),
                match s.status {
                    crate::display::pages::StageStatus::Running => "…",
                    crate::display::pages::StageStatus::Done => "✓",
                    crate::display::pages::StageStatus::Failed => "✗",
                    crate::display::pages::StageStatus::Queued => "•",
                },
                role_label(s.role)
            ),
            msg_index,
            0,
            false,
        );
        let body =
            if s.status == crate::display::pages::StageStatus::Running && !s.stream.is_empty() {
                s.stream.clone()
            } else if !s.summary.is_empty() {
                s.summary.join("\n")
            } else {
                s.full_transcript.clone()
            };
        if body.is_empty() {
            push_line(
                &mut lines,
                "  (no output yet)".to_string(),
                msg_index,
                0,
                false,
            );
        } else {
            for line in body.lines() {
                push_line(&mut lines, format!("  {}", line), msg_index, 0, false);
            }
        }
        push_line(&mut lines, String::new(), usize::MAX, 0, false);
    }
    for (note, _c) in &state.notes {
        push_line(&mut lines, format!("  {}", note), usize::MAX, 0, false);
    }
    push_line(&mut lines, "─".repeat(40), usize::MAX, 0, false);
    if state.finished {
        push_line(
            &mut lines,
            "● NIKI — pipeline finished. Review the branch.".to_string(),
            usize::MAX,
            0,
            false,
        );
    }
    push_line(&mut lines, "> ".to_string(), usize::MAX, 0, true);
    push_line(&mut lines, String::new(), usize::MAX, 0, false);
    state.chat_lines = lines;
}

/// Copy `text` to the system clipboard via OSC 52 (with tmux/screen wrapping).
fn copy_to_clipboard(text: &str) {
    if text.is_empty() {
        return;
    }
    let b64 = base64_encode(text.as_bytes());
    let seq = if is_tmux_or_screen() {
        format!("\x1bPtmux;\x1b\x1b]52;c;{}\x07\x1b\\", b64)
    } else {
        format!("\x1b]52;c;{}\x1b\\", b64)
    };
    let _ = std::io::stdout().write_all(seq.as_bytes());
}

fn is_tmux_or_screen() -> bool {
    if let Ok(term) = std::env::var("TERM") {
        if term.starts_with("tmux") || term.starts_with("screen") {
            return true;
        }
    }
    std::env::var("TMUX").is_ok()
}

/// Minimal base64 encoder (no external dependency) for OSC 52 payloads.
fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i + 2 < input.len() {
        let n = (input[i] as u32) << 16 | (input[i + 1] as u32) << 8 | input[i + 2] as u32;
        out.push(CHARS[(n >> 18 & 63) as usize] as char);
        out.push(CHARS[(n >> 12 & 63) as usize] as char);
        out.push(CHARS[(n >> 6 & 63) as usize] as char);
        out.push(CHARS[(n & 63) as usize] as char);
        i += 3;
    }
    let rem = input.len() - i;
    if rem == 1 {
        let n = (input[i] as u32) << 16;
        out.push(CHARS[(n >> 18 & 63) as usize] as char);
        out.push(CHARS[(n >> 12 & 63) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = (input[i] as u32) << 16 | (input[i + 1] as u32) << 8;
        out.push(CHARS[(n >> 18 & 63) as usize] as char);
        out.push(CHARS[(n >> 12 & 63) as usize] as char);
        out.push(CHARS[(n >> 6 & 63) as usize] as char);
        out.push('=');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NikiConfig;
    use std::path::PathBuf;

    fn base_state() -> AppState {
        AppState::new(
            "test task".to_string(),
            NikiConfig::default(),
            PathBuf::from("."),
        )
    }

    fn b64(s: &str) -> String {
        base64_encode(s.as_bytes())
    }

    #[test]
    fn base64_encode_known_vectors() {
        assert_eq!(b64(""), "");
        assert_eq!(b64("f"), "Zg==");
        assert_eq!(b64("fo"), "Zm8=");
        assert_eq!(b64("foo"), "Zm9v");
        assert_eq!(b64("foob"), "Zm9vYg==");
        assert_eq!(b64("fooba"), "Zm9vYmE=");
        assert_eq!(b64("foobar"), "Zm9vYmFy");
    }

    #[test]
    fn cache_lines_builds_header_and_messages() {
        let mut state = base_state();
        state.chat_log = vec![
            ("user".to_string(), "hello".to_string()),
            ("assistant".to_string(), "world".to_string()),
        ];
        cache_lines(&mut state);
        let lines: Vec<&str> = state.chat_lines.iter().map(|l| l.text.as_str()).collect();
        assert!(lines.iter().any(|l| l.contains("Welcome to NIKI")));
        assert!(lines.iter().any(|l| l.contains("test task")));
        // first chat_log line: "● user: hello"
        assert!(lines.iter().any(|l| l.contains("● user: hello")));
        assert!(lines.iter().any(|l| l.contains("● assistant: world")));
        // last visible line before the input prompt is the separator "---"
        assert!(lines.iter().any(|l| *l == "─".repeat(40).as_str()));
    }

    #[test]
    fn selected_text_within_single_message() {
        let mut state = base_state();
        state.chat_log = vec![("assistant".to_string(), "Hello, world".to_string())];
        cache_lines(&mut state);
        let row = state
            .chat_lines
            .iter()
            .position(|l| l.text.contains("Hello, world"))
            .unwrap();
        // line text is "● assistant: Hello, world"; "Hello" starts at char index 13
        assert_eq!(
            ChatPage::selected_text(&state, (row, 13), (row, 18)),
            "Hello"
        );
    }

    #[test]
    fn selected_text_spans_multiple_lines() {
        let mut state = base_state();
        state.chat_log = vec![
            ("assistant".to_string(), "Hello".to_string()),
            ("user".to_string(), "World".to_string()),
        ];
        cache_lines(&mut state);
        let hello_row = state
            .chat_lines
            .iter()
            .position(|l| l.text.contains("Hello"))
            .unwrap();
        let world_row = state
            .chat_lines
            .iter()
            .position(|l| l.text.contains("World"))
            .unwrap();
        // "● assistant: Hello" -> Hello at chars 13..18 ; next line "● user: World" full
        let sel = ChatPage::selected_text(&state, (hello_row, 13), (world_row, 14));
        assert_eq!(sel, "Hello\n● user: World");
    }

    #[test]
    fn selected_text_skips_input_lines() {
        let mut state = base_state();
        state.chat_log = vec![("assistant".to_string(), "data".to_string())];
        cache_lines(&mut state);
        let input_idx = state.chat_lines.iter().position(|l| l.is_input).unwrap();
        // selecting a range that includes an input line still omits it
        let sel = ChatPage::selected_text(&state, (0, 0), (input_idx, 10));
        assert!(!sel.contains("> "));
    }
}
