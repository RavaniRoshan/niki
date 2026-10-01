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
use crate::display::pages::{AppState, ChatLine, HoverTarget, Page, PageId, StageStatus};
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
        AgentRole::Critic => "Critic",
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
        AgentRole::Critic => theme::warning(),
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
        AgentRole::Critic => "✗",
    }
}

fn status_glyph(status: &StageStatus) -> &'static str {
    crate::display::components::status::glyph(
        crate::display::components::status::UnifiedStatus::from(status.clone()),
    )
}

pub struct ChatPage;

/// The slash-command list, shown by `/help` and by `/?`.
///
/// One constant, and the help page renders the same list the command menu does,
/// so a command cannot exist in one and not the other. It previously lived inline
/// in a `trimmed == "/help"` arm, which is how `/providers` and `/mcp` ended up
/// working while never being listed anywhere a user would look.
/// The text `/help` prints.
///
/// Public so `tests/tui_sheets.rs` can assert every working command is listed,
/// rather than slicing the string out of this file.
/// The standard suffix for a command that is advertised but not implemented.
///
/// These arms used to print a success message for an action they did not take
/// — "Session forked (new branch created)", "Thinking effort set to high",
/// "Security audit queued". A command that reports work it did not do is
/// worse than one that admits it is missing, because the user stops checking.
/// One constant, so the wording cannot drift between commands.
pub const NOT_WIRED: &str = "is on the roadmap, not wired yet";

