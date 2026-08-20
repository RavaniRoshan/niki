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
//! ## Progressive disclosure
//!
//! Each agent stage is a collapsible node (Claude Code / Kimi-style):
//! - **Collapsed** (done stages, default): a one-line disclosure summary.
//! - **Expanded** (running stages, or toggled with `Enter` / click): the full
//!   markdown-rendered transcript, including syntax-highlighted code blocks.

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use std::fs;
use std::io::Write;

use crate::artifacts::types::AgentRole;
use crate::display::chat::markdown::render_markdown;
use crate::display::chat::message::MessageRenderConfig;
use crate::display::chat::streaming::render_streaming_markdown;
use crate::display::components::autocomplete::build_candidates;
use crate::display::input::InputHandler;
use crate::display::pages::{AppState, ChatLine, Page, PageId, StageStatus};
use crate::display::state::{AutocompleteState, InputAction, InputMode};
use crate::display::theme;

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
        AgentRole::Planner => theme::sand(),
        AgentRole::Coder => theme::clay(),
        AgentRole::Tester => theme::fg_dim(),
        AgentRole::Reviewer => theme::warning(),
        AgentRole::Synthesizer => theme::sand(),
        AgentRole::SecurityAuditor => theme::error(),
        AgentRole::Red => theme::error(),
    }
}

fn role_icon(role: AgentRole) -> &'static str {
    match role {
        AgentRole::Planner => "◈",
        AgentRole::Coder => "⟠",
        AgentRole::Tester => "◉",
        AgentRole::Reviewer => "◆",
        AgentRole::Synthesizer => "⧉",
        AgentRole::SecurityAuditor => "⛨",
        AgentRole::Red => "✗",
    }
}

fn status_glyph(status: &StageStatus) -> &'static str {
    match status {
        StageStatus::Running => "⠋",
        StageStatus::Done => "✓",
        StageStatus::Failed => "✗",
        StageStatus::Queued => "·",
    }
}

pub struct ChatPage;

impl ChatPage {
    pub fn new() -> Self {
        Self
    }

