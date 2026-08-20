//! Canonical application state for the TUI.
//!
//! Single source of truth: all views (chat, pages, overlays) read from this state.
//! Events are dispatched through `apply_display_event()`, triggering re-renders.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use ratatui::style::Color;
use ratatui::text::Line;

use crate::artifacts::types::AgentRole;
use crate::config::NikiConfig;
use crate::display::onboarding::OnboardingModal;
use crate::display::theme;
use crate::display::tips::TipsBanner;
use crate::display::tui::DisplayEvent;
use crate::permissions::PermissionAction;

/// One rendered chat row, with metadata for screen-to-source mapping.
#[derive(Debug, Clone, Default)]
pub struct ChatLine {
    /// Visible plain text (used for copy/selection and screen mapping).
    pub text: String,
    /// Rich (markdown-rendered) line, if available. Falls back to `text`.
    pub rich: Option<Line<'static>>,
    /// Index into the message source list this row belongs to (`usize::MAX` = chrome).
    pub msg_index: usize,
    /// Offset of `text` within that message's source string.
    pub char_start: usize,
    /// True if this row is part of the input box (not copyable as a message).
    pub is_input: bool,
    /// If this row is a stage header, the stage index it toggles (progressive
    /// disclosure). `None` for non-toggleable chrome/content rows.
    pub header_stage: Option<usize>,
}

/// Modal overlay types.
#[derive(Debug, Clone)]
pub enum Modal {
    Confirm {
        title: String,
        message: String,
    },
    Error {
        stage: String,
        message: String,
        hint: String,
    },
}

/// View mode — chat or page-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    Chat,
    Page(PageId),
}

/// Permission mode — controls which actions require user confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    Default,
    AcceptEdits,
    Plan,
    Auto,
    DontAsk,
    BypassPermissions,
}

impl PermissionMode {
    pub fn next(&self) -> Self {
        match self {
            PermissionMode::Default => PermissionMode::AcceptEdits,
            PermissionMode::AcceptEdits => PermissionMode::Plan,
            PermissionMode::Plan => PermissionMode::Auto,
            PermissionMode::Auto => PermissionMode::DontAsk,
            PermissionMode::DontAsk => PermissionMode::BypassPermissions,
            PermissionMode::BypassPermissions => PermissionMode::Default,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            PermissionMode::Default => "manual",
            PermissionMode::AcceptEdits => "accept edits",
            PermissionMode::Plan => "plan",
            PermissionMode::Auto => "auto",
            PermissionMode::DontAsk => "don't ask",
            PermissionMode::BypassPermissions => "bypass",
        }
    }

    /// Short badge label for the status bar.
    pub fn badge(&self) -> &'static str {
        match self {
            PermissionMode::Default => "MANUAL",
            PermissionMode::AcceptEdits => "EDITS",
            PermissionMode::Plan => "PLAN",
            PermissionMode::Auto => "AUTO",
            PermissionMode::DontAsk => "YOLO",
            PermissionMode::BypassPermissions => "BYPASS",
        }
    }
}

/// Page identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PageId {
    Run,
    Pipeline,
    Agents,
    Diff,
    Verdict,
    Cost,
    Artifacts,
    History,
    Config,
    Help,
    TestLog,
    Chat,
    Fleet,
    Session,
}

impl PageId {
    pub fn all() -> &'static [PageId] {
        &[
            PageId::Run,
            PageId::Pipeline,
            PageId::Agents,
            PageId::Diff,
            PageId::Verdict,
            PageId::Cost,
            PageId::Artifacts,
            PageId::History,
            PageId::Config,
            PageId::Help,
            PageId::TestLog,
            PageId::Fleet,
            PageId::Session,
            PageId::Chat,
        ]
    }

    pub fn index(&self) -> usize {
        Self::all().iter().position(|p| p == self).unwrap_or(0)
    }

    pub fn from_key(c: char) -> Option<PageId> {
        match c {
            'p' => Some(PageId::Pipeline),
            'a' => Some(PageId::Agents),
            'd' => Some(PageId::Diff),
            'v' => Some(PageId::Verdict),
            'c' => Some(PageId::Cost),
            'f' => Some(PageId::Artifacts),
            'h' => Some(PageId::History),
            ',' => Some(PageId::Config),
            '?' => Some(PageId::Help),
            'l' => Some(PageId::TestLog),
            'g' => Some(PageId::Fleet),
            's' => Some(PageId::Session),
            _ => None,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            PageId::Run => "run",
            PageId::Pipeline => "pipeline",
            PageId::Agents => "agents",
            PageId::Diff => "diff",
            PageId::Verdict => "verdict",
            PageId::Cost => "cost",
            PageId::Artifacts => "artifacts",
            PageId::History => "history",
            PageId::Config => "config",
            PageId::Help => "help",
            PageId::TestLog => "test_log",
            PageId::Chat => "chat",
            PageId::Fleet => "fleet",
            PageId::Session => "session",
        }
    }

    pub fn key_hint(&self) -> &'static str {
        match self {
            PageId::Run => "",
            PageId::Pipeline => "p",
            PageId::Agents => "a",
            PageId::Diff => "d",
            PageId::Verdict => "v",
            PageId::Cost => "c",
            PageId::Artifacts => "f",
            PageId::History => "h",
            PageId::Config => ",",
            PageId::Help => "?",
            PageId::TestLog => "l",
            PageId::Chat => "tab",
            PageId::Fleet => "g",
            PageId::Session => "s",
        }
    }
}

