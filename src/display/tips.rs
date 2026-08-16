use std::time::{Duration, Instant};

/// Curated tips for using NIKI, covering shortcuts, pipeline stages, config, safety, and goals.
const TIPS: &[&str] = &[
    // Keyboard shortcuts
    "Press j/k to scroll, Space to pause, q to quit",
    "Switch pages: p=Pipeline, a=Agents, d=Diff, v=Verdict, c=Cost",
    "Press f for Artifacts, h for History, , for Config, ? for Help",
    "In the Run page, streaming output updates in real-time",
    "Press Enter on the final diff to approve and write changes",
    // Pipeline stages
    "Pipeline: Planner → Coder → Tester → Reviewer (with Red/Blue verification)",
    "Planner breaks your task into a structured spec before any code is written",
    "Coder implements changes in an isolated git worktree for safety",
    "Tester runs your test suite and reports failures back to the pipeline",
    "Reviewer performs independent verification — not a rubber stamp",
    "Red agent adversarially probes the Coder's diff before the Reviewer runs",
    "Synthesizer reconciles parallel coder diffs into one coherent change",
    "SecurityAuditor optionally runs after the Reviewer for an independent audit",
    "Each stage reports token usage and cost — check the Cost page for breakdowns",
    // Config options
    "Use --tui to enable the rich terminal interface during a run",
    "Use --dry-run to preview what NIKI would do without making changes",
    "Use --branch <name> to target a specific branch for your changes",
    "Set max_revision_rounds in [general] to control how many fix iterations",
    "Configure [pipeline.topology] to force Auto, MultiAgent, or SingleAgent mode",
    "Set [parallel].enabled=true with coder_count>1 for parallel coding",
    "Enable [security].enabled=true for an independent security audit pass",
    "Toggle [red_blue].enabled to control adversarial verification (default: on)",
    // Safety features
    "Hermetic proof: all changes happen in isolated git worktrees — your working tree is never modified",
    "Scope lock: NIKI only touches files within its declared scope — no surprise edits",
    "The pipeline auto-rolls back on failure, leaving your repo clean",
    "Diff review: always inspect the final diff before approving changes",
    "Token budget: check the Cost page to monitor spending during long runs",
    // Goal system
    "Goal system: set autonomous goals with /goal and NIKI runs until complete",
    "Goals support retry with configurable attempts and delay between retries",
    "Goal branches are prefixed with [goal.branch_prefix] — easy to find and clean up",
    "Goal state is persisted in [goal.state_dir] so you can resume interrupted goals",
    "Set [goal].fail_fast=false to continue goal iteration even if one step fails",
    // Tips & tricks
    "Use the Config page (,) to view your current niki.toml settings",
    "The History page shows all previous runs and their outcomes",
    "Cost page breaks down token usage and spend per pipeline stage",
    "Artifacts page lists generated files — diff, report, and test output",
    "The pipeline's revision loop stops when the Reviewer gives a terminal verdict",
    "NIKI supports Anthropic, OpenAI, and Google providers — configure in niki.toml",
    "Set ANTHROPIC_API_KEY or OPENAI_API_KEY in your environment for quick setup",
    "The Verdict page shows the Reviewer's detailed assessment of the changes",
    "Pipeline page shows the live progress of each agent stage",
    "Press q or Esc from any page to exit the TUI",
    "NIKI works with any git repository — just run from the project root",
    "Check the Help page (?) for a full keyboard shortcut reference",
];

pub struct TipsBanner {
    current_index: usize,
    last_rotate: Instant,
    rotation_interval: Duration,
    enabled: bool,
}

impl TipsBanner {
    pub fn new(enabled: bool, rotation_seconds: u64) -> Self {
        Self {
            current_index: 0,
            last_rotate: Instant::now(),
            rotation_interval: Duration::from_secs(rotation_seconds),
            enabled,
        }
    }

    /// Advance to the next tip if the rotation interval has elapsed.
    pub fn rotate(&mut self) {
        if !self.enabled {
            return;
        }
        if self.last_rotate.elapsed() >= self.rotation_interval {
            self.current_index = (self.current_index + 1) % TIPS.len();
            self.last_rotate = Instant::now();
        }
    }

    /// Manually advance to the next tip.
    pub fn next_tip(&mut self) {
        if !self.enabled {
            return;
        }
        self.current_index = (self.current_index + 1) % TIPS.len();
        self.last_rotate = Instant::now();
    }

    /// Get the current tip text.
    pub fn current_tip(&self) -> &'static str {
        if !self.enabled {
            return "";
        }
        TIPS[self.current_index]
    }

    /// Render the tips banner as a 1-line widget.
    pub fn render(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        if !self.enabled {
            return;
        }
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};

        let tip = self.current_tip();
        let line = Line::from(vec![
            Span::styled(
                " 💡 ",
                Style::default()
                    .fg(super::theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(tip, Style::default().fg(super::theme::fg_dim())),
        ]);
        frame.render_widget(ratatui::widgets::Paragraph::new(line), area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tips_count() {
        assert!(
            TIPS.len() >= 40,
            "Expected at least 40 tips, got {}",
            TIPS.len()
        );
    }

    #[test]
    fn new_tip_banner_starts_at_zero() {
        let tips = TipsBanner::new(true, 30);
        assert_eq!(tips.current_index, 0);
        assert_eq!(tips.current_tip(), TIPS[0]);
    }

    #[test]
    fn next_tip_advances_index() {
        let mut tips = TipsBanner::new(true, 30);
        tips.next_tip();
        assert_eq!(tips.current_index, 1);
        assert_eq!(tips.current_tip(), TIPS[1]);
    }

    #[test]
    fn next_tip_wraps_around() {
        let mut tips = TipsBanner::new(true, 30);
        tips.current_index = TIPS.len() - 1;
        tips.next_tip();
        assert_eq!(tips.current_index, 0);
    }

    #[test]
    fn rotate_advances_after_interval() {
        let mut tips = TipsBanner::new(true, 0);
        // With 0-second interval, rotate should always advance
        tips.rotate();
        assert_eq!(tips.current_index, 1);
    }

    #[test]
    fn rotate_does_not_advance_before_interval() {
        let mut tips = TipsBanner::new(true, 60);
        // Just created, interval hasn't elapsed
        tips.rotate();
        assert_eq!(tips.current_index, 0);
    }

    #[test]
    fn disabled_tips_returns_empty() {
        let tips = TipsBanner::new(false, 30);
        assert_eq!(tips.current_tip(), "");
    }

    #[test]
    fn disabled_tips_no_advance() {
        let mut tips = TipsBanner::new(false, 30);
        tips.next_tip();
        assert_eq!(tips.current_index, 0);
    }

    #[test]
    fn tips_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for tip in TIPS {
            assert!(seen.insert(*tip), "Duplicate tip: {}", tip);
        }
    }
}
