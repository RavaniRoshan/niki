use crate::artifacts::types::{AgentRole, ReviewIssue};
use crate::config::NikiConfig;
use crate::display::theme::Theme;
use crate::display::tui::{spawn_tui, DisplayEvent};
use crate::NikiError;
use crate::llm::provider::TokenUsage;
use crate::orchestrator::pipeline::{PipelineResult, Task};
use crate::orchestrator::state::PipelineState;
use console::Term;
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub enum StageStatus {
    Pending,
    Running,
    Done,
    Failed,
    Revision,
}

pub struct StageState {
    pub role: AgentRole,
    pub status: StageStatus,
    pub start_time: Option<Instant>,
    pub elapsed: Option<Duration>,
    /// Real token usage reported by the LLM provider for this stage.
    pub usage: Option<TokenUsage>,
    /// Estimated USD cost for this stage (0.0 when the model is unknown).
    pub cost_usd: Option<f64>,
    pub summary_lines: Vec<String>,
}

pub struct AgenticDisplay {
    theme: Theme,
    term: Term,
    is_tty: bool,
    stages: Vec<StageState>,
    current_streaming_lines: usize,
    /// When the rich TUI is enabled, events are forwarded here instead of being
    /// rendered inline. The paired OS thread renders them.
    tui: Option<Sender<DisplayEvent>>,
    tui_thread: Option<JoinHandle<()>>,
    /// Role currently streaming tokens (for routing `stream_token` to the TUI).
    current_role: Option<AgentRole>,
}

fn role_label(role: AgentRole) -> &'static str {
    match role {
        AgentRole::Planner => "Planner",
        AgentRole::Coder => "Coder",
        AgentRole::Tester => "Tester",
        AgentRole::Reviewer => "Reviewer",
        AgentRole::Synthesizer => "Synthesizer",
        AgentRole::SecurityAuditor => "SecurityAuditor",
    }
}

impl AgenticDisplay {
    pub fn new() -> Self {
        let term = Term::stdout();
        let is_tty = term.is_term();
        Self {
            theme: Theme::new(),
            term,
            is_tty,
            stages: vec![],
            current_streaming_lines: 0,
            tui: None,
            tui_thread: None,
            current_role: None,
        }
    }

    pub fn is_tty(&self) -> bool {
        self.is_tty
    }

    /// Create a cheap independent instance of the display that forwards events to the
    /// same TUI render thread (if active). Used when running agents concurrently
    /// (parallel coders): each concurrent task owns its own `AgenticDisplay` so
    /// its streaming/state bookkeeping never contends, while all events still land
    /// on the single visible TUI.
    pub fn fork(&self) -> Self {
        Self {
            theme: self.theme.clone(),
            term: Term::stdout(),
            is_tty: self.is_tty,
            stages: Vec::new(),
            current_streaming_lines: 0,
            tui: self.tui.clone(),
            tui_thread: None,
            current_role: None,
        }
    }

    /// Enable the rich terminal TUI. Spawns the render thread; subsequent
    /// display calls forward events to it instead of printing inline.
    pub fn enable_tui(&mut self, description: String) {
        if !self.is_tty {
            return;
        }
        let (tx, handle) = spawn_tui(description);
        self.tui = Some(tx);
        self.tui_thread = Some(handle);
    }

    /// Signal the TUI thread to finish and wait for it to restore the terminal.
    /// Call once after the pipeline completes (and after `show_completion`).
    pub fn finish_tui(&mut self) {
        // Dropping the sender makes the render thread observe channel closure.
        self.tui = None;
        if let Some(handle) = self.tui_thread.take() {
            let _ = handle.join();
        }
    }

    /// Forward an event to the TUI thread if it's active.
    fn emit(&self, ev: DisplayEvent) {
        if let Some(tx) = &self.tui {
            let _ = tx.send(ev);
        }
    }

    /// Plain timestamped log line, used only in non-TTY (piped/CI) mode.
    fn log(&self, label: &str, msg: &str) {
        if !self.is_tty {
            let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
            let _ = self.term.write_line(&format!("[{}] [{}] {}", ts, label, msg));
        }
    }

    pub fn show_banner(&self, task: &Task, config: &NikiConfig) {
        if self.tui.is_some() {
            self.emit(DisplayEvent::Banner {
                description: task.description.clone(),
            });
            return;
        }
        crate::display::banner::show_banner(task, config, self.is_tty);
    }

