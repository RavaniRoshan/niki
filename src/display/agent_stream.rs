use crate::artifacts::types::{AgentRole, ReviewIssue};
use crate::config::NikiConfig;
use crate::display::theme::Theme;
use crate::NikiError;
use crate::orchestrator::pipeline::{PipelineResult, Task};
use crate::orchestrator::state::PipelineState;
use console::Term;
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
    pub token_count: Option<u32>,
    pub summary_lines: Vec<String>,
}

pub struct AgenticDisplay {
    theme: Theme,
    term: Term,
    term_width: u16,
    is_tty: bool,
    stages: Vec<StageState>,
    current_streaming_lines: usize,
}

fn role_label(role: AgentRole) -> &'static str {
    match role {
        AgentRole::Planner => "Planner",
        AgentRole::Coder => "Coder",
        AgentRole::Tester => "Tester",
        AgentRole::Reviewer => "Reviewer",
    }
}

impl AgenticDisplay {
    pub fn new() -> Self {
        let term = Term::stdout();
        let is_tty = term.is_term();
        let term_width = term.size().1;
        Self {
            theme: Theme::new(),
            term,
            term_width,
            is_tty,
            stages: vec![],
            current_streaming_lines: 0,
        }
    }

    pub fn is_tty(&self) -> bool {
        self.is_tty
    }

    /// Plain timestamped log line, used only in non-TTY (piped/CI) mode.
    fn log(&self, label: &str, msg: &str) {
        if !self.is_tty {
            let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
            let _ = self.term.write_line(&format!("[{}] [{}] {}", ts, label, msg));
        }
    }

    pub fn show_banner(&self, task: &Task, config: &NikiConfig) {
        crate::display::banner::show_banner(task, config, self.is_tty);
    }

    fn agent_icon(&self, role: AgentRole) -> &'static str {
        match role {
            AgentRole::Planner => self.theme.planner.icon,
            AgentRole::Coder => self.theme.coder.icon,
            AgentRole::Tester => self.theme.tester.icon,
            AgentRole::Reviewer => self.theme.reviewer.icon,
        }
    }

    fn agent_name(&self, role: AgentRole) -> &'static str {
        match role {
            AgentRole::Planner => self.theme.planner.name,
            AgentRole::Coder => self.theme.coder.name,
            AgentRole::Tester => self.theme.tester.name,
            AgentRole::Reviewer => self.theme.reviewer.name,
        }
    }

    fn agent_style(&self, role: AgentRole) -> console::Style {
        match role {
            AgentRole::Planner => self.theme.planner.label_style.clone(),
            AgentRole::Coder => self.theme.coder.label_style.clone(),
            AgentRole::Tester => self.theme.tester.label_style.clone(),
            AgentRole::Reviewer => self.theme.reviewer.label_style.clone(),
        }
    }

    pub fn agent_start(&mut self, role: AgentRole) {
        // Bookkeeping happens in both modes.
        self.stages.push(StageState {
            role,
            status: StageStatus::Running,
            start_time: Some(Instant::now()),
            elapsed: None,
            token_count: None,
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

    pub fn agent_done(&mut self, role: AgentRole, summary: Vec<String>, tokens: u32) {
        self.clear_streaming_output();

        let elapsed = if let Some(stage) = self.stages.last_mut() {
            if stage.role == role {
                stage.status = StageStatus::Done;
                stage.elapsed = stage.start_time.map(|s| s.elapsed());
                stage.token_count = Some(tokens);
                stage.summary_lines = summary.clone();
                stage.elapsed
            } else {
                None
            }
        } else {
            None
        };

        if !self.is_tty {
            let secs = elapsed.map(|d| d.as_secs()).unwrap_or(0);
            let tok = format!("{:.1}k tokens", tokens as f64 / 1000.0);
            let summary_str = summary
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            let msg = if summary_str.is_empty() {
                format!("Done ({}s, {})", secs, tok)
            } else {
                format!("Done ({}s, {}) — {}", secs, tok, summary_str)
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

        let header = format!(
            " {} {}                                        {} {}s  {} tk",
            style.apply_to(self.agent_icon(role)),
            style.apply_to(name),
            self.theme.success.apply_to("✓"),
            duration,
            tokens
        );
        let _ = self.term.write_line(&header);

        for line in summary {
            let _ = self.term.write_line(&format!("   {}", line));
        }
        let _ = self.term.write_line("");
    }

    pub fn agent_failed(&mut self, role: AgentRole, error: &str) {
        self.clear_streaming_output();
        if !self.is_tty {
            self.log(role_label(role), &format!("Error: {}", error));
            return;
        }
        let _ = self.term.write_line(&format!(" {} {}", self.theme.error.apply_to("✗"), error));
    }

    pub fn revision_requested(&mut self, round: u32, max: u32, issues: &[ReviewIssue]) {
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
        crate::display::completion::render_completion(
            result,
            &self.theme,
            self.is_tty,
            branch,
            task_dir,
        );
    }

    pub fn show_failure(&self, error: &NikiError, _state: &PipelineState) {
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