/// Input modes (matching Claude Code / Kimi Code).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    #[default]
    Insert,
    Command,
    Shell,
}

/// Result of handling a key in the input system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputAction {
    None,
    /// User submitted the input buffer.
    Submit(String),
    /// Switch to a different page.
    Navigate(PageId),
    /// Quit the application.
    Quit,
    /// Toggle command palette.
    ToggleCommandPalette,
    /// Toggle theme.
    ToggleTheme,
    /// Scroll up in chat.
    ScrollUp,
    /// Scroll down in chat.
    ScrollDown,
    /// Toggle expand/collapse of the stage at the cursor.
    ToggleExpand(usize),
    /// Cancel the current operation (Esc in input, etc.).
    Cancel,
    /// Trigger reverse (incremental) search through command history.
    ReverseSearch,
}

/// Autocomplete state for @ file completion.
#[derive(Debug, Clone, Default)]
pub struct AutocompleteState {
    pub prefix: String,
    pub candidates: Vec<String>,
    pub selected: usize,
}

/// Input state with cursor management.
#[derive(Debug, Clone, Default)]
pub struct InputState {
    pub buffer: String,
    pub cursor_pos: usize,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub mode: InputMode,
    pub autocomplete: Option<AutocompleteState>,
    /// Last time the buffer was edited (drives the "Typing…" indicator).
    pub last_typed_at: Option<Instant>,
    /// Prompts submitted while an operation is in flight are queued and
    /// replayed once the current turn completes.
    pub queued: Vec<String>,
    /// Per-mode command history so Up/Down recalls only same-mode entries.
    pub shell_history: Vec<String>,
    pub command_history: Vec<String>,
    /// During a paste burst (bracketed paste or rapid char stream), Enter keys
    /// are treated as newlines rather than submits. Set by the paste handler,
    /// cleared automatically after the burst window expires.
    pub paste_burst_until: Option<Instant>,
}