    /// Plain-text (copyable) representation of a rendered line.
    fn line_text(l: &Line<'static>) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    /// Build the list of copyable source strings (one per visible message),
    /// indexed by `msg_index` used in [`ChatLine`].
    fn source_texts(state: &AppState) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for (role, text) in state.chat_log.iter() {
            v.push(format!("{}: {}", role, text));
        }
        for s in &state.stages {
            let body = if s.status == StageStatus::Running && !s.stream.is_empty() {
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
        let total = state.chat_lines.len();
        let visible = area.height as usize;
        let offset = scroll_offset(total, visible);
        let abs_row = offset + row;
        match ev.kind {
            MouseEventKind::Down(_) => {
                state.chat_sel_anchor = Some((abs_row, col));
            }
            MouseEventKind::Drag(_) => {
                if let Some(anchor) = state.chat_sel_anchor {
                    let text = ChatPage::selected_text(state, anchor, (abs_row, col));
                    if !text.is_empty() {
                        copy_to_clipboard(&text);
                        state.chat_copied = Some("copied selection".to_string());
                    }
                }
            }
            MouseEventKind::Up(_) => {
                if let Some(anchor) = state.chat_sel_anchor.take() {
                    let text = Self::selected_text(state, anchor, (abs_row, col));
                    if !text.is_empty() {
                        copy_to_clipboard(&text);
                        state.chat_copied = Some("copied selection".to_string());
                    } else if let Some(line) = state.chat_lines.get(abs_row) {
                        if let Some(stage_idx) = line.header_stage {
                            // Click on a stage header toggles disclosure.
                            if state.expanded_stages.contains(&stage_idx) {
                                state.expanded_stages.remove(&stage_idx);
                            } else {
                                state.expanded_stages.insert(stage_idx);
                            }
                        } else if line.msg_index != usize::MAX {
                            Self::copy_message(state, line.msg_index);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Keep slash-menu and @-autocomplete overlays in sync with the input buffer/mode.
    pub fn sync_input_overlays(&self, state: &mut AppState) {
        let buf = state.input_state.buffer.clone();

        // Slash command menu: visible while in Command mode with a '/' prefix.
        if state.input_state.mode == InputMode::Command && buf.starts_with('/') {
            state.show_command_menu = true;
            state.command_filter = buf.clone();
            if state.command_selected >= state.commands.len() {
                state.command_selected = 0;
            }
        } else if state.show_command_menu {
            state.show_command_menu = false;
            state.command_filter.clear();
            state.command_selected = 0;
        }

        // @ file autocomplete: only in Insert mode, '@' prefix, no space yet.
        if state.input_state.mode == InputMode::Insert && buf.starts_with('@') && !buf.contains(' ')
        {
            let files = Self::project_files(state);
            let candidates = build_candidates(&buf, &files);
            state.input_state.autocomplete = Some(AutocompleteState {
                prefix: buf.clone(),
                candidates,
                selected: 0,
            });
        } else if state.input_state.autocomplete.is_some() {
            state.input_state.autocomplete = None;
        }
    }

    /// Bounded walk of the project tree for @-mention file completion.
    pub fn project_files(state: &AppState) -> Vec<String> {
        let root = &state.project_path;
        let mut out = Vec::new();
        let mut stack = vec![root.clone()];
        let mut depth = 0usize;
        'walk: while let Some(dir) = stack.pop() {
            let entries = match fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if path.is_dir() {
                    if matches!(
                        name.as_str(),
                        ".git" | "node_modules" | "target" | "dist" | ".niki"
                    ) {
                        continue;
                    }
                    if depth < 6 {
                        stack.push(path);
                    }
                } else if let Ok(rel) = path.strip_prefix(root) {
                    out.push(rel.to_string_lossy().to_string());
                    if out.len() >= 200 {
                        break 'walk;
                    }
                }
            }
            depth += 1;
            if depth > 6 {
                break;
            }
        }
        out
    }
}

impl Default for ChatPage {
    fn default() -> Self {
        Self::new()
    }
}

/// Scroll offset (bottom-anchored) for `total` lines in `visible` rows.
fn scroll_offset(total: usize, visible: usize) -> usize {
    if total > visible {
        total.saturating_sub(visible)
    } else {
        0
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
        // Remember the render width so handle_key's cached lines match wrapping.
        state.chat_width.set(width);

        let lines = build_chat_lines(state, width, true);

        let visible = area.height as usize;
        let offset = scroll_offset(lines.len(), visible);

        let mut rendered: Vec<Line> = Vec::with_capacity(visible);
        for line in lines.iter().skip(offset).take(visible) {
            let base_style = if line.is_input {
                Style::default().fg(theme::primary())
            } else {
                Style::default().fg(theme::fg_color())
            };
            if let Some(rich) = &line.rich {
                rendered.push(rich.clone());
            } else {
                rendered.push(Line::from(Span::styled(line.text.clone(), base_style)));
            }
        }
        while rendered.len() < visible {
            rendered.push(Line::from(""));
        }

        frame.render_widget(Paragraph::new(rendered), area);
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut AppState) -> bool {
        build_chat_lines_into(state);

        // IME anchoring: ask the terminal for its cursor position so the IME
        // composition window can follow the caret. Opt-in via
        // `config.ui.ime_anchor`; degraded to a no-op under tmux/screen and in
        // test environments (see `display::ime::ime_capable`). We only emit the
        // request — the response is consumed by the terminal, not by us.
        if state.config.ui.ime_anchor && crate::display::ime::ime_capable() && !state.anchor_pending
        {
            state.anchor_pending = true;
            crate::display::ime::request_cursor_position();
        }

        // Ctrl+O: Global toggle for deductive reasoning / thinking traces
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('o') {
            state.show_thinking = !state.show_thinking;
            state.set_notice(
                if state.show_thinking {
                    "∴ Expanded all thinking traces (Ctrl+O)"
                } else {
                    "∴ Collapsed thinking traces (Ctrl+O)"
                },
                2000,
            );
            return true;
        }

        // Ctrl+S: send a live steering correction to the running agent
        // (kimi parity). Pre-fills the composer with `/steer ` so the user
        // types the correction and presses Enter — the pipeline polls
        // `steer_channel` for the result.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            if state.has_running_stage() {
                state.input_state.buffer.clear();
                state.input_state.buffer.push_str("/steer ");
                state.input_state.cursor_pos = state.input_state.buffer.len();
                state.input_state.mode = InputMode::Insert;
                state.set_notice("⌨ Steer: type your correction, then Enter", 2500);
            } else {
                state.set_notice("No agent running — cannot steer", 2500);
            }
            return true;
        }

        // Shift+Tab (reported as `Backtab` by some terminals): cycle permission
        // modes if input is empty, otherwise toggle thinking expansion.
        if key.code == KeyCode::BackTab
            || (key.code == KeyCode::Tab && key.modifiers.contains(KeyModifiers::SHIFT))
        {
            if state.input_state.buffer.is_empty() {
                state.permission_mode = state.permission_mode.next();
                state.set_notice(
                    &format!("⏵⏵ {}", state.permission_mode.label()),
                    2000,
                );
            } else {
                state.show_thinking = !state.show_thinking;
                state.set_notice(
                    if state.show_thinking {
                        "∴ Plan / thinking expanded (Shift+Tab)"
                    } else {
                        "∴ Plan / thinking collapsed (Shift+Tab)"
                    },
                    2000,
                );
            }
            return true;
        }

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

        // Enter on a stage header toggles expand/collapse (progressive disclosure).
        if key.code == KeyCode::Enter {
            let (row, _col) = state.chat_cursor_pos;
            if let Some(line) = state.chat_lines.get(row) {
                if let Some(stage_idx) = line.header_stage {
                    if state.expanded_stages.contains(&stage_idx) {
                        state.expanded_stages.remove(&stage_idx);
                    } else {
                        state.expanded_stages.insert(stage_idx);
                    }
                    return true;
                }
            }
        }

        // Delegate to InputHandler (unified input system).
        let handler = InputHandler::new();
        let handled = match handler.handle_insert(&mut state.input_state, key) {
            InputAction::Submit(text) => {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    if trimmed == "/help" {
                        state.chat_log.push((
                            "system".to_string(),
                            "Available slash commands:\n  /doctor          Check providers, keys, sandbox health\n  /review          Trigger code review audit on workspace\n  /diff            View full-screen unified diff\n  /cost            Show token spend and cost metrics\n  /context         Show context window utilization\n  /compact         Compact session history into memory\n  /clear           Clear conversation log\n  /model <name>    Switch active LLM model\n  /theme           Cycle color theme (dark/light/auto)\n  /config          Open configuration editor\n  /terminal-setup  Guide truecolor & OSC 52 clipboard setup\n  /undo · /redo    Undo or redo workspace checkpoints\n  /steer <msg>     Send a live steering hint to running agent".to_string(),
                        ));
                    } else if trimmed == "/clear" || trimmed == "/reset" {
                        state.chat_log.clear();
                        state.chat_lines.clear();
                        state.set_notice("Conversation cleared", 2500);
                    } else if trimmed == "/compact" {
                        let count = state.chat_log.len();
                        if count > 2 {
                            let last = state.chat_log.split_off(count - 2);
                            state.chat_log = vec![(
                                "system".to_string(),
                                format!(
                                    "Compacted {} previous turns into memory checkpoint.",
                                    count - 2
                                ),
                            )];
                            state.chat_log.extend(last);
                        } else {
                            state.chat_log.push((
                                "system".to_string(),
                                "Context is already minimal (no compaction needed).".to_string(),
                            ));
                        }
                    } else if trimmed == "/cost" {
                        state.chat_log.push((
                            "system".to_string(),
                            format!(
                                "Session Economics:\n  • Total Spend:       ${:.4} USD\n  • Input Tokens:      {}\n  • Output Tokens:     {}\n  • Cache Read Tokens: {}\n  • Cache Write Tokens:{}\n  • Model:             {}\n  • Context Limit:     {} tokens",
                                state.cost,
                                state.input_tokens,
                                state.output_tokens,
                                state.cache_read_tokens,
                                state.cache_write_tokens,
                                state.model,
                                state.context_limit
                            ),
                        ));
                    } else if trimmed == "/context" {
                        let pct = (state.context_usage * 100.0) as u32;
                        state.chat_log.push((
                            "system".to_string(),
                            format!("Context Window:\n  Utilized: {}% (~{} tokens)\n  Capacity: {} tokens (Model: {})", pct, state.token_count, state.context_limit, state.model),
                        ));
                    } else if trimmed == "/diff" {
                        state.current_page = PageId::Diff;
                    } else if trimmed == "/config" {
                        state.current_page = PageId::Config;
                    } else if trimmed == "/terminal-setup" {
                        state.chat_log.push((
                            "system".to_string(),
                            "Terminal Setup Guide:\n  • Truecolor: export COLORTERM=truecolor\n  • OSC-52 Clipboard: Supported in Ghostty, Kitty, iTerm2, WezTerm\n  • Keybindings: 'v' enters copy-mode, 'Space' marks region, 'y' yanks".to_string(),
                        ));
                    } else if trimmed.starts_with("/model") {
                        let arg = trimmed.strip_prefix("/model").unwrap_or("").trim();
                        if arg.is_empty() {
                            state.chat_log.push((
                                "system".to_string(),
                                format!(
                                    "Current model: {}. Usage: /model <model-name>",
                                    state.model
                                ),
                            ));
                        } else {
                            state.model = arg.to_string();
                            state.update_context_limit_for_model(arg);
                            state
                                .chat_log
                                .push(("system".to_string(), format!("Switched model to {}", arg)));
                        }
                    } else if trimmed == "/theme" {
                        let new_theme = crate::display::theme::next_theme();
                        state.chat_log.push((
                            "system".to_string(),
                            format!("Switched theme to {}", new_theme),
                        ));
                    } else if trimmed == "/undo" {
                        let mgr = crate::session::SessionManager::new(&state.project_path);
                        let msg = match mgr.undo() {
                            Ok(true) => "Undid last checkpoint".to_string(),
                            Ok(false) => "Nothing to undo".to_string(),
                            Err(e) => format!("Undo error: {}", e),
                        };
                        state.chat_log.push(("system".to_string(), msg));
                    } else if trimmed == "/redo" {
                        let mgr = crate::session::SessionManager::new(&state.project_path);
                        let msg = match mgr.redo() {
                            Ok(true) => "Redid last undone checkpoint".to_string(),
                            Ok(false) => "Nothing to redo".to_string(),
                            Err(e) => format!("Redo error: {}", e),
                        };
                        state.chat_log.push(("system".to_string(), msg));
                    } else if trimmed == "/rewind" {
                        let mgr = crate::session::SessionManager::new(&state.project_path);
                        let msg = match mgr.rewind() {
                            Ok(Some(label)) => format!("Rewound to checkpoint: {}", label),
                            Ok(None) => "Nothing to rewind to".to_string(),
                            Err(e) => format!("Rewind error: {}", e),
                        };
                        state.chat_log.push(("system".to_string(), msg));
                    } else if trimmed.starts_with("/steer ") {
                        let steer_msg = trimmed.strip_prefix("/steer ").unwrap_or("").trim();
                        if steer_msg.is_empty() {
                            state.chat_log.push((
                                "system".to_string(),
                                "Usage: /steer <your message to the agent>".to_string(),
                            ));
                        } else if let Some(steer_arc) = &state.steer_channel {
                            if let Ok(mut guard) = steer_arc.lock() {
                                *guard = Some(steer_msg.to_string());
                                state.chat_log.push((
                                    "system".to_string(),
                                    format!("Steered agent: {}", steer_msg),
                                ));
                            } else {
                                state.chat_log.push((
                                    "system".to_string(),
                                    "No agent running — cannot steer".to_string(),
                                ));
                            }
                        } else {
                            state.chat_log.push((
                                "system".to_string(),
                                "No agent running — cannot steer".to_string(),
                            ));
                        }
                    } else {
                        state
                            .chat_log
                            .push(("user".to_string(), trimmed.to_string()));
                    }
                }
                true
            }
            InputAction::Cancel => {
                let now = std::time::Instant::now();
                let is_double_esc = state
                    .last_esc_time
                    .map(|t| now.duration_since(t).as_millis() < 500)
                    .unwrap_or(false);
                state.last_esc_time = Some(now);

                if is_double_esc && state.input_state.buffer.is_empty() {
                    let mgr = crate::session::SessionManager::new(&state.project_path);
                    let msg = match mgr.rewind() {
                        Ok(Some(label)) => format!("Rewound to checkpoint: {}", label),
                        Ok(None) => "Nothing to rewind to (no prior checkpoints)".to_string(),
                        Err(e) => format!("Rewind error: {}", e),
                    };
                    state.chat_log.push(("system".to_string(), msg));
                    state.set_notice("Rewound checkpoint (double Esc)", 2500);
                } else {
                    // Esc in input: stop a running pipeline (if any) and surface a notice.
                    state.request_cancel("Stopping… (Esc)");
                }
                true
            }
            InputAction::ToggleCommandPalette => {
                state.show_command_palette = !state.show_command_palette;
                true
            }
            InputAction::ToggleTheme => {
                let new_pref = match state.config.ui.theme {
                    crate::config::types::ThemePreference::Dark => {
                        crate::config::types::ThemePreference::Light
                    }
                    crate::config::types::ThemePreference::Light => {
                        crate::config::types::ThemePreference::Auto
                    }
                    crate::config::types::ThemePreference::Auto => {
                        crate::config::types::ThemePreference::Dark
                    }
                };
                let mode = match new_pref {
                    crate::config::types::ThemePreference::Dark => {
                        crate::display::theme::ThemeMode::Dark
                    }
                    crate::config::types::ThemePreference::Light => {
                        crate::display::theme::ThemeMode::Light
                    }
                    crate::config::types::ThemePreference::Auto => {
                        crate::display::theme::ThemeMode::Auto
                    }
                };
                crate::display::theme::set_mode(mode);
                state.config.ui.theme = new_pref;
                true
            }
            InputAction::Quit => {
                state.modal = Some(crate::display::pages::Modal::Confirm {
                    title: "Quit".into(),
                    message: "Exit NIKI?".into(),
                });
                true
            }
            InputAction::Navigate(page) => {
                state.current_page = page;
                true
            }
            InputAction::ScrollUp => {
                state.scroll_offset = state.scroll_offset.saturating_sub(1);
                true
            }
            InputAction::ScrollDown => {
                state.scroll_offset += 1;
                true
            }
            InputAction::ToggleExpand(stage_idx) => {
                if state.expanded_stages.contains(&stage_idx) {
                    state.expanded_stages.remove(&stage_idx);
                } else {
                    state.expanded_stages.insert(stage_idx);
                }
                true
            }
            InputAction::ReverseSearch => {
                // Ctrl+R: enter reverse history search. Load the most recent
                // history entry as a starting point for incremental editing.
                state.reverse_search = !state.reverse_search;
                if state.reverse_search {
                    let hist = state.input_state.active_history().clone();
                    if let Some(last) = hist.last().cloned() {
                        state.input_state.buffer = last;
                        state.input_state.cursor_pos = state.input_state.buffer.len();
                        state.input_state.history_index = Some(hist.len() - 1);
                    }
                    state.set_notice("(reverse-search) type to filter · Enter to accept", 4000);
                }
                true
            }
            InputAction::None => false,
        };
        self.sync_input_overlays(state);
        handled
    }

    fn title(&self) -> &str {
        "chat"
    }
}

/// Render markdown `body` as indented (2-space) rich+plain rows.
///
/// When `streaming` is true the body is still being produced by the model, so
/// a partial closing code fence is trimmed (see `trim_partial_closing_fences`)
/// to keep an in-progress code block open instead of collapsing it.
fn markdown_rows(body: &str, width: usize, streaming: bool) -> Vec<ChatLine> {
    if body.trim().is_empty() {
        return Vec::new();
    }
    let inner = width.saturating_sub(2).max(20);
    let cfg = MessageRenderConfig::from_theme(inner);
    let rendered = if streaming {
        render_streaming_markdown(body, inner, &cfg)
    } else {
        render_markdown(body, inner, &cfg)
    };
    let mut out = Vec::with_capacity(rendered.len());
    for l in rendered {
        let plain = ChatPage::line_text(&l);
        let mut spans = vec![Span::styled("  ".to_string(), Style::default())];
        spans.extend(l.spans);
        spans.push(Span::styled("", Style::default())); // SEGMENT_RESET
        out.push(ChatLine {
            text: format!("  {}", plain),
            rich: Some(Line::from(spans)),
            msg_index: usize::MAX,
            char_start: 0,
            is_input: false,
            header_stage: None,
        });
    }
    out
}

/// Up to 3 preview lines for a collapsed stage (progressive disclosure).
fn disclosure_preview(s: &crate::display::pages::StageInfo) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    // Pull from summary first
    for line in &s.summary {
        let trimmed = line.trim().to_string();
        if !trimmed.is_empty() {
            lines.push(trimmed);
            if lines.len() >= 3 {
                return lines;
            }
        }
    }
    // Then from transcript
    for line in s.full_transcript.lines() {
        let trimmed = line.trim().to_string();
        if !trimmed.is_empty() {
            lines.push(trimmed);
            if lines.len() >= 3 {
                return lines;
            }
        }
    }
    if lines.is_empty() && s.status == StageStatus::Running {
        lines.push("(streaming…)".to_string());
    } else if lines.is_empty() {
        lines.push("(no output)".to_string());
    }
    lines
}

/// Build the full list of chat rows from state.
pub fn build_chat_lines(state: &AppState, width: usize, include_input: bool) -> Vec<ChatLine> {
    let mut lines: Vec<ChatLine> = Vec::new();

    push_line(
        &mut lines,
        "✦ Welcome to NIKI".to_string(),
        usize::MAX,
        0,
        false,
        None,
        None,
    );
    push_line(
        &mut lines,
        format!("  {}", state.description),
        usize::MAX,
        0,
        false,
        None,
        None,
    );
    push_line(
        &mut lines,
        format!("  Directory: {}", state.project_path.display()),
        usize::MAX,
        0,
        false,
        None,
        None,
    );
    if !state.branch_name.is_empty() {
        push_line(
            &mut lines,
            format!("  Branch: {}", state.branch_name),
            usize::MAX,
            0,
            false,
            None,
            None,
        );
    }
    push_line(&mut lines, String::new(), usize::MAX, 0, false, None, None);

    for (i, (role, text)) in state.chat_log.iter().enumerate() {
        let (icon, color) = match role.as_str() {
            "user" => ("◈", theme::clay()),
            "assistant" => ("⟠", theme::sand()),
            "system" => ("◆", theme::fg_subtle()),
            _ => ("●", theme::fg_bright()),
        };

        for (l_idx, line_str) in text.lines().enumerate() {
            if l_idx == 0 {
                let rich_line = Line::from(vec![
                    Span::styled(format!("{} ", icon), Style::default().fg(color)),
                    Span::styled(
                        format!("{}: ", role),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        line_str.to_string(),
                        Style::default().fg(theme::fg_bright()),
                    ),
                    Span::styled("", Style::default()), // SEGMENT_RESET
                ]);
                push_line(
                    &mut lines,
                    format!("{} {}: {}", icon, role, line_str),
                    i,
                    0,
                    false,
                    Some(rich_line),
                    None,
                );
            } else {
                let rich_line = Line::from(vec![
                    Span::raw("   "),
                    Span::styled(
                        line_str.to_string(),
                        Style::default().fg(theme::fg_bright()),
                    ),
                    Span::styled("", Style::default()), // SEGMENT_RESET
                ]);
                push_line(
                    &mut lines,
                    format!("   {}", line_str),
                    i,
                    0,
                    false,
                    Some(rich_line),
                    None,
                );
            }
        }
        push_line(&mut lines, String::new(), usize::MAX, 0, false, None, None);
    }

    let base = state.chat_log.len();

    // R8: Sliding-window transcript fold — when auto_collapse_turns is enabled
    // and there are many completed stages, fold the oldest ones into a summary.
    let max_visible_stages = 10usize;
    let total_stages = state.stages.len();
    let collapse_threshold = max_visible_stages.saturating_add(5);
    let skip_oldest =
        if state.config.ui.transcript.auto_collapse_turns && total_stages > collapse_threshold {
            let completed = state
                .stages
                .iter()
                .filter(|s| s.status == StageStatus::Done || s.status == StageStatus::Failed)
                .count();
            if completed > max_visible_stages {
                completed.saturating_sub(max_visible_stages)
            } else {
                0
            }
        } else {
            0
        };

    if skip_oldest > 0 {
        let skipped: Vec<_> = state.stages.iter().take(skip_oldest).collect();
        let summary = format!("··· {} earlier stages ···", skipped.len());
        push_line(
            &mut lines,
            summary,
            usize::MAX,
            0,
            false,
            Some(Line::from(Span::styled(
                format!("··· {} earlier stages ···", skipped.len()),
                Style::default().fg(theme::fg_dim()),
            ))),
            None,
        );
    }

    for (i, s) in state.stages.iter().enumerate().skip(skip_oldest) {
        let msg_index = base + i;
        let is_running = s.status == StageStatus::Running;
        let is_expanded = is_running || state.show_thinking || state.expanded_stages.contains(&i);

        let disclosure = if is_expanded { "▾" } else { "▸" };
        let mut header_text = format!(
            " {} {} {} {}",
            disclosure,
            status_glyph(&s.status),
            role_icon(s.role),
            role_label(s.role)
        );
        if s.status == StageStatus::Done {
            header_text.push_str(&format!(
                "  {} tok · ${:.4}",
                s.input_tokens + s.output_tokens,
                s.cost_usd
            ));
        }
        let status_color = match s.status {
            StageStatus::Running => theme::thinking_green(),
            StageStatus::Done => theme::success(),
            StageStatus::Failed => theme::error(),
            StageStatus::Queued => theme::fg_subtle(),
        };
        let mut header_spans = vec![
            Span::styled(
                format!(" {} ", disclosure),
                Style::default().fg(theme::fg_subtle()),
            ),
            Span::styled(
                format!("{} ", role_icon(s.role)),
                Style::default()
                    .fg(role_color(s.role))
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<9} ", role_label(s.role)),
                Style::default()
                    .fg(role_color(s.role))
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
            Span::styled(
                format!("{} ", status_glyph(&s.status)),
                Style::default().fg(status_color),
            ),
        ];
        if s.status == StageStatus::Done {
            header_spans.push(Span::styled(
                format!(
                    "{} tok · ${:.4}",
                    s.input_tokens + s.output_tokens,
                    s.cost_usd
                ),
                Style::default().fg(theme::fg_subtle()),
            ));
        } else if s.status == StageStatus::Running {
            let glyph = crate::display::components::progress::spinner_glyph(state.tick);
            let verb = crate::display::components::progress::action_verb(state.tick);
            header_spans.push(Span::styled(
                format!("∴ {} · {}...", glyph, verb),
                Style::default().fg(theme::thinking_green()),
            ));
        }
        header_spans.push(Span::styled("", Style::default())); // SEGMENT_RESET
        let header_rich = Line::from(header_spans);
        push_line(
            &mut lines,
            header_text,
            msg_index,
            0,
            false,
            Some(header_rich),
            Some(i),
        );

        if is_expanded {
            let mut parts: Vec<String> = Vec::new();
            if !s.summary.is_empty() {
                parts.push(s.summary.join("\n"));
            }
            if is_running && !s.stream.is_empty() {
                parts.push(s.stream.clone());
            } else if !s.full_transcript.is_empty() {
                parts.push(s.full_transcript.clone());
            }
            let body = parts.join("\n\n");
            for mut row in markdown_rows(&body, width, is_running) {
                row.msg_index = msg_index;
                lines.push(row);
            }
        } else {
            // Progressive disclosure: multi-line preview with dimmed styling
            let preview = disclosure_preview(s);
            for (j, preview_line) in preview.iter().enumerate() {
                let prefix = if j == 0 { "  └ " } else { "    " };
                let styled_line = Line::from(Span::styled(
                    format!("{}{}", prefix, preview_line),
                    Style::default().fg(theme::fg_subtle()),
                ));
                push_line(
                    &mut lines,
                    format!("{}{}", prefix, preview_line),
                    msg_index,
                    0,
                    false,
                    Some(styled_line),
                    None,
                );
            }
            // Hint line
            let hint = Line::from(Span::styled(
                "      Ctrl+O to expand",
                Style::default().fg(theme::fg_subtle()).add_modifier(
                    ratatui::style::Modifier::ITALIC,
                ),
            ));
            push_line(
                &mut lines,
                "      Ctrl+O to expand".to_string(),
                msg_index,
                0,
                false,
                Some(hint),
                None,
            );
        }
        push_line(&mut lines, String::new(), usize::MAX, 0, false, None, None);
    }

    for (note, _color) in &state.notes {
        push_line(
            &mut lines,
            format!("  {}", note),
            usize::MAX,
            0,
            false,
            None,
            None,
        );
    }

    push_line(
        &mut lines,
        "─".repeat(width.min(200)),
        usize::MAX,
        0,
        false,
        None,
        None,
    );

    if state.finished {
        push_line(
            &mut lines,
            "● NIKI — pipeline finished. Review the branch.".to_string(),
            usize::MAX,
            0,
            false,
            None,
            None,
        );
    }

    if include_input {
        let prompt = match state.input_state.mode {
            crate::display::state::InputMode::Shell => "! ",
            _ => {
                if state.chat_copy_mode {
                    "COPY "
                } else {
                    "> "
                }
            }
        };
        let buf = &state.input_state.buffer;
        let cursor_pos = state.input_state.cursor_pos.min(buf.len());
        let before = &buf[..cursor_pos];
        let cursor_char = buf[cursor_pos..]
            .chars()
            .next()
            .map(|c| c.to_string())
            .unwrap_or_else(|| " ".to_string());
        let after_start = cursor_pos + cursor_char.chars().count();
        let after = &buf[after_start.min(buf.len())..];
        let input_display = format!("{}{}{}{}", prompt, before, cursor_char, after);
        push_line(&mut lines, input_display, usize::MAX, 0, true, None, None);

        let hint = if state.chat_copy_mode {
            "[copy-mode] arrows move · Space mark · y yank · c char · Esc cancel"
        } else {
            "type + Enter to send · Tab pages · Enter expand · v copy-mode · y copy message · drag to select"
        };
        push_line(
            &mut lines,
            hint.to_string(),
            usize::MAX,
            0,
            false,
            None,
            None,
        );
    }

    lines
}

/// Rebuild `state.chat_lines` from current state (called at the start of
/// handle_key so selection/toggle math uses coordinates that match the view).
fn build_chat_lines_into(state: &mut AppState) {
    let width = state.chat_width.get();
    state.chat_lines = build_chat_lines(state, width, true);
}

fn push_line(
    lines: &mut Vec<ChatLine>,
    text: String,
    msg_index: usize,
    _char_start: usize,
    is_input: bool,
    rich: Option<Line<'static>>,
    header_stage: Option<usize>,
) {
    lines.push(ChatLine {
        text,
        rich,
        msg_index,
        char_start: 0,
        is_input,
        header_stage,
    });
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
    fn build_lines_header_and_messages() {
        let mut state = base_state();
        state.chat_log = vec![
            ("user".to_string(), "hello".to_string()),
            ("assistant".to_string(), "world".to_string()),
        ];
        state.chat_lines = build_chat_lines(&state, 80, true);
        let lines: Vec<&str> = state.chat_lines.iter().map(|l| l.text.as_str()).collect();
        assert!(lines.iter().any(|l| l.contains("Welcome to NIKI")));
        assert!(lines.iter().any(|l| l.contains("test task")));
        assert!(lines.iter().any(|l| l.contains("user: hello")));
        assert!(lines.iter().any(|l| l.contains("assistant: world")));
        assert!(lines.iter().any(|l| *l == "─".repeat(80).as_str()));
    }

    #[test]
    fn collapsed_stage_shows_disclosure_summary() {
        let mut state = base_state();
        state.stages = vec![crate::display::pages::StageInfo {
            role: AgentRole::Coder,
            status: StageStatus::Done,
            stream: String::new(),
            full_transcript: "long transcript\nsecond line".to_string(),
            input_tokens: 10,
            output_tokens: 20,
            cost_usd: 0.001,
            latency_ms: 100,
            summary: vec!["did the thing".to_string()],
            start: None,
        }];
        state.chat_lines = build_chat_lines(&state, 80, true);
        // Progressive disclosure: summary takes priority, then first transcript lines
        assert!(
            state
                .chat_lines
                .iter()
                .any(|l| l.text.contains("did the thing"))
        );
        // Collapsed view does NOT dump the entire transcript
        let full_lines: Vec<_> = state
            .chat_lines
            .iter()
            .filter(|l| l.text.contains("long transcript"))
            .collect();
        // At most the first line of transcript appears in preview (up to 3 lines total)
        assert!(full_lines.len() <= 1);
        let header = state
            .chat_lines
            .iter()
            .find(|l| l.text.contains("Coder"))
            .unwrap();
        assert_eq!(header.header_stage, Some(0));
    }

    #[test]
    fn expanded_stage_shows_markdown_body() {
        let mut state = base_state();
        state.expanded_stages.insert(0);
        state.stages = vec![crate::display::pages::StageInfo {
            role: AgentRole::Coder,
            status: StageStatus::Done,
            stream: String::new(),
            full_transcript: "```rust\nfn main() {}\n```".to_string(),
            input_tokens: 10,
            output_tokens: 20,
            cost_usd: 0.001,
            latency_ms: 100,
            summary: vec!["did the thing".to_string()],
            start: None,
        }];
        state.chat_lines = build_chat_lines(&state, 80, true);
        assert!(
            state
                .chat_lines
                .iter()
                .any(|l| l.text.contains("fn main()"))
        );
        assert!(state.chat_lines.iter().any(|l| l.rich.is_some()));
    }

    #[test]
    fn running_stage_is_always_expanded() {
        let mut state = base_state();
        state.stages = vec![crate::display::pages::StageInfo {
            role: AgentRole::Planner,
            status: StageStatus::Running,
            stream: "planning now".to_string(),
            full_transcript: String::new(),
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
            latency_ms: 0,
            summary: vec![],
            start: None,
        }];
        state.chat_lines = build_chat_lines(&state, 80, true);
        assert!(
            state
                .chat_lines
                .iter()
                .any(|l| l.text.contains("planning now"))
        );
    }

    #[test]
    fn selected_text_within_single_message() {
        let mut state = base_state();
        state.chat_log = vec![("assistant".to_string(), "Hello, world".to_string())];
        state.chat_lines = build_chat_lines(&state, 80, true);
        let row = state
            .chat_lines
            .iter()
            .position(|l| l.text.contains("Hello, world"))
            .unwrap();
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
        state.chat_lines = build_chat_lines(&state, 80, true);
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
        let sel = ChatPage::selected_text(&state, (hello_row, 13), (world_row, 14));
        assert!(sel.contains("Hello") && sel.contains("World"));
    }

    #[test]
    fn selected_text_skips_input_lines() {
        let mut state = base_state();
        state.chat_log = vec![("assistant".to_string(), "data".to_string())];
        state.chat_lines = build_chat_lines(&state, 80, true);
        let input_idx = state.chat_lines.iter().position(|l| l.is_input).unwrap();
        let sel = ChatPage::selected_text(&state, (0, 0), (input_idx, 10));
        assert!(!sel.contains("> "));
    }

    #[test]
    fn show_thinking_expands_all_stages() {
        let mut state = base_state();
        state.show_thinking = true;
        state.stages = vec![crate::display::pages::StageInfo {
            role: AgentRole::Planner,
            status: StageStatus::Done,
            stream: String::new(),
            full_transcript: "architecture reasoning".to_string(),
            input_tokens: 10,
            output_tokens: 20,
            cost_usd: 0.001,
            latency_ms: 100,
            summary: vec!["planned architecture".to_string()],
            start: None,
        }];
        state.chat_lines = build_chat_lines(&state, 80, true);
        assert!(
            state
                .chat_lines
                .iter()
                .any(|l| l.text.contains("architecture reasoning"))
        );
    }

    #[test]
    fn ctrl_s_prefills_steer_when_running() {
        let mut state = base_state();
        state.stages = vec![crate::display::pages::StageInfo {
            role: AgentRole::Planner,
            status: StageStatus::Running,
            stream: "planning now".to_string(),
            full_transcript: String::new(),
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
            latency_ms: 0,
            summary: vec![],
            start: None,
        }];
        let mut page = ChatPage::new();
        let key = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert!(page.handle_key(key, &mut state));
        assert!(state.input_state.buffer.starts_with("/steer "));
        assert_eq!(state.input_state.mode, InputMode::Insert);
        assert_eq!(state.input_state.cursor_pos, state.input_state.buffer.len());
    }

    #[test]
    fn ctrl_s_noop_when_no_running_stage() {
        let mut state = base_state();
        let mut page = ChatPage::new();
        let key = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert!(page.handle_key(key, &mut state));
        assert!(!state.input_state.buffer.starts_with("/steer "));
    }

    #[test]
    fn shift_tab_cycles_permission_mode() {
        let mut state = base_state();
        let initial_mode = state.permission_mode;
        let mut page = ChatPage::new();
        // Shift+Tab with empty input cycles permission modes.
        let key = KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE);
        assert!(page.handle_key(key, &mut state));
        assert_ne!(state.permission_mode, initial_mode);
    }

    #[test]
    fn shift_tab_toggles_thinking_when_typing() {
        let mut state = base_state();
        let mut page = ChatPage::new();
        // Put some text in the input buffer so Shift+Tab toggles thinking.
        state.input_state.buffer = "hello".to_string();
        let initial = state.show_thinking;
        let key = KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE);
        assert!(page.handle_key(key, &mut state));
        assert_ne!(state.show_thinking, initial);
    }
}