    fn agent_icon(&self, role: AgentRole) -> &'static str {
        match role {
            AgentRole::Planner => self.theme.planner.icon,
            AgentRole::Coder => self.theme.coder.icon,
            AgentRole::Tester => self.theme.tester.icon,
            AgentRole::Reviewer => self.theme.reviewer.icon,
            AgentRole::Synthesizer => self.theme.synthesizer.icon,
            AgentRole::SecurityAuditor => self.theme.security_auditor.icon,
        }
    }

    fn agent_name(&self, role: AgentRole) -> &'static str {
        match role {
            AgentRole::Planner => self.theme.planner.name,
            AgentRole::Coder => self.theme.coder.name,
            AgentRole::Tester => self.theme.tester.name,
            AgentRole::Reviewer => self.theme.reviewer.name,
            AgentRole::Synthesizer => self.theme.synthesizer.name,
            AgentRole::SecurityAuditor => self.theme.security_auditor.name,
        }
    }

    fn agent_style(&self, role: AgentRole) -> console::Style {
        match role {
            AgentRole::Planner => self.theme.planner.label_style.clone(),
            AgentRole::Coder => self.theme.coder.label_style.clone(),
            AgentRole::Tester => self.theme.tester.label_style.clone(),
            AgentRole::Reviewer => self.theme.reviewer.label_style.clone(),
            AgentRole::Synthesizer => self.theme.synthesizer.label_style.clone(),
            AgentRole::SecurityAuditor => self.theme.security_auditor.label_style.clone(),
        }
    }

    pub fn agent_start(&mut self, role: AgentRole) {
        // In TUI mode, just notify the render thread and return.
        if self.tui.is_some() {
            self.current_role = Some(role);
            self.emit(DisplayEvent::StageStart { role });
            return;
        }

        // Bookkeeping happens in both modes.
        self.stages.push(StageState {
            role,
            status: StageStatus::Running,
            start_time: Some(Instant::now()),
            elapsed: None,
            usage: None,
            cost_usd: None,
            summary_lines: vec![],
        });

        if !self.is_tty {
            self.log(role_label(role), "Starting...");
            self.current_streaming_lines = 0;
            return;
        }

        let (name, style) = (self.agent_name(role), self.agent_style(role));
        let header = format!(
            " {} {}                                              ⠋",
            style.apply_to(self.agent_icon(role)),
            style.apply_to(name)
        );
        let _ = self.term.write_line(&header);
        let separator = " ─────────────────────────────────────────────────────────────";
        let _ = self.term.write_line(&self.theme.border.apply_to(separator).to_string());
        self.current_streaming_lines = 2; // header + separator
    }

    pub fn stream_token(&mut self, token: &str) {
        if self.tui.is_some() {
            if let Some(role) = self.current_role {
                self.emit(DisplayEvent::StageToken {
                    role,
                    token: token.to_string(),
                });
            }
            return;
        }

        if !self.is_tty {
            // In non-TTY mode we skip streaming entirely and only show summaries.
            return;
        }
        let newlines = token.chars().filter(|c| *c == '\n').count();
        self.current_streaming_lines += newlines;
        print!("{}", token);
        use std::io::Write;
        let _ = std::io::stdout().flush();
    }

    pub fn agent_done(&mut self, role: AgentRole, summary: Vec<String>, usage: TokenUsage, cost_usd: f64) {
        if self.tui.is_some() {
            let latency_ms = self
                .stages
                .last()
                .and_then(|s| s.start_time)
                .map(|t| t.elapsed().as_millis() as u64)
                .unwrap_or(0);
            self.emit(DisplayEvent::StageDone {
                role,
                summary: summary.clone(),
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cost_usd,
                latency_ms,
            });
            return;
        }

        self.clear_streaming_output();

        let elapsed = if let Some(stage) = self.stages.last_mut() {
            if stage.role == role {
                stage.status = StageStatus::Done;
                stage.elapsed = stage.start_time.map(|s| s.elapsed());
                stage.usage = Some(usage.clone());
                stage.cost_usd = Some(cost_usd);
                stage.summary_lines = summary.clone();
                stage.elapsed
            } else {
                None
            }
        } else {
            None
        };

        let total_tokens = usage.input_tokens + usage.output_tokens;

        if !self.is_tty {
            let secs = elapsed.map(|d| d.as_secs()).unwrap_or(0);
            let tok = format!(
                "in {} / out {} ({:.1}k tok)",
                usage.input_tokens,
                usage.output_tokens,
                total_tokens as f64 / 1000.0
            );
            let cost = if cost_usd > 0.0 {
                format!(", ${:.4}", cost_usd)
            } else {
                String::new()
            };
            let summary_str = summary
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            let msg = if summary_str.is_empty() {
                format!("Done ({}s, {}{})", secs, tok, cost)
            } else {
                format!("Done ({}s, {}{}) — {}", secs, tok, cost, summary_str)
            };
            self.log(role_label(role), &msg);
            return;
        }

        let (name, style) = (self.agent_name(role), self.agent_style(role));

        let duration = self
            .stages
            .last()
            .and_then(|s| s.elapsed)
            .unwrap_or(Duration::from_secs(0))
            .as_secs();

        let cost = if cost_usd > 0.0 {
            format!("  ${:.4}", cost_usd)
        } else {
            String::new()
        };

        let header = format!(
            " {} {}                                  {} {}s  {} tk{}",
            style.apply_to(self.agent_icon(role)),
            style.apply_to(name),
            self.theme.success.apply_to("✓"),
            duration,
            total_tokens,
            cost
        );
        let _ = self.term.write_line(&header);

        for line in summary {
            let _ = self.term.write_line(&format!("   {}", line));
        }
        let _ = self.term.write_line("");
    }

    pub fn agent_failed(&mut self, role: AgentRole, error: &str) {
        if self.tui.is_some() {
            self.emit(DisplayEvent::StageFailed {
                role,
                error: error.to_string(),
            });
            return;
        }

        self.clear_streaming_output();
        if !self.is_tty {
            self.log(role_label(role), &format!("Error: {}", error));
            return;
        }
        let _ = self.term.write_line(&format!(" {} {}", self.theme.error.apply_to("✗"), error));
    }

    pub fn revision_requested(&mut self, round: u32, max: u32, issues: &[ReviewIssue]) {
        if self.tui.is_some() {
            self.emit(DisplayEvent::Revision {
                round,
                max,
                issues: issues
                    .iter()
                    .map(|i| format!("{:?}: {}", i.category, i.description))
                    .collect(),
            });
            return;
        }

        self.clear_streaming_output();

        if !self.is_tty {
            let mut msg = format!("Revision needed (round {}/{})", round, max);
            for issue in issues {
                msg.push_str(&format!(
                    " | {}: {}",
                    format!("{:?}", issue.category),
                    issue.description
                ));
            }
            self.log("Reviewer", &msg);
            return;
        }

        let header = format!(
            " {} {}                                       {} (round {}/{})",
            self.theme.reviewer.label_style.apply_to(self.theme.reviewer.icon),
            self.theme.reviewer.label_style.apply_to(self.theme.reviewer.name),
            self.theme.warning.apply_to("⟳ Revision needed"),
            round,
            max
        );
        let _ = self.term.write_line(&header);
        for issue in issues {
            let _ = self.term.write_line(&format!(
                "   • {}: {}",
                format!("{:?}", issue.category),
                issue.description
            ));
        }
        let _ = self.term.write_line("");
    }

    pub fn update_pipeline_status(&self) {
        if self.tui.is_some() {
            // The TUI thread redraws continuously; nothing to do here.
            return;
        }
        if !self.is_tty {
            // Pipeline status is rendered inline in agentic mode; in log mode the
            // per-agent start/done lines already convey progress.
            return;
        }
        crate::display::pipeline_status::update_pipeline_status(&self.stages, &self.theme);
    }

    pub fn show_completion(
        &self,
        result: &PipelineResult,
        branch: &str,
        task_dir: &std::path::Path,
    ) {
        if self.tui.is_some() {
            self.emit(DisplayEvent::Final);
            return;
        }
        crate::display::completion::render_completion(
            result,
            &self.theme,
            self.is_tty,
            branch,
            task_dir,
        );
    }

    pub fn show_failure(&self, error: &NikiError, _state: &PipelineState) {
        if self.tui.is_some() {
            self.emit(DisplayEvent::Revision {
                round: 0,
                max: 0,
                issues: vec![format!("Task failed: {}", error)],
            });
            self.emit(DisplayEvent::Final);
            return;
        }
        if !self.is_tty {
            self.log("NIKI", &format!("Task failed: {}", error));
            return;
        }
        crate::display::completion::render_failure(error, _state, &self.theme, self.is_tty);
    }

    fn clear_streaming_output(&mut self) {
        if self.current_streaming_lines > 0 && self.is_tty {
            let _ = self.term.clear_last_lines(self.current_streaming_lines);
            self.current_streaming_lines = 0;
        }
    }
}

impl Default for AgenticDisplay {
    fn default() -> Self {
        Self::new()
    }
}
