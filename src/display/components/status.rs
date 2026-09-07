//! Unified status grammar — one glyph + one color per state, everywhere.
//!
//! Before this module, each surface invented its own: Fleet used `●` for both
//! Running *and* Paused (indistinguishable), tool cards used `●` for Running
//! while chat used `⠋`, and raw ANSI colors bypassed the theme in six files.
//! Every status-bearing view must map through [`UnifiedStatus`] so a glance
//! means the same thing on every page.

use ratatui::style::Color;

use crate::display::theme;

/// One status vocabulary for stages, missions, tools, and verdicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnifiedStatus {
    /// Queued / pending / created — waiting, dim hollow circle.
    Pending,
    /// Actively working — braille spinner cell (animated by the caller).
    Running,
    /// Paused by user or policy — double bar, amber. Never shares Running's glyph.
    Paused,
    /// Finished successfully — check, success green.
    Done,
    /// Cancelled by user — slashed circle, dim.
    Cancelled,
    /// Failed — cross, error coral.
    Failed,
    /// Revision loop — recycle arrows, accent clay.
    Revision,
}

/// Glyph for a status. Running returns the braille base cell; callers doing
/// animation substitute `spinner_glyph(tick)` for it (same column width).
pub fn glyph(status: UnifiedStatus) -> &'static str {
    match status {
        UnifiedStatus::Pending => "○",
        UnifiedStatus::Running => "⠋",
        UnifiedStatus::Paused => "⏸",
        UnifiedStatus::Done => "✓",
        UnifiedStatus::Cancelled => "⊘",
        UnifiedStatus::Failed => "✗",
        UnifiedStatus::Revision => "⟳",
    }
}

/// Theme color for a status. All tokens — no raw ANSI anywhere downstream.
pub fn color(status: UnifiedStatus) -> Color {
    match status {
        UnifiedStatus::Pending => theme::fg_dim(),
        UnifiedStatus::Running => theme::accent(),
        UnifiedStatus::Paused => theme::warning(),
        UnifiedStatus::Done => theme::success(),
        UnifiedStatus::Cancelled => theme::fg_dim(),
        UnifiedStatus::Failed => theme::error(),
        UnifiedStatus::Revision => theme::accent(),
    }
}

/// Screen-reader / log text for a status.
pub fn text(status: UnifiedStatus) -> &'static str {
    match status {
        UnifiedStatus::Pending => "pending",
        UnifiedStatus::Running => "running",
        UnifiedStatus::Paused => "paused",
        UnifiedStatus::Done => "done",
        UnifiedStatus::Cancelled => "cancelled",
        UnifiedStatus::Failed => "failed",
        UnifiedStatus::Revision => "revision",
    }
}

/// Styled span ready to render: `glyph + color`.
pub fn span(status: UnifiedStatus) -> ratatui::text::Span<'static> {
    ratatui::text::Span::styled(
        glyph(status),
        ratatui::style::Style::default().fg(color(status)),
    )
}

impl From<crate::display::agent_stream::StageStatus> for UnifiedStatus {
    fn from(s: crate::display::agent_stream::StageStatus) -> Self {
        use crate::display::agent_stream::StageStatus as S;
        match s {
            S::Pending => UnifiedStatus::Pending,
            S::Running => UnifiedStatus::Running,
            S::Done => UnifiedStatus::Done,
            S::Failed => UnifiedStatus::Failed,
            S::Revision => UnifiedStatus::Revision,
        }
    }
}

impl From<crate::display::state::StageStatus> for UnifiedStatus {
    fn from(s: crate::display::state::StageStatus) -> Self {
        use crate::display::state::StageStatus as S;
        match s {
            S::Running => UnifiedStatus::Running,
            S::Done => UnifiedStatus::Done,
            S::Failed => UnifiedStatus::Failed,
            S::Queued => UnifiedStatus::Pending,
        }
    }
}

impl From<crate::mission::MissionStatus> for UnifiedStatus {
    fn from(s: crate::mission::MissionStatus) -> Self {
        use crate::mission::MissionStatus as M;
        match s {
            M::Created => UnifiedStatus::Pending,
            M::Running => UnifiedStatus::Running,
            M::Paused => UnifiedStatus::Paused,
            M::Completed => UnifiedStatus::Done,
            M::Failed => UnifiedStatus::Failed,
            M::Cancelled => UnifiedStatus::Cancelled,
        }
    }
}

impl From<&crate::display::components::tool_card::ToolStatus> for UnifiedStatus {
    fn from(s: &crate::display::components::tool_card::ToolStatus) -> Self {
        use crate::display::components::tool_card::ToolStatus as T;
        match s {
            T::Pending => UnifiedStatus::Pending,
            T::Running { .. } => UnifiedStatus::Running,
            T::Success { .. } => UnifiedStatus::Done,
            T::Failed { .. } => UnifiedStatus::Failed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_and_paused_differ() {
        // The fleet bug: Running and Paused shared `●`. Never again.
        assert_ne!(glyph(UnifiedStatus::Running), glyph(UnifiedStatus::Paused));
        assert_ne!(color(UnifiedStatus::Running), color(UnifiedStatus::Paused));
    }

    #[test]
    fn every_variant_has_distinct_glyph() {
        let glyphs = [
            UnifiedStatus::Pending,
            UnifiedStatus::Running,
            UnifiedStatus::Paused,
            UnifiedStatus::Done,
            UnifiedStatus::Cancelled,
            UnifiedStatus::Failed,
            UnifiedStatus::Revision,
        ]
        .map(glyph);
        let mut seen = std::collections::HashSet::new();
        for g in glyphs {
            assert!(seen.insert(g), "duplicate glyph: {g}");
        }
    }

    #[test]
    fn mission_mapping_covers_all() {
        use crate::mission::MissionStatus as M;
        assert_eq!(UnifiedStatus::from(M::Paused), UnifiedStatus::Paused);
        assert_eq!(UnifiedStatus::from(M::Cancelled), UnifiedStatus::Cancelled);
        assert_eq!(UnifiedStatus::from(M::Created), UnifiedStatus::Pending);
    }
}