impl InputState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset the input buffer after submission.
    pub fn clear(&mut self) {
        if !self.buffer.is_empty() {
            match self.mode {
                InputMode::Shell => self.shell_history.push(self.buffer.clone()),
                InputMode::Command => self.command_history.push(self.buffer.clone()),
                InputMode::Insert => self.history.push(self.buffer.clone()),
            }
        }
        self.buffer.clear();
        self.cursor_pos = 0;
        self.history_index = None;
        self.autocomplete = None;
        self.paste_burst_until = None;
    }

    /// Mark the start of a paste burst. Enter keys received within the burst
    /// window are treated as newlines, not submits.
    pub fn start_paste_burst(&mut self) {
        self.paste_burst_until = Some(Instant::now() + std::time::Duration::from_millis(80));
    }

    /// Whether the input is currently in a paste burst window.
    pub fn in_paste_burst(&self) -> bool {
        self.paste_burst_until
            .map(|t| Instant::now() < t)
            .unwrap_or(false)
    }

    /// Insert a character at the cursor position.
    pub fn insert_char(&mut self, c: char) {
        self.buffer.insert(self.cursor_pos, c);
        self.cursor_pos += 1;
        self.last_typed_at = Some(Instant::now());
    }

    /// Insert a sanitized string (normalizing CRLF and CR line endings) at the cursor position.
    pub fn insert_str(&mut self, s: &str) {
        let normalized = s.replace("\r\n", "\n").replace('\r', "\n");
        self.buffer.insert_str(self.cursor_pos, &normalized);
        self.cursor_pos += normalized.len();
        self.last_typed_at = Some(Instant::now());
    }

    /// Delete the character before the cursor (backspace).
    pub fn delete_back(&mut self) -> bool {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            self.buffer.remove(self.cursor_pos);
            self.last_typed_at = Some(Instant::now());
            true
        } else {
            false
        }
    }

    /// Delete the character at the cursor (delete key).
    pub fn delete_forward(&mut self) -> bool {
        if self.cursor_pos < self.buffer.len() {
            self.buffer.remove(self.cursor_pos);
            self.last_typed_at = Some(Instant::now());
            true
        } else {
            false
        }
    }

    /// Move cursor left.
    pub fn move_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    /// Move cursor right.
    pub fn move_right(&mut self) {
        if self.cursor_pos < self.buffer.len() {
            self.cursor_pos += 1;
        }
    }

    /// Move cursor to beginning of line.
    pub fn move_to_start(&mut self) {
        self.cursor_pos = 0;
    }

    /// Move cursor to end of line.
    pub fn move_to_end(&mut self) {
        self.cursor_pos = self.buffer.len();
    }

    /// Move cursor one word to the left (Ctrl+Left / Alt+Left).
    pub fn move_word_left(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }
        let bytes = self.buffer.as_bytes();
        let mut pos = self.cursor_pos;
        // Skip whitespace to the left.
        while pos > 0 && bytes[pos - 1].is_ascii_whitespace() {
            pos -= 1;
        }
        // Skip the word itself.
        while pos > 0 && !bytes[pos - 1].is_ascii_whitespace() {
            pos -= 1;
        }
        self.cursor_pos = pos;
    }

    /// Move cursor one word to the right (Ctrl+Right / Alt+Right).
    pub fn move_word_right(&mut self) {
        let len = self.buffer.len();
        if self.cursor_pos >= len {
            return;
        }
        let bytes = self.buffer.as_bytes();
        let mut pos = self.cursor_pos;
        // Skip the current word.
        while pos < len && !bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        // Skip whitespace to the right.
        while pos < len && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        self.cursor_pos = pos;
    }

    /// Insert a newline at the cursor (multiline composer, Shift+Enter).
    pub fn insert_newline(&mut self) {
        self.buffer.insert(self.cursor_pos, '\n');
        self.cursor_pos += 1;
        self.last_typed_at = Some(Instant::now());
    }

    /// Queue a prompt to be replayed after the current turn.
    pub fn queue_prompt(&mut self, prompt: String) {
        self.queued.push(prompt);
    }

    /// Take the next queued prompt, if any.
    pub fn next_queued(&mut self) -> Option<String> {
        if self.queued.is_empty() {
            None
        } else {
            Some(self.queued.remove(0))
        }
    }

    /// Whether any prompts are queued.
    pub fn has_queued(&self) -> bool {
        !self.queued.is_empty()
    }

    /// Navigate to previous history entry.
    pub fn history_prev(&mut self) {
        let hist: &Vec<String> = match self.mode {
            InputMode::Shell => &self.shell_history,
            InputMode::Command => &self.command_history,
            InputMode::Insert => &self.history,
        };
        if hist.is_empty() {
            return;
        }
        match self.history_index {
            None => {
                self.history_index = Some(hist.len() - 1);
                self.buffer = hist[hist.len() - 1].clone();
                self.cursor_pos = self.buffer.len();
            }
            Some(idx) if idx > 0 => {
                self.history_index = Some(idx - 1);
                self.buffer = hist[idx - 1].clone();
                self.cursor_pos = self.buffer.len();
            }
            _ => {}
        }
    }

    /// Navigate to next history entry.
    pub fn history_next(&mut self) {
        let hist: &Vec<String> = match self.mode {
            InputMode::Shell => &self.shell_history,
            InputMode::Command => &self.command_history,
            InputMode::Insert => &self.history,
        };
        match self.history_index {
            Some(idx) if idx + 1 < hist.len() => {
                self.history_index = Some(idx + 1);
                self.buffer = hist[idx + 1].clone();
                self.cursor_pos = self.buffer.len();
            }
            Some(_) => {
                self.history_index = None;
                self.buffer.clear();
                self.cursor_pos = 0;
            }
            None => {}
        }
    }

    /// Active history vec for the current input mode (mode-aware recall).
    pub fn active_history(&self) -> &Vec<String> {
        match self.mode {
            InputMode::Shell => &self.shell_history,
            InputMode::Command => &self.command_history,
            InputMode::Insert => &self.history,
        }
    }

    /// Current (1-based) line and (1-based) column of the caret.
    pub fn line_col(&self) -> (usize, usize) {
        let before = &self.buffer[..self.cursor_pos.min(self.buffer.len())];
        let line = before.matches('\n').count() + 1;
        let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
        (line, col)
    }

    /// Whether the user edited the buffer within the last `ms` milliseconds.
    pub fn is_typing(&self, ms: u64) -> bool {
        match self.last_typed_at {
            Some(t) => t.elapsed() < Duration::from_millis(ms),
            None => false,
        }
    }
}

/// A single message in the conversation.
#[derive(Debug, Clone)]
pub enum Message {
    User {
        content: String,
        timestamp: DateTime<Utc>,
    },
    Assistant {
        content: String,
        role: AgentRole,
        timestamp: DateTime<Utc>,
        tool_calls: Vec<ToolCall>,
        thinking: Option<String>,
    },
    System {
        content: String,
        level: SystemLevel,
    },
}

/// Tool call display.
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub tool_name: String,
    pub status: ToolCallStatus,
    pub summary: Option<String>,
}

/// Tool call status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCallStatus {
    Running,
    Done,
    Failed,
}

/// System message severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemLevel {
    Info,
    Warning,
    Error,
}

/// Permission request from the agent.
#[derive(Debug)]
pub struct PermissionRequest {
    pub tool_name: String,
    pub command: String,
    pub description: String,
    pub params: Option<String>,
    pub response_tx: std::sync::mpsc::Sender<PermissionAction>,
}

/// Slash command definition.
#[derive(Debug, Clone)]
pub struct Command {
    pub name: String,
    pub description: String,
    pub action: CommandAction,
}

/// Command action types.
#[derive(Debug, Clone)]
pub enum CommandAction {
    Clear,
    Compact,
    ShowCost,
    ShowDiff,
    SwitchModel(String),
    ShowPipeline,
    SwitchTuiMode,
    CycleTheme,
    Help,
    Quit,
    Undo,
    Redo,
    Rewind,
    Steer,
    Doctor,
    Review,
    Context,
    Config,
    Init,
    TerminalSetup,
}

