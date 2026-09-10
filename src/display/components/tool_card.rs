//! Tool execution card — renders a single tool call (Bash, Read, Edit, Write)
//! as a collapsible card with status, timing, and output preview.
//!
//! Matches Claude Code / Kimi Code visual treatment:
//! - Status glyph (unified grammar: ○ pending / ⠋ running / ✓ done / ✗ failed) + tool name + summary
//! - Expanded body shows first N lines of output with "N more lines" disclosure
//! - Footer shows elapsed/total duration and token count

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::display::theme;

/// Status of a tool execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolStatus {
    /// Waiting in queue.
    Pending,
    /// Currently executing — `elapsed_ms` tracks runtime.
    Running { elapsed_ms: u64 },
    /// Completed successfully — `duration_ms` is the wall-clock time.
    Success { duration_ms: u64 },
    /// Failed — carries the error message.
    Failed { error: String },
}

/// A single tool execution (Bash, Read, Edit, Write, etc.).
#[derive(Debug, Clone)]
pub struct ToolCard {
    /// Tool name: "Bash", "Read", "Edit", "Write", "Glob", "Grep"
    pub tool_name: String,
    /// Current status.
    pub status: ToolStatus,
    /// One-line summary: command string, file path, or diff preview.
    pub summary: String,
    /// Full output (stdout/stderr/diff), available once complete.
    pub output: Option<String>,
    /// Whether the card body is expanded.
    pub expanded: bool,
}

impl ToolCard {
    /// Create a new pending tool card.
    pub fn new(tool_name: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            tool_name: tool_name.into(),
            status: ToolStatus::Pending,
            summary: summary.into(),
            output: None,
            expanded: false,
        }
    }

    /// Mark the card as running (dispatched to sandbox).
    pub fn set_running(&mut self) {
        self.status = ToolStatus::Running { elapsed_ms: 0 };
    }

    /// Mark the card as succeeded with output.
    pub fn set_success(&mut self, output: Option<String>, duration_ms: u64) {
        self.status = ToolStatus::Success { duration_ms };
        self.output = output;
        self.expanded = true; // Auto-expand on success so output is visible
    }

    /// Mark the card as failed with an error message.
    pub fn set_failed(&mut self, error: impl Into<String>) {
        self.status = ToolStatus::Failed {
            error: error.into(),
        };
        self.expanded = true; // Auto-expand to show error
    }

    /// Toggle expanded/collapsed state.
    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }

    /// Status glyph for the current state (unified grammar — see `super::status`).
    pub fn status_glyph(&self) -> &'static str {
        super::status::glyph(super::status::UnifiedStatus::from(&self.status))
    }

    /// Color for the status glyph (unified grammar — see `super::status`).
    pub fn status_color(&self) -> Color {
        super::status::color(super::status::UnifiedStatus::from(&self.status))
    }

    /// Short timing string for the footer.
    pub fn timing(&self) -> Option<String> {
        match self.status {
            ToolStatus::Running { elapsed_ms } => Some(format!("{}ms", elapsed_ms)),
            ToolStatus::Success { duration_ms } => Some(format!("{}ms", duration_ms)),
            _ => None,
        }
    }
}

/// Render a tool card into a list of `Line`s for the given area width.
///
/// Returns the lines consumed (card may span multiple rows). The caller is
/// responsible for clipping/scrolling.
pub fn render_tool_card(card: &ToolCard, area_width: u16) -> Vec<Line<'static>> {
    let width = area_width as usize;
    if width < 10 {
        return vec![Line::from("...")];
    }

    let mut lines: Vec<Line<'static>> = Vec::new();

    // ── Header row: "<glyph> Bash cargo test --verbose" ───────────────
    let glyph = card.status_glyph();
    let glyph_color = card.status_color();

    let summary = if card.summary.len() > width.saturating_sub(8) {
        theme::truncate_str_ellipsis(&card.summary, width.saturating_sub(8))
    } else {
        card.summary.clone()
    };

    let header = Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(
            glyph,
            Style::default()
                .fg(glyph_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ", Style::default()),
        Span::styled(
            card.tool_name.clone(),
            Style::default()
                .fg(theme::fg_bright())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", summary),
            Style::default().fg(theme::fg_dim()),
        ),
    ]);
    lines.push(header);

    // ── Body (expanded only) ─────────────────────────────────────────
    if card.expanded {
        if let Some(output) = &card.output {
            let output_lines: Vec<&str> = output.lines().collect();
            let preview_len = output_lines.len().min(5);
            for line in &output_lines[..preview_len] {
                let trimmed = if line.len() > width.saturating_sub(6) {
                    format!(
                        "    {}",
                        theme::truncate_str_ellipsis(line, width.saturating_sub(6))
                    )
                } else {
                    format!("    {}", line)
                };
                lines.push(Line::from(Span::styled(
                    trimmed,
                    Style::default().fg(theme::fg_dim()),
                )));
            }
            if output_lines.len() > 5 {
                lines.push(Line::from(Span::styled(
                    format!(
                        "    +{} more lines (Enter to expand all)",
                        output_lines.len() - 5
                    ),
                    Style::default()
                        .fg(theme::clay())
                        .add_modifier(Modifier::ITALIC),
                )));
            }
        }

        // ── Footer: timing ────────────────────────────────────────────
        if let Some(timing) = card.timing() {
            lines.push(Line::from(Span::styled(
                format!("  ─ {}", timing),
                Style::default().fg(theme::fg_subtle()),
            )));
        }
    }

    lines
}