pub const HELP_TEXT: &str = concat!(
    "Available slash commands:\n",
    "  /run <task>      Run the four agents on a coding task; ends in a niki/<id> branch\n",
    "  /help            This list\n",
    "  /config          Edit settings — saved to niki.toml\n",
    "  /providers       See and change each agent's provider and model\n",
    "  /mcp             See and change the MCP servers this run may use\n",
    "  /theme           Pick a colour theme (with live preview)\n",
    "  /skills          List the skills available to agents\n",
    "  /model <name>    Switch the model for this session\n",
    "  /branch <name>   Switch branch (no argument: show the current one)\n",
    "  /status          Session status and model information\n",
    "  /doctor          Check providers, keys, sandbox health\n",
    "  /review          Trigger a code review audit on the workspace\n",
    "  /diff            Full-screen unified diff\n",
    "  /cost            Token spend and cost metrics\n",
    "  /context         Context window utilisation\n",
    "  /compact         Compact session history into memory\n",
    "  /clear           Clear the conversation log\n",
    "  /init            Scan the project and draft AGENTS.md\n",
    "  /copy            Copy the last assistant message\n",
    "  /export-md       Export the conversation as markdown\n",
    "  /terminal-setup  Truecolor and OSC 52 clipboard setup\n",
    "  /undo /redo      Undo or redo workspace checkpoints\n",
    "  /steer <msg>     Send a live steering hint to a running agent",
);
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
        // Nothing to select when there is nothing shown.
        //
        // `saturating_sub(1)` on an empty vector is 0, so the range below was
        // `0..=0` and the index that followed panicked — on an empty
        // transcript, which is exactly the state a fresh `niki chat` is in
        // until the first message is rendered. The click did not need
        // anything unusual to get there: open a chat, left-click anywhere
        // before typing, and the render thread unwound. The app vanished
        // mid-session with no shell prompt explaining where it went.
        //
        // Returning early is also the honest answer. An empty transcript has
        // no selection, so the right result is the empty string the function
        // already documents, not a panic.
        if state.chat_lines.is_empty() || start_row >= state.chat_lines.len() {
            return out;
        }
        let last_row = end_row.min(state.chat_lines.len() - 1);
        for row in start_row..=last_row {
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
        let offset = state.chat_scroll.view_offset(total, visible);
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
        state.chat_viewport_h.set(visible);
        let offset = state.chat_scroll.view_offset(lines.len(), visible);

        // Scroll indicator: show "↑ more" when scrolled up
        let scroll_indicator = if offset > 0 {
            let indicator_text = format!("  ↑ {} lines above  ", offset);
            let indicator_style = Style::default()
                .fg(theme::fg_subtle())
                .add_modifier(ratatui::style::Modifier::ITALIC);
            Some(Line::from(Span::styled(indicator_text, indicator_style)))
        } else {
            None
        };

        let mut rendered: Vec<Line> = Vec::with_capacity(visible);
        if let Some(indicator) = scroll_indicator {
            rendered.push(indicator);
        }
        // Calculate hover fade-in progress (0.0 to 1.0 over 100ms)
        let hover_progress = if let Some(hover_time) = state.hover_time {
            let elapsed = hover_time.elapsed().as_millis() as f32;
            (elapsed / 100.0).min(1.0)
        } else {
            0.0
        };
        for line in lines.iter().skip(offset).take(visible) {
            let is_hovered = match &state.hover_target {
                HoverTarget::StageHeader(idx) => line.header_stage == Some(*idx),
                HoverTarget::ChatMessage(idx) => line.msg_index == *idx,
                HoverTarget::InputBox => line.is_input,
                _ => false,
            };
            let base_style = if line.is_input {
                Style::default().fg(theme::primary())
            } else {
                Style::default().fg(theme::fg_color())
            };
            let style = if is_hovered {
                // Fade-in: use dimmed style when hover just started, full when settled
                if hover_progress < 0.5 {
                    base_style.add_modifier(ratatui::style::Modifier::DIM)
                } else {
                    base_style.add_modifier(ratatui::style::Modifier::REVERSED)
                }
            } else {
                base_style
            };
            if let Some(rich) = &line.rich {
                // Apply hover to rich lines by re-styling each span
                let styled_line: Line = rich
                    .spans
                    .iter()
                    .map(|span| {
                        Span::styled(
                            span.content.clone(),
                            if is_hovered {
                                if hover_progress < 0.5 {
                                    span.style.add_modifier(ratatui::style::Modifier::DIM)
                                } else {
                                    span.style.add_modifier(ratatui::style::Modifier::REVERSED)
                                }
                            } else {
                                span.style
                            },
                        )
                    })
                    .collect();
                rendered.push(styled_line);
            } else {
                rendered.push(Line::from(Span::styled(line.text.clone(), style)));
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

        // Ctrl+F: toggle transcript search (TUI-011). While open, printable
        // keys edit the query and Enter/Up/Down cycle matches (handled
        // below); everything else falls through to the composer.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('f') {
            if state.search.is_some() {
                state.search = None;
            } else {
                state.search = Some(crate::display::search::SearchState::new());
                state.set_notice("Search transcript — type, Enter next, Esc close", 2500);
            }
            return true;
        }
        if state.search.is_some() && crate::display::search::handle_search_key(key, state) {
            return true;
        }

        // @-file autocomplete navigation + apply (TUI-012). The overlay
        // renders from sync_input_overlays; these keys drive its selection.
        if state.input_state.autocomplete.is_some() {
            match key.code {
                KeyCode::Up => {
                    let n = state
                        .input_state
                        .autocomplete
                        .as_ref()
                        .map(|a| a.candidates.len())
                        .unwrap_or(0);
                    if n > 0 {
                        let sel = &mut state.input_state.autocomplete.as_mut().unwrap().selected;
                        *sel = sel.checked_sub(1).unwrap_or(n - 1);
                    }
                    return true;
                }
                KeyCode::Down => {
                    let n = state
                        .input_state
                        .autocomplete
                        .as_ref()
                        .map(|a| a.candidates.len().max(1))
                        .unwrap_or(1);
                    let sel = &mut state.input_state.autocomplete.as_mut().unwrap().selected;
                    *sel = (*sel + 1) % n;
                    return true;
                }
                KeyCode::Tab if key.modifiers.is_empty() => {
                    if let Some(ac) = state.input_state.autocomplete.take() {
                        if let Some(choice) = ac.candidates.get(ac.selected).cloned() {
                            state.input_state.buffer = format!("@{choice} ");
                            state.input_state.cursor_pos = state.input_state.buffer.len();
                        }
                    }
                    return true;
                }
                KeyCode::Esc => {
                    state.input_state.autocomplete = None;
                    return true;
                }
                _ => {}
            }
        }

        // /model argument completion (TUI-012): Tab completes the model id
        // from session models + well-known ids. Overlay integration is a
        // follow-up; direct completion keeps this dependency-free.
        if state.input_state.mode == InputMode::Command
            && key.code == KeyCode::Tab
            && key.modifiers.is_empty()
            && complete_model_arg(state)
        {
            return true;
        }

        // Shift+Tab (reported as `Backtab` by some terminals): cycle permission
        // modes if input is empty, otherwise toggle thinking expansion.
        if key.code == KeyCode::BackTab
            || (key.code == KeyCode::Tab && key.modifiers.contains(KeyModifiers::SHIFT))
        {
            if state.input_state.buffer.is_empty() {
                state.permission_mode = state.permission_mode.next();
                // Say what it means and when it lands. The badge used to change
                // a label and nothing else; now it reaches the stages that
                // have not started, and a notice that said only "manual" left
                // the user unable to tell a working control from a decorative
                // one — which is the whole problem with the old version.
                state.set_notice(
                    &format!(
                        "Permission mode: {} (applies from the next stage; \
                         set [permissions] mode to make it the default)",
                        state.permission_mode.label()
                    ),
                    3000,
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
                    if trimmed == "/clear" || trimmed == "/reset" {
                        state.chat_log.clear();
                        state.chat_lines.clear();
                        // The conversation is new, so the warning is owed again.
                        // Leaving it set would mean a user who cleared to get
                        // out of trouble never saw it a second time.
                        state.context_warned = false;
                        state.token_count = 0;
                        state.context_usage = 0.0;
                        state.set_notice("Conversation cleared", 2500);
                    } else if trimmed == "/compact" {
                        // This used to say "Compacted N previous turns into
                        // memory checkpoint" and then `split_off` the turns:
                        // the text was gone, the model was never told, no
                        // checkpoint was written, and the next save persisted
                        // the shortened transcript. Destructive and
                        // misreported.
                        let count = state.chat_log.len();
                        if count > 2 {
                            state.chat_log.push((
                                "system".to_string(),
                                format!(
                                    "Compacting the conversation {NOT_WIRED}. Nothing has been \
                                     removed — your full transcript is intact."
                                ),
                            ));
                        } else {
                            state.chat_log.push((
                                "system".to_string(),
                                format!("Nothing to compact yet; and compacting {NOT_WIRED}."),
                            ));
                        }
                    } else if false {
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
                                "Session Economics:\n  • Total Spend:       ${:.4} USD\n  • Input Tokens:      {}\n  • Output Tokens:     {}\n  • Cache Read Tokens: {}\n  • Cache Write Tokens: {}\n  • Model:             {}\n  • Context Limit:     {} tokens",
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
                        // A sheet, not the read-only Config page. The page could
                        // be tabbed through and nothing could be changed; a user
                        // who wanted to change a setting had to leave the product
                        // and edit TOML by hand.
                        crate::display::sheets::open_sheet(
                            state,
                            crate::display::sheets::Sheet::Settings(Default::default()),
                        );
                        state.chat_log.push((
                            "system".to_string(),
                            "Settings — space changes a value, enter saves, esc discards and closes.".to_string(),
                        ));
                    } else if trimmed == "/terminal-setup" {
                        state.chat_log.push((
                            "system".to_string(),
                            "Terminal Setup Guide:\n  • Truecolor: export COLORTERM=truecolor\n  • OSC-52 Clipboard: Supported in Ghostty, Kitty, iTerm2, WezTerm\n  • Keybindings: 'v' enters copy-mode, 'Space' marks region, 'y' yanks".to_string(),
                        ));
                    } else if trimmed.starts_with("/model") {
                        let arg = trimmed.strip_prefix("/model").unwrap_or("").trim();
                        if arg.is_empty() {
                            // The configured model, not the recorded one.
                            // `state.model` is display-only, so reporting it
                            // here told the user which model was answering when
                            // it was not the one they were shown.
                            state.chat_log.push((
                                "system".to_string(),
                                format!(
                                    "Model: {} (from [agents.coder] in niki.toml).\n\
                                     Usage: /model <name> — note that switching {NOT_WIRED}.",
                                    state.config.agents.coder.model
                                ),
                            ));
                        } else {
                            // Recorded for display only. The provider is
                            // rebuilt every turn from `config.agents.coder` in a
                            // different thread that never sees `AppState`, so
                            // this could not change which model answers — while
                            // the status bar then displayed the new name.
                            state.model = arg.to_string();
                            state.chat_log.push((
                                "system".to_string(),
                                format!(
                                    "Switching the model for this session {NOT_WIRED}. \
                                     The name is recorded for display, but the provider is \
                                     still `{}`. Change `[agents.coder] model` in niki.toml \
                                     to actually switch.",
                                    state.config.agents.coder.model
                                ),
                            ));
                        }
                    } else if trimmed == "/theme" {
                        // A list with a live preview, not a cycle. Cycling means
                        // you cannot see what exists, cannot go back without
                        // going all the way round, and cannot tell which one you
                        // are on.
                        crate::display::sheets::open_sheet(
                            state,
                            crate::display::sheets::Sheet::Theme(Default::default()),
                        );
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
                    } else if trimmed == "/status" {
                        let (_, _, cost, _) = state.totals();
                        state.chat_log.push((
                            "system".to_string(),
                            format!(
                                "Session Status\n  • Branch:    {}\n  • Model:     {}\n  • Context:   {}% (~{} / {} tokens)\n  • Spend:     ${:.4}\n  • Stages:    {}\n  • Revision:  {}/{}",
                                state.branch_name,
                                state.model,
                                (state.context_usage * 100.0) as u32,
                                state.token_count,
                                state.context_limit,
                                cost,
                                state.stages.len(),
                                state.revision_round,
                                state.max_revision_rounds,
                            ),
                        ));
                    } else if trimmed == "/permissions" {
                        state.chat_log.push((
                            "system".to_string(),
                            "Permission modes (Shift+Tab to cycle):\n  manual / accept edits / plan / auto / don't ask / bypass\n  Click the mode badge in the status bar to cycle too.".to_string(),
                        ));
                    } else if trimmed == "/plan" {
                        state.permission_mode = crate::display::state::PermissionMode::Plan;
                        state.set_notice("Entered plan mode", 1500);
                    } else if trimmed == "/version" {
                        state.chat_log.push((
                            "system".to_string(),
                            format!(
                                "niki v{} — Claude Code parity build",
                                env!("CARGO_PKG_VERSION")
                            ),
                        ));
                    } else if trimmed.starts_with("/rename") {
                        let name = trimmed
                            .strip_prefix("/rename")
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        if name.is_empty() {
                            state.chat_log.push((
                                "system".to_string(),
                                "Usage: /rename <session-name>".to_string(),
                            ));
                        } else {
                            state.chat_log.push((
                                "system".to_string(),
                                format!(
                                    "Session renaming {NOT_WIRED}. Sessions are not named yet."
                                ),
                            ));
                        }
                    } else if trimmed == "/fork" {
                        state.chat_log.push((
                            "system".to_string(),
                            format!("Forking a session {NOT_WIRED}."),
                        ));
                    } else if trimmed.starts_with("/branch") {
                        let name = trimmed
                            .strip_prefix("/branch")
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        if name.is_empty() {
                            // Read the real HEAD, not the run's remembered
                            // branch: after a checkout the two differ, and
                            // showing the run's value here is a small lie
                            // about where the user is standing.
                            let msg =
                                match crate::session::branch::current_branch(&state.project_path) {
                                    Some(b) => format!("Current branch: {b}"),
                                    None => "HEAD is detached — no branch is checked out. \
                                         Use `/branch <name>` to switch."
                                        .to_string(),
                                };
                            state.chat_log.push(("system".to_string(), msg));
                        } else {
                            // The run's whole deliverable is a `niki/<id>`
                            // branch. It used to be reachable only by leaving
                            // the TUI for a shell, which made the product's
                            // central handoff the one thing it would not do.
                            match crate::session::branch::checkout(&state.project_path, &name) {
                                Ok(after) => {
                                    // Keep the status bar honest about where
                                    // the user is now standing.
                                    state.branch_name = after.clone();
                                    state.chat_log.push((
                                        "system".to_string(),
                                        format!("Switched to branch: {after}"),
                                    ));
                                }
                                Err(e) => state.chat_log.push((
                                    "error".to_string(),
                                    format!("Could not switch branch: {e}"),
                                )),
                            }
                        }
                    } else if trimmed == "/usage" {
                        let (in_t, out_t, cost, _) = state.totals();
                        state.chat_log.push((
                            "system".to_string(),
                            format!(
                                "Usage\n  • Input tokens:  {}\n  • Output tokens: {}\n  • Total spend:   ${:.4}",
                                in_t, out_t, cost
                            ),
                        ));
                    } else if trimmed.starts_with("/effort") {
                        let level = trimmed
                            .strip_prefix("/effort")
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        state.chat_log.push((
                            "system".to_string(),
                            if level.is_empty() {
                                "Usage: /effort <low|medium|high>".to_string()
                            } else {
                                format!(
                                    "Reasoning effort {NOT_WIRED} on this surface. \
                                     Set it per agent in niki.toml instead."
                                )
                            },
                        ));
                    } else if trimmed == "/mcp" {
                        // Used to print "configured via niki.toml, use /config
                        // to edit" — the stub shape. Now that `/config` is a real
                        // editor this either does the thing or says plainly
                        // that there is nothing to show.
                        crate::display::sheets::open_sheet(
                            state,
                            crate::display::sheets::Sheet::Mcp(Default::default()),
                        );
                    } else if trimmed == "/providers" {
                        crate::display::sheets::open_sheet(
                            state,
                            crate::display::sheets::Sheet::Providers(Default::default()),
                        );
                    } else if trimmed == "/skills" {
                        // List what is actually there rather than naming a
                        // directory and leaving the user to go and look.
                        let mut dirs: Vec<String> = Vec::new();
                        if let Ok(home) = std::env::var("HOME")
                            .map(std::path::PathBuf::from)
                            .or_else(|_| std::env::var("USERPROFILE").map(std::path::PathBuf::from))
                        {
                            let shared = home.join(".agents/skills");
                            if let Ok(entries) = std::fs::read_dir(&shared) {
                                for e in entries.flatten() {
                                    if e.path().is_dir() {
                                        dirs.push(e.file_name().to_string_lossy().to_string());
                                    }
                                }
                            }
                        }
                        dirs.sort();
                        let msg = if dirs.is_empty() {
                            "Skills: none found. NIKI reads ~/.agents/skills/<name>/SKILL.md — \
create one and it is available immediately."
                                .to_string()
                        } else {
                            format!(
                                "Skills ({}):\n{}",
                                dirs.len(),
                                dirs.iter()
                                    .map(|d| format!("  {d}"))
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            )
                        };
                        state.chat_log.push(("system".to_string(), msg));
                    } else if trimmed == "/help" || trimmed == "/?" {
                        state
                            .chat_log
                            .push(("system".to_string(), HELP_TEXT.to_string()));
                    } else if trimmed == "/copy" {
                        if let Some(last) =
                            state.chat_log.iter().rev().find(|(r, _)| r == "assistant")
                        {
                            let text = last.1.clone();
                            if !text.is_empty() {
                                copy_to_clipboard(&text);
                                state.chat_copied = Some("copied last message".to_string());
                            }
                        }
                    } else if trimmed == "/export-md" {
                        let md: String = state
                            .chat_log
                            .iter()
                            .map(|(r, t)| format!("# {}\n{}\n", r, t))
                            .collect();
                        state.chat_log.push((
                            "system".to_string(),
                            format!("Markdown export ({} chars):\n{}", md.len(), md),
                        ));
                    } else if trimmed == "/doctor" {
                        // Advertised in the slash menu and in `/help` with the
                        // description "Check providers, auth, and sandbox
                        // health", and had no handler — so it was sent to the
                        // model as the literal text "/doctor".
                        state.chat_log.push((
                            "system".to_string(),
                            "Running `niki doctor` in a shell — it needs the terminal, and \
                             this surface owns it. Press Ctrl+C to leave, then run it."
                                .to_string(),
                        ));
                    } else if trimmed == "/review" || trimmed == "/code-review" {
                        // The risk classifier already escalates auth, crypto
                        // and network work to a security audit on its own.
                        state.current_page = PageId::Verdict;
                        state.chat_log.push((
                            "system".to_string(),
                            "Showing the Verdict page for the current run. A run schedules \
                             its own review; there is nothing to queue by hand."
                                .to_string(),
                        ));
                    } else if trimmed == "/btw" {
                        state.chat_log.push((
                            "system".to_string(),
                            format!("Side-question mode {NOT_WIRED}."),
                        ));
                    } else if trimmed == "/security-review" {
                        state.current_page = PageId::Verdict;
                        state.chat_log.push((
                            "system".to_string(),
                            format!(
                                "A security audit is scheduled automatically when a run touches \
                                 auth, crypto or network code. There is no way to queue one by \
                                 hand {NOT_WIRED}."
                            ),
                        ));
                    } else if trimmed.starts_with("/add-dir") {
                        let path = trimmed
                            .strip_prefix("/add-dir")
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        if path.is_empty() {
                            state
                                .chat_log
                                .push(("system".to_string(), "Usage: /add-dir <path>".to_string()));
                        } else {
                            state.chat_log.push((
                                "system".to_string(),
                                format!(
                                    "Adding a second working directory {NOT_WIRED}. The path \
                                     was not added. Pass `--project <path>` instead."
                                ),
                            ));
                        }
                    } else if trimmed == "/loop" {
                        state.chat_log.push((
                            "system".to_string(),
                            "Recurring tasks: not yet wired (post-MVP).".to_string(),
                        ));
                    } else if trimmed == "/init" {
                        // Run the real `niki init --scan` in the project dir and
                        // stream its output back into the chat log (same pattern
                        // as `niki smoke`, which shells out to current_exe).
                        let msg = match std::env::current_exe() {
                            Ok(exe) => match std::process::Command::new(exe)
                                .args(["init", "--scan"])
                                .current_dir(&state.project_path)
                                .output()
                            {
                                Ok(out) => {
                                    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
                                    let err = String::from_utf8_lossy(&out.stderr);
                                    if !err.trim().is_empty() {
                                        text.push_str(&err);
                                    }
                                    if text.trim().is_empty() {
                                        "init --scan produced no output.".to_string()
                                    } else {
                                        text.chars().take(2000).collect()
                                    }
                                }
                                Err(e) => format!("Could not run init: {e}"),
                            },
                            Err(e) => format!("Could not locate niki binary: {e}"),
                        };
                        state.chat_log.push(("system".to_string(), msg));
                    } else if trimmed == "/voice" {
                        state.chat_log.push((
                            "system".to_string(),
                            "Voice input: `niki voice` records via ffmpeg and transcribes \
                             through your configured provider's STT endpoint. Set up a \
                             provider (e.g. OpenAI) in niki.toml or via OPENAI_API_KEY."
                                .to_string(),
                        ));
                    } else if trimmed.starts_with("/run ") || trimmed == "/run" {
                        // Handled by the session processor, which owns the
                        // pipeline. It has to be named here too: the
                        // unknown-command arm below rejects anything starting
                        // with `/` that it does not recognise, and `/run` is
                        // dispatched one layer down, after this returns.
                        state
                            .chat_log
                            .push(("user".to_string(), trimmed.to_string()));
                    } else if trimmed.starts_with('/') {
                        // A command NIKI does not know must not become a prompt.
                        //
                        // Everything unmatched used to be pushed as a user
                        // message, so `/doctor` and `/review` — both listed in
                        // the slash menu and in `/help` — were silently sent to
                        // the model as the text "/doctor". The user saw the
                        // model reply, or nothing happen, with no indication
                        // that a command had been typed at all.
                        state.chat_log.push((
                            "error".to_string(),
                            format!("Unknown command: {trimmed}\nType /help for the list."),
                        ));
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
                state.chat_scroll.nudge(-1);
                true
            }
            InputAction::ScrollDown => {
                state.chat_scroll.nudge(1);
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

        // Tool card interactions (handled after input dispatch so they work
        // even when the input box is focused).
        if state.tool_detail_index.is_some() {
            // Tool detail modal is open — handle its keys.
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    state.tool_detail_index = None;
                    state.tool_detail_scroll = crate::display::scroll::ScrollState::new();
                    return true;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    state.tool_detail_scroll.nudge(1);
                    return true;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    state.tool_detail_scroll.nudge(-1);
                    return true;
                }
                KeyCode::Char('y') => {
                    if let Some(idx) = state.tool_detail_index {
                        if let Some(card) = state.tool_cards.get(idx) {
                            if let Some(output) = &card.output {
                                copy_to_clipboard(output);
                                state.chat_copied = Some("copied tool output".to_string());
                            }
                        }
                    }
                    return true;
                }
                _ => {}
            }
        } else {
            // No modal — check for Enter on a tool card to open detail.
            if key.code == KeyCode::Enter {
                let (row, _) = state.chat_cursor_pos;
                // Find which tool card (if any) the cursor is on.
                let tool_start_row = state
                    .chat_lines
                    .iter()
                    .enumerate()
                    .find(|(_, line)| line.text.contains("Tool Execution"))
                    .map(|(current_row, _)| current_row);
                if let Some(start) = tool_start_row {
                    let rel_row = row.saturating_sub(start + 1); // +1 for header
                    let mut rows_consumed = 0;
                    // Same width, same block builder as the renderer: the row a
                    // card occupies on screen is the number of lines it emits,
                    // not a second guess at it.
                    let chat_width = tool_card_width(state.chat_width.get());
                    let detail_open = state.tool_detail_index.is_some();
                    for (idx, card) in state.tool_cards.iter().enumerate() {
                        let h = crate::display::components::tool_card::tool_card_block(
                            card,
                            chat_width,
                            state.expanded_tools.contains(&idx),
                            detail_open,
                        )
                        .len();
                        if rel_row >= rows_consumed && rel_row < rows_consumed + h {
                            state.tool_detail_index = Some(idx);
                            state.tool_detail_scroll = crate::display::scroll::ScrollState::new();
                            return true;
                        }
                        rows_consumed += h;
                    }
                }
            }
        }

        self.sync_input_overlays(state);
        handled
    }

    fn title(&self) -> &str {
        "chat"
    }
}

/// Complete a `/model <prefix>` argument with Tab (TUI-012). Returns true
/// when the buffer was rewritten. Sources: session models (current + each
/// configured agent) first, then well-known ids. Best match = shortest
/// prefix hit, then alphabetical (deterministic).
/// Width the transcript lays tool cards out at.
///
/// Shared by the renderer and the Enter hit-test. They previously derived it
/// independently — `width - 4` for painting, `chat_width` for hit-testing —
/// so a card could be painted at one width and measured at another, and any
/// wrapping difference would shift every row below it.
fn tool_card_width(container_width: usize) -> u16 {
    container_width.saturating_sub(4).max(20) as u16
}

fn complete_model_arg(state: &mut AppState) -> bool {
    let buf = state.input_state.buffer.clone();
    let prefix = match buf.strip_prefix("/model ") {
        Some(p) if !p.contains(' ') => p.to_ascii_lowercase(),
        _ => return false,
    };
    let agents = &state.config.agents;
    let mut pool: Vec<String> = vec![
        state.model.clone(),
        agents.planner.model.clone(),
        agents.coder.model.clone(),
        agents.tester.model.clone(),
        agents.reviewer.model.clone(),
    ];
    pool.extend(
        [
            "claude-sonnet-4",
            "claude-haiku",
            "claude-opus-4",
            "gpt-4o",
            "gpt-4o-mini",
            "gemini-2.0-flash",
            "gemini-2.5-pro",
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    pool.sort();
    pool.dedup();
    let mut hits: Vec<&String> = pool
        .iter()
        .filter(|m| m.to_ascii_lowercase().starts_with(&prefix))
        .collect();
    if hits.is_empty() {
        hits = pool
            .iter()
            .filter(|m| m.to_ascii_lowercase().contains(&prefix))
            .collect();
    }
    hits.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    if let Some(best) = hits.first() {
        state.input_state.buffer = format!("/model {best} ");
        state.input_state.cursor_pos = state.input_state.buffer.len();
        true
    } else {
        false
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
        render_markdown(body, inner, &cfg, true)
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

        // Assistant turns go through the markdown engine.
        //
        // `src/display/chat/` is 1,367 lines of pulldown-cmark rendering —
        // fenced code with language hints, tables, lists, inline styles — and
        // none of it rendered the conversation. `build_chat_lines` used
        // `text.lines()`, so a reply containing ```rust or **bold** or a table
        // appeared literally, unstyled and unhighlighted, on the one surface
        // where a coding assistant's output is read. The engine was reachable
        // only for *stage bodies*, and stages are empty in chat.
        //
        // Errors stay plain: their text is a diagnostic, not prose, and it must
        // survive verbatim into a copy or a bug report.
        if role == "assistant" {
            let rows = markdown_rows(text, width, false);
            if !rows.is_empty() {
                push_line(
                    &mut lines,
                    format!("{icon} assistant:"),
                    usize::MAX,
                    0,
                    false,
                    Some(Line::from(vec![
                        Span::styled(format!("{icon} "), Style::default().fg(color)),
                        Span::styled(
                            "assistant:".to_string(),
                            Style::default().fg(color).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled("", Style::default()), // SEGMENT_RESET
                    ])),
                    None,
                );
                for mut row in rows {
                    // Keep the row addressable by the click/copy hit-test.
                    row.msg_index = i;
                    lines.push(row);
                }
                push_line(&mut lines, String::new(), usize::MAX, 0, false, None, None);
                continue;
            }
        }

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

    // The turn in flight. Kept out of `chat_log` so a partial reply is never
    // mistaken for a finished one, and rendered with the same styling as a
    // committed assistant turn so it does not visibly restyle itself when it
    // lands.
    if !state.chat_stream.is_empty() {
        // Same renderer as a committed turn, in its streaming form, so a reply
        // does not restyle itself the moment it lands.
        let i = state.chat_log.len();
        let rows = markdown_rows(&state.chat_stream, width, true);
        if rows.is_empty() {
            // A stream that has produced only whitespace still has to show that
            // something is arriving.
            push_line(
                &mut lines,
                "⟠ assistant: …".to_string(),
                usize::MAX,
                0,
                false,
                None,
                None,
            );
        }
        for mut row in rows {
            row.msg_index = i;
            lines.push(row);
        }
    }

    // Something is in flight and no token has arrived yet. Previously the only
    // "waiting" signal was a literal "(thinking…)" assistant bubble, so a typed
    // message produced no feedback at all and the surface looked frozen.
    if state.chat_pending && state.chat_stream.is_empty() {
        push_line(
            &mut lines,
            "  ⟠ thinking… (esc to cancel)".to_string(),
            usize::MAX,
            0,
            false,
            Some(Line::from(vec![
                Span::styled("  ⟠ ", Style::default().fg(theme::sand())),
                Span::styled(
                    "thinking… ",
                    Style::default()
                        .fg(theme::fg_subtle())
                        .add_modifier(Modifier::ITALIC),
                ),
                Span::styled("(esc to cancel)", Style::default().fg(theme::fg_subtle())),
                Span::styled("", Style::default()), // SEGMENT_RESET
            ])),
            None,
        );
    }

    if state.chat_truncated {
        push_line(
            &mut lines,
            "  ⚠ the reply was cut off at the model's token limit — it is not complete."
                .to_string(),
            usize::MAX,
            0,
            false,
            Some(Line::from(vec![
                Span::styled("  ⚠ ", Style::default().fg(theme::warning())),
                Span::styled(
                    "the reply was cut off at the model's token limit — it is not complete.",
                    Style::default().fg(theme::warning()),
                ),
                Span::styled("", Style::default()), // SEGMENT_RESET
            ])),
            None,
        );
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

    // TUI-004: read once per build so every cached row shares one mode even
    // if another thread flips the global mid-render.
    let build_theme = crate::display::theme::current_mode();
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
        let status_color = crate::display::components::status::color(
            crate::display::components::status::UnifiedStatus::from(s.status.clone()),
        );
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
        } else if s.status == StageStatus::Failed {
            // Show retry count if any retries occurred
            if s.retry_count > 0 {
                header_spans.push(Span::styled(
                    format!(" retry {}/3", s.retry_count),
                    Style::default()
                        .fg(theme::error())
                        .add_modifier(ratatui::style::Modifier::BOLD),
                ));
            }
            // Show inline error message
            if let Some(ref err) = s.error_message {
                let short_err = err.lines().next().unwrap_or("unknown error");
                let truncated = if short_err.len() > 60 {
                    format!("{}…", theme::truncate_str(short_err, 59))
                } else {
                    short_err.to_string()
                };
                header_spans.push(Span::styled(
                    format!(" — {}", truncated),
                    Style::default().fg(theme::error()),
                ));
            }
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
            // TUI-004: completed stages re-render byte-identical markdown on
            // every frame (the spinner tick defeats the coarse content hash).
            // Memoize by body+width+thinking+theme; the running stage always
            // re-renders (its stream changes) and never populates the cache.
            if is_running {
                for mut row in markdown_rows(&body, width, true) {
                    row.msg_index = msg_index;
                    lines.push(row);
                }
            } else {
                let key = crate::display::state::MarkdownCacheKey {
                    body_hash: crate::display::state::text_hash(&body),
                    width,
                    show_thinking: state.show_thinking,
                    theme: build_theme,
                };
                let cached = state.markdown_cache.borrow().get(&key).cloned();
                match cached {
                    Some(rows) => {
                        for mut row in rows {
                            row.msg_index = msg_index;
                            lines.push(row);
                        }
                    }
                    None => {
                        let rows = markdown_rows(&body, width, false);
                        for mut row in rows.clone() {
                            row.msg_index = msg_index;
                            lines.push(row);
                        }
                        let mut cache = state.markdown_cache.borrow_mut();
                        if cache.len() >= 256 {
                            cache.clear();
                        }
                        cache.insert(key, rows);
                    }
                }
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
                Style::default()
                    .fg(theme::fg_subtle())
                    .add_modifier(ratatui::style::Modifier::ITALIC),
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

    // ── Tool execution cards (Claude Code parity) ─────────────────────
    if !state.tool_cards.is_empty() {
        push_line(&mut lines, String::new(), usize::MAX, 0, false, None, None);
        push_line(
            &mut lines,
            "  ┌─ Tool Execution ─────────────────────────────┐".to_string(),
            usize::MAX,
            0,
            false,
            Some(Line::from(Span::styled(
                "  ┌─ Tool Execution ─────────────────────────────┐",
                Style::default().fg(theme::border_dim()),
            ))),
            None,
        );
        for (idx, card) in state.tool_cards.iter().enumerate() {
            let expanded = state.expanded_tools.contains(&idx);
            let card_lines = crate::display::components::tool_card::tool_card_block(
                card,
                tool_card_width(width),
                expanded,
                state.tool_detail_index.is_some(),
            );
            for card_line in card_lines {
                let plain_text = card_line
                    .spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>();
                push_line(
                    &mut lines,
                    plain_text,
                    usize::MAX,
                    0,
                    false,
                    Some(card_line),
                    None,
                );
            }
        }
        push_line(
            &mut lines,
            "  └──────────────────────────────────────────────┘".to_string(),
            usize::MAX,
            0,
            false,
            Some(Line::from(Span::styled(
                "  └──────────────────────────────────────────────┘",
                Style::default().fg(theme::border_dim()),
            ))),
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
        // Byte length, not char count: multibyte caret chars must not leave
        // `after_start` mid-character (TUI-020).
        let after_start = cursor_pos + cursor_char.len();
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

    // TUI-011: mark the current search hit with a position badge. Only the
    // rich rendering is touched; `text` (copy source) stays clean. Rows
    // without rich styling (blank/chrome) are skipped.
    if let Some(search) = state.search.as_ref() {
        if let (Some(cur), Some((pos, total))) = (search.current(), search.position()) {
            if let Some(row) = lines.get_mut(cur) {
                if let Some(rich) = row.rich.as_mut() {
                    rich.spans.push(Span::styled(
                        format!(" ◀ {pos}/{total}"),
                        Style::default()
                            .fg(theme::warning())
                            .add_modifier(Modifier::BOLD),
                    ));
                }
            }
        }
    }

    lines
}

/// Compute a simple content hash from the state that affects chat rendering.
fn chat_content_hash(state: &AppState) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    // Hash the stages (role + status + stream length + summary count)
    for s in &state.stages {
        s.role.hash(&mut hasher);
        format!("{:?}", s.status).hash(&mut hasher);
        s.stream.len().hash(&mut hasher);
        s.summary.len().hash(&mut hasher);
        s.prompt_file.hash(&mut hasher);
    }
    // Hash chat_log length
    state.chat_log.len().hash(&mut hasher);
    // Hash the in-flight turn.
    //
    // Without this, `build_chat_lines_into` early-returns on every delta
    // because the hash has not changed, `state.chat_lines` is never rebuilt,
    // and the streamed reply never appears — the surface would sit on a frozen
    // frame for the whole request and then jump straight to the finished turn.
    // A feature that works in the state machine and not on screen is the exact
    // shape of bug this slice is removing, so it does not get to ship.
    state.chat_stream.len().hash(&mut hasher);
    state.chat_stream.hash(&mut hasher);
    state.chat_pending.hash(&mut hasher);
    state.chat_truncated.hash(&mut hasher);
    // Content, not just count: a turn that is replaced in place (the streamed
    // text committing into the log) keeps the length identical at the moment
    // it matters.
    for (role, text) in &state.chat_log {
        role.hash(&mut hasher);
        text.len().hash(&mut hasher);
    }
    // Hash expanded stages
    let mut expanded: Vec<_> = state.expanded_stages.iter().copied().collect();
    expanded.sort();
    expanded.hash(&mut hasher);
    // Hash show_thinking and tick
    state.show_thinking.hash(&mut hasher);
    state.tick.hash(&mut hasher);
    hasher.finish()
}

/// Rebuild `state.chat_lines` from current state (called at the start of
/// handle_key so selection/toggle math uses coordinates that match the view).
/// Skips rebuild if content hash hasn't changed.
fn build_chat_lines_into(state: &mut AppState) {
    let new_hash = chat_content_hash(state);
    if new_hash == state.chat_content_hash && !state.chat_lines.is_empty() {
        return;
    }
    let width = state.chat_width.get();
    state.chat_lines = build_chat_lines(state, width, true);
    state.chat_content_hash = new_hash;
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
        // An assistant turn is a header row plus a rendered body. It used to
        // be asserted as the single string "assistant: world", which pinned the
        // test to one specific layout: any change to how a reply is rendered
        // broke the test without anything being wrong. What has to hold is that
        // the turn is labelled and its body is addressable.
        assert!(
            lines.iter().any(|l| l.contains("assistant:")),
            "an assistant turn must be labelled: {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.trim() == "world"),
            "the assistant body must be its own addressable row: {lines:?}"
        );
        assert!(lines.iter().any(|l| *l == "─".repeat(80).as_str()));
    }

    fn expanded_done_state() -> AppState {
        let mut state = base_state();
        state.stages = vec![crate::display::pages::StageInfo {
            role: AgentRole::Coder,
            status: StageStatus::Done,
            stream: String::new(),
            full_transcript: "did stuff\n```rust\nfn f() {}\n```".to_string(),
            input_tokens: 10,
            output_tokens: 20,
            cost_usd: 0.001,
            latency_ms: 100,
            summary: vec!["did the thing".to_string()],
            start: None,
            completed_at: None,
            prompt_file: None,
            retry_count: 0,
            error_message: None,
        }];
        state.expanded_stages.insert(0);
        state
    }

    fn lock_theme(mode: crate::display::theme::ThemeMode) -> std::sync::MutexGuard<'static, ()> {
        let guard = crate::display::theme::MODE_TEST_LOCK.lock().unwrap();
        crate::display::theme::set_mode(mode);
        guard
    }

    #[test]
    fn stage_body_memoized_across_rebuilds() {
        let _guard = lock_theme(crate::display::theme::ThemeMode::Dark);
        let state = expanded_done_state();
        let first = build_chat_lines(&state, 80, false);
        assert_eq!(state.markdown_cache.borrow().len(), 1);
        let second = build_chat_lines(&state, 80, false);
        assert_eq!(state.markdown_cache.borrow().len(), 1);
        let t1: Vec<&str> = first.iter().map(|l| l.text.as_str()).collect();
        let t2: Vec<&str> = second.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(t1, t2);
        assert!(t1.iter().any(|l| l.contains("did the thing")));
    }

    #[test]
    fn stage_body_cache_misses_on_width_change() {
        let _guard = lock_theme(crate::display::theme::ThemeMode::Dark);
        let state = expanded_done_state();
        let _ = build_chat_lines(&state, 80, false);
        let _ = build_chat_lines(&state, 100, false);
        assert_eq!(state.markdown_cache.borrow().len(), 2);
    }

    #[test]
    fn running_stage_bypasses_cache() {
        let _guard = lock_theme(crate::display::theme::ThemeMode::Dark);
        let mut state = expanded_done_state();
        state.stages[0].status = StageStatus::Running;
        state.stages[0].stream = "partial output".to_string();
        let _ = build_chat_lines(&state, 80, false);
        assert!(state.markdown_cache.borrow().is_empty());
    }

    #[test]
    fn stage_body_cache_misses_on_theme_change() {
        let _guard = lock_theme(crate::display::theme::ThemeMode::Dark);
        let state = expanded_done_state();
        crate::display::theme::set_mode(crate::display::theme::ThemeMode::Dark);
        let _ = build_chat_lines(&state, 80, false);
        crate::display::theme::set_mode(crate::display::theme::ThemeMode::Light);
        let _ = build_chat_lines(&state, 80, false);
        assert_eq!(state.markdown_cache.borrow().len(), 2);
    }

    #[test]
    fn transcript_search_toggle_type_cycle_close() {
        let mut state = base_state();
        state.chat_log = vec![
            ("assistant".to_string(), "alpha beta".to_string()),
            ("assistant".to_string(), "gamma delta".to_string()),
        ];
        state.chat_viewport_h.set(20);
        let mut page = ChatPage::new();
        let ctrl = KeyModifiers::CONTROL;
        let plain = KeyModifiers::empty();
        // Ctrl+F opens.
        assert!(page.handle_key(KeyEvent::new(KeyCode::Char('f'), ctrl), &mut state));
        assert!(state.search.is_some());
        // Typing filters; reveal unpins the follow.
        assert!(page.handle_key(KeyEvent::new(KeyCode::Char('b'), plain), &mut state));
        assert!(page.handle_key(KeyEvent::new(KeyCode::Char('e'), plain), &mut state));
        {
            let s = state.search.as_ref().unwrap();
            assert_eq!(s.query, "be");
            assert!(!s.matches.is_empty());
        }
        assert!(!state.chat_scroll.follow);
        // Enter cycles; the hit row carries the position badge.
        let first = state.search.as_ref().unwrap().current();
        assert!(page.handle_key(KeyEvent::new(KeyCode::Enter, plain), &mut state));
        assert_eq!(state.search.as_ref().unwrap().current(), first);
        state.chat_lines = build_chat_lines(&state, 80, false);
        let marked = state.chat_lines.iter().any(|l| {
            l.rich
                .as_ref()
                .map(|r| r.spans.iter().any(|sp| sp.content.contains('◀')))
                .unwrap_or(false)
        });
        assert!(marked);
        // Esc closes.
        assert!(page.handle_key(KeyEvent::new(KeyCode::Esc, plain), &mut state));
        assert!(state.search.is_none());
    }

    #[test]
    fn input_echo_with_multibyte_caret_no_panic() {
        // TUI-020: after_start used char count (always 1) instead of byte
        // length, slicing mid-character for multibyte caret chars.
        let mut state = base_state();
        state.input_state.buffer = "aé日x".to_string();
        for cursor in [0, 1, 3, 6, 7] {
            state.input_state.cursor_pos = cursor;
            let lines = build_chat_lines(&state, 80, true);
            assert!(!lines.is_empty());
        }
    }

    #[test]
    fn autocomplete_tab_applies_selection() {
        let mut state = base_state();
        state.input_state.mode = crate::display::state::InputMode::Insert;
        state.input_state.buffer = "@mai".to_string();
        state.input_state.cursor_pos = 4;
        state.input_state.autocomplete = Some(crate::display::state::AutocompleteState {
            prefix: "@mai".to_string(),
            candidates: vec!["src/main.rs".to_string(), "tests/main_test.rs".to_string()],
            selected: 1,
        });
        let mut page = ChatPage::new();
        let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::empty());
        assert!(page.handle_key(tab, &mut state));
        assert_eq!(state.input_state.buffer, "@tests/main_test.rs ");
        assert!(state.input_state.autocomplete.is_none());
    }

    #[test]
    fn autocomplete_arrows_cycle_selection() {
        let mut state = base_state();
        state.input_state.mode = crate::display::state::InputMode::Insert;
        state.input_state.buffer = "@m".to_string();
        state.input_state.cursor_pos = 2;
        state.input_state.autocomplete = Some(crate::display::state::AutocompleteState {
            prefix: "@m".to_string(),
            candidates: vec!["a".to_string(), "b".to_string()],
            selected: 0,
        });
        let mut page = ChatPage::new();
        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::empty());
        assert!(page.handle_key(up, &mut state));
        assert_eq!(state.input_state.autocomplete.as_ref().unwrap().selected, 1);
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::empty());
        assert!(page.handle_key(down, &mut state));
        assert_eq!(state.input_state.autocomplete.as_ref().unwrap().selected, 0);
    }

    #[test]
    fn model_arg_tab_completes() {
        let mut state = base_state();
        state.model = "gpt-4o-mini".to_string();
        state.input_state.mode = crate::display::state::InputMode::Command;
        state.input_state.buffer = "/model gpt".to_string();
        state.input_state.cursor_pos = 10;
        let mut page = ChatPage::new();
        let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::empty());
        assert!(page.handle_key(tab, &mut state));
        assert_eq!(state.input_state.buffer, "/model gpt-4o ");
    }

    #[test]
    fn model_arg_tab_no_match_leaves_buffer() {
        let mut state = base_state();
        state.input_state.mode = crate::display::state::InputMode::Command;
        state.input_state.buffer = "/model zzz-no-such-model".to_string();
        state.input_state.cursor_pos = 24;
        // Falls through to input dispatch (Tab → None), buffer untouched.
        let mut page = ChatPage::new();
        let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::empty());
        page.handle_key(tab, &mut state);
        assert_eq!(state.input_state.buffer, "/model zzz-no-such-model");
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
            completed_at: None,
            prompt_file: None,
            retry_count: 0,
            error_message: None,
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
            completed_at: None,
            prompt_file: None,
            retry_count: 0,
            error_message: None,
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
            completed_at: None,
            prompt_file: None,
            retry_count: 0,
            error_message: None,
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
            .expect("the reply must be on the line map");
        // Columns are derived from the row that was actually found, not
        // hard-coded. The old test passed 13..18, which silently depended on
        // the assistant label sharing a line with the body — so a rendering
        // change moved the text and the test failed for a reason that had
        // nothing to do with what it claimed to test.
        let text = &state.chat_lines[row].text;
        let start = text.find("Hello").unwrap();
        assert_eq!(
            ChatPage::selected_text(&state, (row, start), (row, start + 5)),
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
        let hello_col = state.chat_lines[hello_row].text.find("Hello").unwrap();
        let world_end = state.chat_lines[world_row].text.find("World").unwrap() + 5;
        let sel = ChatPage::selected_text(&state, (hello_row, hello_col), (world_row, world_end));
        assert!(
            sel.contains("Hello") && sel.contains("World"),
            "got: {sel:?}"
        );
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
            completed_at: None,
            prompt_file: None,
            retry_count: 0,
            error_message: None,
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
            completed_at: None,
            prompt_file: None,
            retry_count: 0,
            error_message: None,
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

    /// The cached line map must track the turn in flight.
    ///
    /// `build_chat_lines_into` early-returns unless `chat_content_hash`
    /// changes. The hash covered `chat_log.len()` but not `chat_stream`, so
    /// every streamed delta was discarded before it could be drawn: the
    /// surface held a frozen frame for the whole request and then jumped
    /// straight to the finished turn. The state machine was correct and the
    /// screen was not — which is the shape of bug this slice exists to remove,
    /// so it does not get to ship.
    #[test]
    fn the_line_map_rebuilds_as_a_turn_streams_in() {
        use crate::display::state::AppState;
        use crate::display::tui::DisplayEvent;

        let mut state = AppState::new(
            "chat".to_string(),
            crate::config::NikiConfig::default(),
            std::path::PathBuf::from("."),
        );
        state.chat_width.set(80);
        state.apply_display_event(DisplayEvent::ChatMessage {
            role: "user".to_string(),
            text: "hello".to_string(),
        });
        build_chat_lines_into(&mut state);
        let after_user = state.chat_lines.len();
        assert!(after_user > 0, "the user turn must be on the line map");

        // Nothing has arrived yet.
        let before_delta = chat_content_hash(&state);
        state.apply_display_event(DisplayEvent::ChatDelta {
            text: "Hi there".to_string(),
        });
        assert_ne!(
            chat_content_hash(&state),
            before_delta,
            "a streamed delta must invalidate the cached line map"
        );
        build_chat_lines_into(&mut state);
        let drawn: Vec<String> = state.chat_lines.iter().map(|l| l.text.clone()).collect();
        assert!(
            drawn.iter().any(|l| l.contains("Hi there")),
            "the streamed text must reach the drawn rows:\n{drawn:#?}"
        );

        // And it must keep up as more arrives.
        let after_first = chat_content_hash(&state);
        state.apply_display_event(DisplayEvent::ChatDelta {
            text: ", here to help".to_string(),
        });
        assert_ne!(
            chat_content_hash(&state),
            after_first,
            "a second delta must invalidate it again"
        );
        build_chat_lines_into(&mut state);
        let drawn: Vec<String> = state.chat_lines.iter().map(|l| l.text.clone()).collect();
        assert!(
            drawn.iter().any(|l| l.contains("here to help")),
            "the stream must accumulate, not replace:\n{drawn:#?}"
        );
    }

    /// Pending and truncated are screen state, so they belong in the hash.
    #[test]
    fn the_line_map_tracks_pending_and_truncation() {
        use crate::display::state::AppState;
        use crate::display::tui::DisplayEvent;

        let mut state = AppState::new(
            "chat".to_string(),
            crate::config::NikiConfig::default(),
            std::path::PathBuf::from("."),
        );
        let before = chat_content_hash(&state);
        state.apply_display_event(DisplayEvent::ChatPending);
        assert_ne!(chat_content_hash(&state), before, "pending must invalidate");

        let after_pending = chat_content_hash(&state);
        state.apply_display_event(DisplayEvent::ChatFinished {
            usage: None,
            finish_reason: Some("max_tokens".to_string()),
        });
        assert_ne!(
            chat_content_hash(&state),
            after_pending,
            "a truncated finish must invalidate"
        );
    }
}