/// The main application state — single source of truth for the UI.
#[derive(Debug)]
/// Canonical application state — single source of truth for the TUI.
pub struct AppState {
    /// Current view mode (chat or page).
    pub view: ViewMode,
    /// Current page (for page navigation; mirrors view when ViewMode::Page).
    pub current_page: PageId,
    /// Scroll offset for chat view.
    pub scroll_offset: usize,
    /// Auto-scroll to bottom on new messages.
    pub auto_scroll: bool,
    /// Conversation messages.
    pub messages: Vec<Message>,
    /// Current input state.
    pub input_state: InputState,
    /// Whether the command menu is visible.
    pub show_command_menu: bool,
    /// Command menu filter.
    pub command_filter: String,
    /// Selected command index.
    pub command_selected: usize,
    /// Available slash commands.
    pub commands: Vec<Command>,
    /// Handle used to cancel the running pipeline (set by run_tui).
    pub cancel: Option<Arc<AtomicBool>>,
    /// Transient on-screen notice, e.g. "Esc — stopping…". Auto-clears after a few seconds.
    pub notice: Option<(String, Instant)>,
    /// Whether reverse (incremental) history search is active (Ctrl+R).
    pub reverse_search: bool,
    /// Set while we are awaiting a terminal cursor-position report for IME
    /// anchoring. Cleared once the response is drained (see `display::ime`).
    pub anchor_pending: bool,
    /// Whether the permission modal is visible.
    pub show_permission_modal: bool,
    /// Current permission request (if any).
    pub permission_request: Option<PermissionRequest>,
    /// Selected permission option (0=allow once, 1=allow always, 2=deny).
    pub permission_selected: usize,
    /// Show raw params detail in permission modal (Ctrl+D toggle).
    pub show_permission_detail: bool,
    /// Permission scope selector (0=Turn, 1=Session, 2=Project).
    pub permission_scope: usize,
    /// Whether help overlay is shown.
    pub show_help: bool,
    /// Context usage percentage (0.0 - 1.0).
    pub context_usage: f64,
    /// Token count.
    pub token_count: usize,
    /// Context limit (model-specific).
    pub context_limit: usize,
    /// Total cost in USD.
    pub cost: f64,
    /// Model name.
    pub model: String,
    /// Project directory.
    pub project_path: PathBuf,
    /// Branch name.
    pub branch_name: String,
    /// Spinner tick for animations.
    pub tick: usize,
    /// Whether the pipeline is paused.
    pub paused: bool,
    /// Background task count.
    pub background_tasks: usize,
    /// Pipeline stages (flat — the canonical stage list).
    pub stages: Vec<StageInfo>,
    /// Run state for page view.
    pub run_state: RunState,
    /// Revision round.
    pub revision_round: u32,
    /// Max revision rounds.
    pub max_revision_rounds: u32,
    /// Task description.
    pub description: String,
    /// Notes for revision display.
    pub notes: Vec<(String, Color)>,
    /// Diff content.
    pub diff_content: Option<String>,
    /// Report content.
    pub report_content: Option<String>,
    /// Cost JSON.
    pub cost_json: Option<String>,
    /// Test log.
    pub test_log: Option<String>,
    /// Artifacts directory.
    pub artifacts_dir: Option<PathBuf>,
    /// Whether pipeline finished.
    pub finished: bool,
    /// Pipeline start time.
    pub start_time: Option<std::time::Instant>,
    // --- Chat view state (ported from pages::AppState) ---
    /// Current chat input text.
    pub chat_input: String,
    /// Chat input cursor position.
    pub chat_cursor: usize,
    /// Whether chat copy mode is active (v key).
    pub chat_copy_mode: bool,
    /// Anchor position for selection.
    pub chat_sel_anchor: Option<(usize, usize)>,
    /// Current chat cursor position (row, col).
    pub chat_cursor_pos: (usize, usize),
    /// Last copied text.
    pub chat_copied: Option<String>,
    /// Rendered chat lines.
    pub chat_lines: Vec<ChatLine>,
    /// Content hash of the last chat_lines build (for skip-if-unchanged).
    pub chat_content_hash: u64,
    /// Chat log — (role, text) pairs.
    pub chat_log: Vec<(String, String)>,
    /// Stages expanded in chat view (by index). Collapsed by default.
    pub expanded_stages: std::collections::HashSet<usize>,
    /// Last rendered content width for the chat view (kept in sync by render()).
    /// Interior-mutable because `Page::render` borrows state immutably.
    pub chat_width: std::cell::Cell<usize>,
    // --- Config and UI chrome ---
    /// Pipeline configuration.
    pub config: NikiConfig,
    /// Active modal overlay.
    pub modal: Option<Modal>,
    /// Onboarding state.
    pub onboarding: Option<OnboardingModal>,
    /// Whether onboarding is complete.
    pub onboarded: bool,
    /// Tips banner state.
    pub tips: TipsBanner,
    /// Whether the command palette is visible.
    pub show_command_palette: bool,
    /// Channel for /steer corrections — the pipeline polls the Arc<Mutex<Option<String>>> for user messages.
    pub steer_channel: Option<std::sync::Arc<std::sync::Mutex<Option<String>>>>,
    /// Shared domain stores (missions / sessions / agents / event bus).
    pub stores: crate::mission::Stores,
    /// Live Fleet view state (mission grid). Refreshed from `stores` each render.
    pub fleet: crate::display::pages::fleet::FleetState,
    /// Session view state for the currently-open mission (`g`/`s` pages).
    pub session_view: Option<crate::display::pages::session::SessionState>,
    /// Id of the mission selected in the Fleet grid.
    pub selected_mission: Option<crate::mission::MissionId>,
    /// Whether deductive thinking blocks are expanded globally (toggled via Ctrl+O).
    pub show_thinking: bool,
    /// Timestamp of last Esc keypress for double-Esc rewind shortcut.
    pub last_esc_time: Option<std::time::Instant>,
    /// Current permission mode.
    pub permission_mode: PermissionMode,
    /// Total session input tokens.
    pub input_tokens: usize,
    /// Total session output tokens.
    pub output_tokens: usize,
    /// Total session cache-read tokens.
    pub cache_read_tokens: usize,
    /// Total session cache-write tokens.
    pub cache_write_tokens: usize,
}

