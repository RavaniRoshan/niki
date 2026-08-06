//! Reactive state management for the conversational chat interface.
//!
//! Replaces the imperative `apply_event()` pattern with a reactive Store pattern:
//! - Centralized `AppState` with all UI state
//! - `Store` manages state mutations and subscriber notifications
//! - Events are dispatched through the store, triggering re-renders

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use chrono::{DateTime, Utc};
use ratatui::crossterm::event::KeyEvent;
use ratatui::style::Color;
use uuid::Uuid;

use crate::artifacts::types::AgentRole;
use crate::config::NikiConfig;
use crate::display::theme;
use crate::display::tui::DisplayEvent;

/// View mode — chat or page-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Chat,
    Page(PageId),
}

impl Default for ViewMode {
    fn default() -> Self {
        ViewMode::Chat
    }
}

/// Page identifiers (re-exported from pages module for convenience).
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
}

impl PageId {
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
        }
    }
}

/// Input modes (matching Claude Code / Kimi Code).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Insert,
    Command,
    Shell,
}

impl Default for InputMode {
    fn default() -> Self {
        InputMode::Insert
    }
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
}

impl InputState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset the input buffer after submission.
    pub fn clear(&mut self) {
        if !self.buffer.is_empty() {
            self.history.push(self.buffer.clone());
        }
        self.buffer.clear();
        self.cursor_pos = 0;
        self.history_index = None;
        self.autocomplete = None;
    }

    /// Insert a character at the cursor position.
    pub fn insert_char(&mut self, c: char) {
        self.buffer.insert(self.cursor_pos, c);
        self.cursor_pos += 1;
    }

    /// Delete the character before the cursor (backspace).
    pub fn delete_back(&mut self) -> bool {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            self.buffer.remove(self.cursor_pos);
            true
        } else {
            false
        }
    }

    /// Delete the character at the cursor (delete key).
    pub fn delete_forward(&mut self) -> bool {
        if self.cursor_pos < self.buffer.len() {
            self.buffer.remove(self.cursor_pos);
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

    /// Navigate to previous history entry.
    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        match self.history_index {
            None => {
                self.history_index = Some(self.history.len() - 1);
                self.buffer = self.history[self.history.len() - 1].clone();
                self.cursor_pos = self.buffer.len();
            }
            Some(idx) if idx > 0 => {
                self.history_index = Some(idx - 1);
                self.buffer = self.history[idx - 1].clone();
                self.cursor_pos = self.buffer.len();
            }
            _ => {}
        }
    }

    /// Navigate to next history entry.
    pub fn history_next(&mut self) {
        match self.history_index {
            Some(idx) if idx + 1 < self.history.len() => {
                self.history_index = Some(idx + 1);
                self.buffer = self.history[idx + 1].clone();
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
#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub tool_name: String,
    pub command: String,
    pub description: String,
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
}

/// The main application state — single source of truth for the UI.
#[derive(Debug)]
pub struct AppState {
    /// Current view mode (chat or page).
    pub view: ViewMode,
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
    /// Whether the permission modal is visible.
    pub show_permission_modal: bool,
    /// Current permission request (if any).
    pub permission_request: Option<PermissionRequest>,
    /// Selected permission option (0=allow once, 1=allow always, 2=deny).
    pub permission_selected: usize,
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
    /// Pipeline state (for page view).
    pub pipeline: PipelineState,
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
}

/// Pipeline state for page view.
#[derive(Debug, Clone, Default)]
pub struct PipelineState {
    pub stages: Vec<StageInfo>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Idle,
    Running,
    AwaitingReviewer,
    AwaitingApproval,
    Failed,
    Cancelled,
}

impl Default for RunState {
    fn default() -> Self {
        RunState::Idle
    }
}

impl AppState {
    /// Create a new AppState with default values.
    pub fn new(description: String, config: NikiConfig, project_path: PathBuf) -> Self {
        Self {
            view: ViewMode::Chat,
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
            pipeline: PipelineState::default(),
            run_state: RunState::Idle,
            revision_round: 1,
            max_revision_rounds: config.general.max_revision_rounds,
            description,
            notes: Vec::new(),
            diff_content: None,
            report_content: None,
            cost_json: None,
            test_log: None,
            artifacts_dir: None,
            finished: false,
        }
    }

    /// Get the theme background color for the current theme mode.
    pub fn theme_bg(&self) -> Color {
        theme::bg_color()
    }

    /// Apply a legacy DisplayEvent to update pipeline-related state.
    pub fn apply_display_event(&mut self, ev: DisplayEvent) {
        match ev {
            DisplayEvent::Banner { description } => {
                self.description = description;
            }
            DisplayEvent::StageStart { role } => {
                self.pipeline.stages.push(StageInfo {
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
                });
                self.run_state = RunState::Running;
            }
            DisplayEvent::StageToken { role, token } => {
                if let Some(s) = self
                    .pipeline
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
                    .pipeline
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
            }
            DisplayEvent::StageFailed { role, error } => {
                if let Some(s) = self
                    .pipeline
                    .stages
                    .iter_mut()
                    .rev()
                    .find(|s| s.role == role && s.status == StageStatus::Running)
                {
                    s.status = StageStatus::Failed;
                    s.summary = vec![error];
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
                    self.notes
                        .push((format!("  {}", i), theme::text_dim()));
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
            DisplayEvent::StageTotals {
                input_tokens,
                output_tokens,
                cost_usd,
                latency_ms,
            } => {
                if let Some(s) = self.pipeline.stages.iter_mut().rev().find(|s| s.status == StageStatus::Running) {
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
        for s in &self.pipeline.stages {
            in_t += s.input_tokens;
            out_t += s.output_tokens;
            cost += s.cost_usd;
            ms += s.latency_ms;
        }
        (in_t, out_t, cost, ms)
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
            name: "/compact".to_string(),
            description: "Compact conversation context".to_string(),
            action: CommandAction::Compact,
        },
        Command {
            name: "/clear".to_string(),
            description: "Clear conversation".to_string(),
            action: CommandAction::Clear,
        },
        Command {
            name: "/cost".to_string(),
            description: "Show cost breakdown".to_string(),
            action: CommandAction::ShowCost,
        },
        Command {
            name: "/diff".to_string(),
            description: "Show current diff".to_string(),
            action: CommandAction::ShowDiff,
        },
        Command {
            name: "/model".to_string(),
            description: "Switch model".to_string(),
            action: CommandAction::SwitchModel(String::new()),
        },
        Command {
            name: "/pipeline".to_string(),
            description: "Show pipeline status".to_string(),
            action: CommandAction::ShowPipeline,
        },
        Command {
            name: "/tui".to_string(),
            description: "Switch TUI mode".to_string(),
            action: CommandAction::SwitchTuiMode,
        },
        Command {
            name: "/theme".to_string(),
            description: "Cycle theme".to_string(),
            action: CommandAction::CycleTheme,
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
                self.state.scroll_offset = self.state.scroll_offset.saturating_sub(1);
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
}
