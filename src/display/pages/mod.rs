pub mod agents;
pub mod artifacts;
pub mod config;
pub mod cost;
pub mod diff;
pub mod help;
pub mod history;
pub mod pipeline;
pub mod run;
pub mod verdict;

use std::collections::HashMap;
use std::path::PathBuf;

use ratatui::Frame;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;

use crate::artifacts::types::AgentRole;
use crate::config::NikiConfig;
use crate::display::tips::TipsBanner;
use crate::display::tui::DisplayEvent;

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
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Idle,
    Running,
    AwaitingReviewer,
    AwaitingApproval,
    Failed,
    Cancelled,
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageStatus {
    Running,
    Done,
    Failed,
    Queued,
}

pub struct AgentTranscript {
    pub system: String,
    pub user: String,
    pub assistant: String,
    pub artifacts: Vec<String>,
}

pub struct AppState {
    pub current_page: PageId,
    pub run_state: RunState,
    pub task_id: Option<uuid::Uuid>,
    pub description: String,
    pub branch_name: String,
    pub revision_round: u32,
    pub max_revision_rounds: u32,
    pub stages: Vec<StageInfo>,
    pub notes: Vec<(String, ratatui::style::Color)>,
    pub finished: bool,
    pub tick: usize,
    pub paused: bool,
    pub config: NikiConfig,
    pub project_path: PathBuf,
    pub artifacts_dir: Option<PathBuf>,
    pub report_content: Option<String>,
    pub diff_content: Option<String>,
    pub cost_json: Option<String>,
    pub modal: Option<Modal>,
    pub onboarding: Option<crate::display::onboarding::OnboardingModal>,
    pub onboarded: bool,
    pub start_time: Option<std::time::Instant>,
    pub tips: TipsBanner,
}

#[derive(Debug)]
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

impl AppState {
    pub fn new(description: String, config: NikiConfig, project_path: PathBuf) -> Self {
        let tips_enabled = config.ui.tips.enabled;
        let tips_rotation = config.ui.tips.rotation_seconds;
        Self {
            current_page: PageId::Run,
            run_state: RunState::Idle,
            task_id: None,
            description,
            branch_name: String::new(),
            revision_round: 1,
            max_revision_rounds: config.general.max_revision_rounds,
            stages: Vec::new(),
            notes: Vec::new(),
            finished: false,
            tick: 0,
            paused: false,
            config,
            project_path,
            artifacts_dir: None,
            report_content: None,
            diff_content: None,
            cost_json: None,
            modal: None,
            onboarding: None,
            onboarded: false,
            start_time: None,
            tips: TipsBanner::new(tips_enabled, tips_rotation),
        }
    }

    pub fn apply_event(&mut self, ev: DisplayEvent) {
        match ev {
            DisplayEvent::Banner { description } => self.description = description,
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
            }
            DisplayEvent::StageFailed { role, error } => {
                if let Some(s) = self
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
                    ratatui::style::Color::Yellow,
                ));
                for i in &issues {
                    self.notes
                        .push((format!("  {}", i), ratatui::style::Color::DarkGray));
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
            DisplayEvent::ArtifactsDir(dir) => {
                self.artifacts_dir = Some(std::path::PathBuf::from(dir));
            }
            DisplayEvent::Final => {
                self.finished = true;
                self.run_state = RunState::AwaitingApproval;
            }
        }
    }

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

    pub fn active_stage(&self) -> Option<&StageInfo> {
        self.stages
            .iter()
            .find(|s| s.status == StageStatus::Running)
    }
}

pub trait Page {
    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState);
    fn handle_key(&mut self, key: KeyEvent, state: &mut AppState) -> bool;
    fn title(&self) -> &str;
}

pub struct PageRouter {
    pub pages: HashMap<PageId, Box<dyn Page>>,
}

impl PageRouter {
    pub fn new() -> Self {
        let mut pages: HashMap<PageId, Box<dyn Page>> = HashMap::new();
        pages.insert(PageId::Run, Box::new(run::RunPage::new()));
        pages.insert(PageId::Pipeline, Box::new(pipeline::PipelinePage::new()));
        pages.insert(PageId::Agents, Box::new(agents::AgentsPage::new()));
        pages.insert(PageId::Diff, Box::new(diff::DiffPage::new()));
        pages.insert(PageId::Verdict, Box::new(verdict::VerdictPage::new()));
        pages.insert(PageId::Cost, Box::new(cost::CostPage::new()));
        pages.insert(PageId::Artifacts, Box::new(artifacts::ArtifactsPage::new()));
        pages.insert(PageId::History, Box::new(history::HistoryPage::new()));
        pages.insert(PageId::Config, Box::new(config::ConfigPage::new()));
        pages.insert(PageId::Help, Box::new(help::HelpPage::new()));
        Self { pages }
    }

    pub fn render_current(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        if let Some(page) = self.pages.get(&state.current_page) {
            page.render(frame, area, state);
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent, state: &mut AppState) -> bool {
        if let Some(page) = self.pages.get_mut(&state.current_page) {
            page.handle_key(key, state)
        } else {
            false
        }
    }
}