/// Stage information (mirrors existing StageInfo).
#[derive(Debug, Clone)]
pub struct StageInfo {
    pub role: AgentRole,
    pub status: StageStatus,
    pub stream: String,
    pub full_transcript: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cost_usd: f64,
    pub latency_ms: u64,
    pub summary: Vec<String>,
    pub start: Option<std::time::Instant>,
    /// Prompt file used for this stage (e.g. "planner.md").
    pub prompt_file: Option<String>,
    /// Number of retries attempted for this stage.
    pub retry_count: u32,
    /// Error message if the stage failed.
    pub error_message: Option<String>,
}

/// Map an AgentRole to its prompt file name (without extension).
fn role_to_prompt_name(role: AgentRole) -> &'static str {
    match role {
        AgentRole::Planner => "planner",
        AgentRole::Coder => "coder",
        AgentRole::Tester => "tester",
        AgentRole::Reviewer => "reviewer",
        AgentRole::Synthesizer => "synthesizer",
        AgentRole::SecurityAuditor => "security_auditor",
        AgentRole::Red => "red",
    }
}

/// Stage status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageStatus {
    Running,
    Done,
    Failed,
    Queued,
}

/// Run state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunState {
    #[default]
    Idle,
    Running,
    AwaitingReviewer,
    AwaitingApproval,
    Failed,
    Cancelled,
}

impl AppState {
    /// Create a new AppState with default values.
    pub fn new(description: String, config: NikiConfig, project_path: PathBuf) -> Self {
        let tips_enabled = config.ui.tips.enabled;
        let tips_rotation = config.ui.tips.rotation_seconds;
        Self {
            view: ViewMode::Chat,
            current_page: PageId::Run,
            scroll_offset: 0,
            auto_scroll: true,
            messages: Vec::new(),
            input_state: InputState::new(),
            show_command_menu: false,
            command_filter: String::new(),
            command_selected: 0,
            commands: default_commands(),
            show_permission_modal: false,
            permission_request: None,
            permission_selected: 0,
            show_permission_detail: false,
            permission_scope: 0,
            show_help: false,
            context_usage: 0.0,
            token_count: 0,
            context_limit: 200_000,
            cost: 0.0,
            model: config.agents.coder.model.clone(),
            project_path,
            branch_name: String::new(),
            tick: 0,
            paused: false,
            background_tasks: 0,
            stages: Vec::new(),
            run_state: RunState::Idle,
            revision_round: 0,
            max_revision_rounds: config.general.max_revision_rounds,
            description,
            notes: Vec::new(),
            diff_content: None,
            report_content: None,
            cost_json: None,
            test_log: None,
            artifacts_dir: None,
            finished: false,
            start_time: None,
            chat_input: String::new(),
            chat_cursor: 0,
            chat_copy_mode: false,
            chat_sel_anchor: None,
            chat_cursor_pos: (0, 0),
            chat_copied: None,
            chat_lines: Vec::new(),
            chat_content_hash: 0,
            chat_log: Vec::new(),
            expanded_stages: std::collections::HashSet::new(),
            chat_width: std::cell::Cell::new(80),
            config,
            modal: None,
            onboarding: None,
            onboarded: false,
            tips: TipsBanner::new(tips_enabled, tips_rotation),
            show_command_palette: false,
            steer_channel: None,
            cancel: None,
            notice: None,
            reverse_search: false,
            anchor_pending: false,
            stores: crate::mission::Stores::new(crate::event::EventBus::new()),
            fleet: crate::display::pages::fleet::FleetState::new(Vec::new()),
            session_view: None,
            selected_mission: None,
            show_thinking: false,
            last_esc_time: None,
            permission_mode: PermissionMode::Default,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        }
    }

    /// Show a transient notice (auto-cleared after `ttl_ms`).
    pub fn set_notice(&mut self, msg: &str, ttl_ms: u64) {
        self.notice = Some((
            msg.to_string(),
            Instant::now() + Duration::from_millis(ttl_ms),
        ));
    }

    /// Drop the notice if its TTL has elapsed. Call once per render tick.
    pub fn clear_stale_notice(&mut self) {
        if let Some((_, until)) = self.notice {
            if Instant::now() >= until {
                self.notice = None;
            }
        }
    }

    /// Update the context window limit based on the active model name.
    /// Uses a small hardcoded registry of known models; falls back to 200K.
    pub fn update_context_limit_for_model(&mut self, model: &str) {
        let lower = model.to_lowercase();
        let limit = if lower.contains("gemini") {
            1_000_000
        } else if lower.contains("gpt-4o") || lower.contains("gpt-4-turbo") {
            128_000
        } else if lower.contains("gpt-4") && lower.contains("32k") {
            32_000
        } else if lower.contains("gpt-4") {
            8_000
        } else {
            200_000
        };
        self.context_limit = limit;
        if self.context_limit > 0 {
            self.context_usage = (self.token_count as f64) / (self.context_limit as f64);
        }
    }