/// Estimate the height a card will occupy at the given width.
pub fn tool_card_height(card: &ToolCard, _area_width: u16) -> usize {
    let mut h = 1; // header
    if card.expanded {
        if let Some(output) = &card.output {
            let line_count = output.lines().count();
            h += line_count.min(5);
            if line_count > 5 {
                h += 1; // "N more lines" hint
            }
        }
        if card.timing().is_some() {
            h += 1;
        }
    }
    h
}

/// Hit-test a mouse row against a tool card at the given offset.
/// Returns true if the click landed on this card.
pub fn hit_test_card(card: &ToolCard, row_offset: u16, area_width: u16) -> bool {
    let height = tool_card_card_height(card, area_width);
    (row_offset as usize) < height
}

// Alias for consistency with naming convention.
fn tool_card_card_height(card: &ToolCard, area_width: u16) -> usize {
    tool_card_height(card, area_width)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_card_is_pending() {
        let card = ToolCard::new("Bash", "cargo test");
        assert_eq!(card.status, ToolStatus::Pending);
        assert_eq!(card.status_glyph(), "○");
        assert!(!card.expanded);
    }

    #[test]
    fn running_state() {
        let mut card = ToolCard::new("Bash", "cargo build");
        card.set_running();
        assert!(matches!(card.status, ToolStatus::Running { elapsed_ms: 0 }));
        assert_eq!(card.status_glyph(), "⠋");
    }

    #[test]
    fn success_state() {
        let mut card = ToolCard::new("Bash", "echo hello");
        card.set_success(Some("hello\n".to_string()), 150);
        assert!(matches!(
            card.status,
            ToolStatus::Success { duration_ms: 150 }
        ));
        assert_eq!(card.status_glyph(), "✓");
        assert!(card.expanded); // auto-expanded on success
        assert_eq!(card.timing(), Some("150ms".to_string()));
    }

    #[test]
    fn failed_state() {
        let mut card = ToolCard::new("Bash", "rm -rf /");
        card.set_failed("Permission denied");
        assert!(matches!(card.status, ToolStatus::Failed { error: _ }));
        assert_eq!(card.status_glyph(), "✗");
        assert!(card.expanded); // auto-expanded on failure
    }

    #[test]
    fn toggle_expanded() {
        let mut card = ToolCard::new("Read", "src/main.rs");
        assert!(!card.expanded);
        card.toggle();
        assert!(card.expanded);
        card.toggle();
        assert!(!card.expanded);
    }

    #[test]
    fn render_header_only_when_collapsed() {
        let card = ToolCard::new("Bash", "cargo test");
        let lines = render_tool_card(&card, 60);
        assert_eq!(lines.len(), 1);
        let header = lines[0].to_string();
        assert!(header.contains("Bash"));
        assert!(header.contains("cargo test"));
    }

    #[test]
    fn render_full_when_expanded_with_output() {
        let mut card = ToolCard::new("Bash", "ls -la");
        card.set_success(Some("file1.rs\nfile2.rs\n".to_string()), 50);
        let lines = render_tool_card(&card, 80);
        // header + 2 output lines + timing
        assert!(lines.len() >= 4);
        assert!(lines[1].to_string().contains("file1.rs"));
    }

    #[test]
    fn render_more_lines_hint() {
        let mut card = ToolCard::new("Bash", "cat big_file.rs");
        let big_output = (0..20)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        card.set_success(Some(big_output), 100);
        let lines = render_tool_card(&card, 80);
        let joined = lines
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("+15 more lines"));
    }

    #[test]
    fn render_truncates_long_summary() {
        let long_cmd = "a".repeat(200);
        let card = ToolCard::new("Bash", long_cmd);
        let lines = render_tool_card(&card, 60);
        let header = lines[0].to_string();
        assert!(header.len() < 200);
        assert!(header.contains("..."));
    }

    #[test]
    fn height_estimate() {
        let card = ToolCard::new("Bash", "echo hi");
        assert_eq!(tool_card_height(&card, 80), 1);

        let mut card = ToolCard::new("Bash", "echo hi");
        card.expanded = true;
        card.output = Some("line1\nline2\nline3".to_string());
        card.status = ToolStatus::Success { duration_ms: 10 };
        assert_eq!(tool_card_height(&card, 80), 1 + 3 + 1); // header + 3 lines + timing
    }

    #[test]
    fn hit_test_card_within_bounds() {
        let card = ToolCard::new("Bash", "test");
        assert!(hit_test_card(&card, 0, 80));
    }

    #[test]
    fn hit_test_card_outside_bounds() {
        let card = ToolCard::new("Bash", "test");
        assert!(!hit_test_card(&card, 5, 80));
    }

    #[test]
    fn render_unicode_summary_and_output_no_panic() {
        // TUI-020: byte slicing here used to panic on multibyte text.
        let mut card = ToolCard::new("Bash", "déploie l’API 日🎉".repeat(4));
        card.set_success(
            Some("résultat héllo wörld output line\nsecond lïne 日".to_string()),
            12,
        );
        let lines = render_tool_card(&card, 30);
        assert!(!lines.is_empty());
        let height = tool_card_height(&card, 30);
        assert!(height >= lines.len() - 1);
    }
}