    /// Request cancellation of the running pipeline, if any, and show a notice.
    pub fn request_cancel(&mut self, notice: &str) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.set_notice(notice, 4000);
    }

    /// Refresh the Fleet grid from the shared mission store (called each render).
    pub fn refresh_fleet(&mut self) {
        let missions = futures::executor::block_on(self.stores.missions.list());
        // Preserve the current selection index where possible.
        let selected = self.fleet.selected.min(missions.len().saturating_sub(1));
        self.fleet = crate::display::pages::fleet::FleetState::new(missions);
        self.fleet.selected = selected;
    }

    /// Open the Session view for the mission at the Fleet cursor.
    pub fn open_selected_mission(&mut self) {
        let missions = futures::executor::block_on(self.stores.missions.list());
        if let Some(m) = missions.into_iter().nth(self.fleet.selected) {
            self.selected_mission = Some(m.id.clone());
            let mission_sessions: std::collections::HashSet<_> =
                m.sessions.iter().cloned().collect();
            let agents = futures::executor::block_on(self.stores.agents.list())
                .into_iter()
                .filter(|a| mission_sessions.contains(&a.session_id))
                .collect();
            let session_state =
                crate::display::pages::session::SessionState::with_agents(m, agents);
            self.session_view = Some(session_state);
            self.current_page = PageId::Session;
        }
    }

    /// Return from the Session view to the Fleet grid.
    pub fn close_session_to_fleet(&mut self) {
        self.session_view = None;
        self.current_page = PageId::Fleet;
    }

    /// Get the theme background color for the current theme mode.
    pub fn theme_bg(&self) -> Color {
        theme::bg_color()
    }

    /// Apply a DisplayEvent to update pipeline-related state.
    pub fn apply_display_event(&mut self, ev: DisplayEvent) {
        match ev {
            DisplayEvent::Banner { description } => {
                self.description = description;
            }
            DisplayEvent::StageStart { role } => {
                if self.start_time.is_none() {
                    self.start_time = Some(std::time::Instant::now());
                }
                self.stages.push(StageInfo {
                    role,
                    status: StageStatus::Running,
                    stream: String::new(),
                    full_transcript: String::new(),
                    input_tokens: 0,
                    output_tokens: 0,
                    cost_usd: 0.0,
                    latency_ms: 0,
                    summary: Vec::new(),
                    start: Some(std::time::Instant::now()),
                    prompt_file: Some(format!("{}.md", role_to_prompt_name(role))),
                    retry_count: 0,
                    error_message: None,
                });
                self.run_state = RunState::Running;
            }
            DisplayEvent::StageToken { role, token } => {
                if let Some(s) = self
                    .stages
                    .iter_mut()
                    .rev()
                    .find(|s| s.role == role && s.status == StageStatus::Running)
                {
                    s.stream.push_str(&token);
                    s.full_transcript.push_str(&token);
                    if s.stream.len() > 2000 {
                        let drop = s.stream.len() - 2000;
                        s.stream.drain(..drop);
                    }
                }
            }
            DisplayEvent::StageDone {
                role,
                summary,
                input_tokens,
                output_tokens,
                cost_usd,
                latency_ms,
            } => {
                if let Some(s) = self
                    .stages
                    .iter_mut()
                    .rev()
                    .find(|s| s.role == role && s.status == StageStatus::Running)
                {
                    s.status = StageStatus::Done;
                    s.summary = summary;
                    s.input_tokens = input_tokens;
                    s.output_tokens = output_tokens;
                    s.cost_usd = cost_usd;
                    s.latency_ms = latency_ms;
                    s.stream.clear();
                }
                let total = input_tokens.saturating_add(output_tokens) as usize;
                self.token_count = self.token_count.saturating_add(total);
                if self.context_limit > 0 {
                    self.context_usage = (self.token_count as f64) / (self.context_limit as f64);
                }
            }
            DisplayEvent::StageFailed { role, error } => {
                if let Some(s) = self
                    .stages
                    .iter_mut()
                    .rev()
                    .find(|s| s.role == role && s.status == StageStatus::Running)
                {
                    s.status = StageStatus::Failed;
                    s.summary = vec![error.clone()];
                    s.error_message = Some(error);
                    s.stream.clear();
                }
                self.run_state = RunState::Failed;
            }
            DisplayEvent::Revision { round, max, issues } => {
                self.revision_round = round;
                self.max_revision_rounds = max;
                self.notes.push((
                    format!("Revision {} of {} requested", round, max),
                    theme::warning(),
                ));
                for i in &issues {
                    self.notes.push((format!("  {}", i), theme::text_dim()));
                }
                self.run_state = RunState::AwaitingReviewer;
            }
            DisplayEvent::DiffContent(diff) => {
                self.diff_content = Some(diff);
            }
            DisplayEvent::ReportContent(report) => {
                self.report_content = Some(report);
            }
            DisplayEvent::CostJson(json) => {
                self.cost_json = Some(json);
            }
            DisplayEvent::TestLogContent(content) => {
                self.test_log = Some(content);
            }
            DisplayEvent::ArtifactsDir(dir) => {
                self.artifacts_dir = Some(PathBuf::from(dir));
            }
            DisplayEvent::BranchName(name) => {
                self.branch_name = name;
            }
            DisplayEvent::ChatMessage { role, text } => {
                self.chat_log.push((role, text));
            }
            DisplayEvent::StageTotals {
                input_tokens,
                output_tokens,
                cost_usd,
                latency_ms,
            } => {
                if let Some(s) = self
                    .stages
                    .iter_mut()
                    .rev()
                    .find(|s| s.status == StageStatus::Running)
                {
                    s.input_tokens = input_tokens;
                    s.output_tokens = output_tokens;
                    s.cost_usd = cost_usd;
                    s.latency_ms = latency_ms;
                }
            }
            DisplayEvent::Final => {
                self.finished = true;
                self.run_state = RunState::AwaitingApproval;
            }
            DisplayEvent::PermissionRequest {
                command,
                response_tx,
                ..
            } => {
                self.permission_request = Some(PermissionRequest {
                    tool_name: "sandbox_exec".to_string(),
                    command: command.clone(),
                    description: String::new(),
                    params: None,
                    response_tx,
                });
                self.permission_selected = 0;
                if !self.show_permission_modal {
                    self.show_permission_modal = true;
                    crate::display::notify::permission_needed(&command);
                }
            }
            DisplayEvent::SteerChannel(tx) => {
                self.steer_channel = Some(tx);
            }
        }
    }

    /// Advance the animation tick.
    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    /// Get total tokens across all pipeline stages.
    pub fn totals(&self) -> (u32, u32, f64, u64) {
        let mut in_t = 0u32;
        let mut out_t = 0u32;
        let mut cost = 0.0f64;
        let mut ms = 0u64;
        for s in &self.stages {
            in_t += s.input_tokens;
            out_t += s.output_tokens;
            cost += s.cost_usd;
            ms += s.latency_ms;
        }
        (in_t, out_t, cost, ms)
    }

    /// Alias for apply_display_event (compatibility with tui.rs).
    pub fn apply_event(&mut self, ev: DisplayEvent) {
        self.apply_display_event(ev);
    }

    /// Get the currently running stage, if any.
    pub fn active_stage(&self) -> Option<&StageInfo> {
        self.stages
            .iter()
            .find(|s| s.status == StageStatus::Running)
    }

    /// Whether any pipeline stage is currently running.
    pub fn has_running_stage(&self) -> bool {
        self.stages.iter().any(|s| s.status == StageStatus::Running)
    }
}

/// Build the default slash command list.
fn default_commands() -> Vec<Command> {
    vec![
        Command {
            name: "/help".to_string(),
            description: "Show help information".to_string(),
            action: CommandAction::Help,
        },
        Command {
            name: "/doctor".to_string(),
            description: "Check providers, auth, and sandbox health".to_string(),
            action: CommandAction::Doctor,
        },
        Command {
            name: "/review".to_string(),
            description: "Trigger code review audit on workspace changes".to_string(),
            action: CommandAction::Review,
        },
        Command {
            name: "/diff".to_string(),
            description: "Show unified diff of current changes".to_string(),
            action: CommandAction::ShowDiff,
        },
        Command {
            name: "/compact".to_string(),
            description: "Compact conversation context".to_string(),
            action: CommandAction::Compact,
        },
        Command {
            name: "/clear".to_string(),
            description: "Clear conversation history".to_string(),
            action: CommandAction::Clear,
        },
        Command {
            name: "/cost".to_string(),
            description: "Show token usage & cost breakdown".to_string(),
            action: CommandAction::ShowCost,
        },
        Command {
            name: "/context".to_string(),
            description: "Show context window utilization".to_string(),
            action: CommandAction::Context,
        },
        Command {
            name: "/model".to_string(),
            description: "Switch active LLM model".to_string(),
            action: CommandAction::SwitchModel(String::new()),
        },
        Command {
            name: "/config".to_string(),
            description: "Open configuration settings".to_string(),
            action: CommandAction::Config,
        },
        Command {
            name: "/theme".to_string(),
            description: "Cycle color theme (dark/light/auto)".to_string(),
            action: CommandAction::CycleTheme,
        },
        Command {
            name: "/init".to_string(),
            description: "Scan project and initialize .niki configuration".to_string(),
            action: CommandAction::Init,
        },
        Command {
            name: "/terminal-setup".to_string(),
            description: "Guide terminal truecolor & OSC 52 clipboard setup".to_string(),
            action: CommandAction::TerminalSetup,
        },
        Command {
            name: "/undo".to_string(),
            description: "Undo last checkpoint".to_string(),
            action: CommandAction::Undo,
        },
        Command {
            name: "/redo".to_string(),
            description: "Redo last undone checkpoint".to_string(),
            action: CommandAction::Redo,
        },
        Command {
            name: "/rewind".to_string(),
            description: "Rewind to previous checkpoint".to_string(),
            action: CommandAction::Rewind,
        },
        Command {
            name: "/steer".to_string(),
            description: "Send a live correction to the running agent".to_string(),
            action: CommandAction::Steer,
        },
    ]
}

/// Store event types for reactive state updates.
#[derive(Debug, Clone)]
pub enum StoreEvent {
    UserInput(String),
    PipelineEvent(DisplayEvent),
    Navigate(PageId),
    ScrollUp,
    ScrollDown,
    ToggleAutoScroll,
    Tick,
}

/// Reactive state store with subscriber notification.
pub struct Store {
    state: AppState,
    event_tx: Sender<StoreEvent>,
    event_rx: Receiver<StoreEvent>,
}

impl Store {
    /// Create a new store with the given initial state.
    pub fn new(state: AppState) -> Self {
        let (event_tx, event_rx) = mpsc::channel();
        Self {
            state,
            event_tx,
            event_rx,
        }
    }

    /// Get a reference to the current state.
    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// Get a mutable reference to the state.
    pub fn state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }

    /// Get the event sender for dispatching events.
    pub fn sender(&self) -> Sender<StoreEvent> {
        self.event_tx.clone()
    }

    /// Try to receive the next event (non-blocking).
    pub fn try_recv(&self) -> Option<StoreEvent> {
        self.event_rx.try_recv().ok()
    }

    /// Dispatch an event, updating state accordingly.
    pub fn dispatch(&mut self, event: StoreEvent) {
        match event {
            StoreEvent::UserInput(input) => {
                self.state.messages.push(Message::User {
                    content: input,
                    timestamp: Utc::now(),
                });
                if self.state.auto_scroll {
                    self.state.scroll_offset = 0;
                }
            }
            StoreEvent::PipelineEvent(ev) => {
                self.state.apply_display_event(ev);
            }
            StoreEvent::Navigate(page) => {
                self.state.view = ViewMode::Page(page);
            }
            StoreEvent::ScrollUp => {
                self.state.scroll_offset = self.state.scroll_offset.saturating_add(1);
                self.state.auto_scroll = false;
            }
            StoreEvent::ScrollDown => {
                let new_offset = self.state.scroll_offset.saturating_sub(1);
                self.state.scroll_offset = new_offset;
                // Re-enable auto-scroll when user scrolls back to bottom.
                if new_offset == 0 {
                    self.state.auto_scroll = true;
                }
            }
            StoreEvent::ToggleAutoScroll => {
                self.state.auto_scroll = !self.state.auto_scroll;
            }
            StoreEvent::Tick => {
                self.state.tick();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_state_insert() {
        let mut input = InputState::new();
        input.insert_char('h');
        input.insert_char('i');
        assert_eq!(input.buffer, "hi");
        assert_eq!(input.cursor_pos, 2);
    }

    #[test]
    fn input_state_delete_back() {
        let mut input = InputState::new();
        input.insert_char('a');
        input.insert_char('b');
        input.delete_back();
        assert_eq!(input.buffer, "a");
        assert_eq!(input.cursor_pos, 1);
    }

    #[test]
    fn input_state_move_cursor() {
        let mut input = InputState::new();
        input.insert_char('a');
        input.insert_char('b');
        input.insert_char('c');
        input.move_left();
        assert_eq!(input.cursor_pos, 2);
        input.move_left();
        assert_eq!(input.cursor_pos, 1);
        input.move_left();
        assert_eq!(input.cursor_pos, 0);
        input.move_left();
        assert_eq!(input.cursor_pos, 0);
        input.move_right();
        assert_eq!(input.cursor_pos, 1);
    }

    #[test]
    fn input_state_history() {
        let mut input = InputState::new();
        input.buffer = "first".to_string();
        input.cursor_pos = 5;
        input.clear();
        input.buffer = "second".to_string();
        input.cursor_pos = 6;
        input.clear();

        assert_eq!(input.history.len(), 2);
        input.history_prev();
        assert_eq!(input.buffer, "second");
        input.history_prev();
        assert_eq!(input.buffer, "first");
        input.history_next();
        assert_eq!(input.buffer, "second");
    }

    #[test]
    fn app_state_new() {
        let config = NikiConfig::default();
        let state = AppState::new("test task".to_string(), config, ".".into());
        assert_eq!(state.view, ViewMode::Chat);
        assert_eq!(state.messages.len(), 0);
        assert!(state.auto_scroll);
    }

    #[test]
    fn store_dispatch_user_input() {
        let config = NikiConfig::default();
        let state = AppState::new("test".to_string(), config, ".".into());
        let mut store = Store::new(state);
        store.dispatch(StoreEvent::UserInput("Hello".to_string()));
        assert_eq!(store.state().messages.len(), 1);
    }

    #[test]
    fn store_dispatch_navigate() {
        let config = NikiConfig::default();
        let state = AppState::new("test".to_string(), config, ".".into());
        let mut store = Store::new(state);
        store.dispatch(StoreEvent::Navigate(PageId::Pipeline));
        assert_eq!(store.state().view, ViewMode::Page(PageId::Pipeline));
    }

    #[test]
    fn default_commands_exist() {
        let commands = default_commands();
        assert!(!commands.is_empty());
        assert!(commands.iter().any(|c| c.name == "/help"));
        assert!(commands.iter().any(|c| c.name == "/theme"));
    }

    #[test]
    fn page_id_titles() {
        assert_eq!(PageId::Run.title(), "run");
        assert_eq!(PageId::Pipeline.title(), "pipeline");
        assert_eq!(PageId::Help.title(), "help");
    }

    #[test]
    fn input_state_insert_str_normalizes_crlf() {
        let mut input = InputState::new();
        input.insert_str("line1\r\nline2\rline3\nline4");
        assert_eq!(input.buffer, "line1\nline2\nline3\nline4");
        assert_eq!(input.cursor_pos, input.buffer.len());
    }
}
