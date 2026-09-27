//! Bottom status bar with key shortcuts, model, branch, and transient notices.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::display::state::AppState;
use crate::display::theme;

/// Render the bottom status line.
pub fn render_status_bar(frame: &mut Frame, state: &AppState, area: Rect) {
    let width = area.width as usize;
    if width < 10 || area.height == 0 {
        return;
    }

    // Model badge + current tool indicator (left side, before shortcuts).
    let mut left_spans = vec![];
    if width >= 40 {
        let model_display = if state.model.len() > 12 {
            format!("{:.12}…", state.model)
        } else {
            state.model.clone()
        };
        left_spans.push(Span::styled(
            format!(" {} ", model_display),
            Style::default()
                .fg(theme::fg_bright())
                .add_modifier(Modifier::BOLD),
        ));
        if let Some(idx) = state.current_tool_index {
            if let Some(card) = state.tool_cards.get(idx) {
                let glyph = card.status_glyph();
                let color = card.status_color();
                let tool_text = if card.summary.len() > 20 {
                    format!("{}: {:.20}…", card.tool_name, card.summary)
                } else {
                    format!("{}: {}", card.tool_name, card.summary)
                };
                left_spans.push(Span::styled(
                    format!(" │ {} {} ", glyph, tool_text),
                    Style::default().fg(color),
                ));
            }
        }
    }

    // Key hints come from `footer::contextual_hints` and are fitted by the
    // documented priority ladder, rather than being a fixed list chosen by
    // three hand-tuned width tiers. That way a hint appears only when the
    // action is available, and the footer degrades by dropping the least
    // important hint instead of hard-switching wording at an arbitrary width.
    // The old tiers also never mentioned the arrow keys.
    //
    // `LEFT_BUDGET` leaves room for the right-aligned status (branch, cost,
    // notice) so hints cannot push it off the bar.
    const LEFT_BUDGET: usize = 46;
    let hint_budget = width.saturating_sub(LEFT_BUDGET);
    let kept = super::footer::fit(&super::footer::contextual_hints(state), hint_budget);

    let mut shortcut_spans: Vec<Span> = Vec::with_capacity(kept.len() * 2);
    for (i, h) in kept.iter().enumerate() {
        if i > 0 {
            shortcut_spans.push(Span::styled("· ", Style::default().fg(theme::fg_subtle())));
        }
        shortcut_spans.push(Span::styled(
            h.key,
            Style::default()
                .fg(theme::fg_bright())
                .add_modifier(Modifier::BOLD),
        ));
        shortcut_spans.push(Span::styled(
            format!(" {} ", h.label),
            Style::default().fg(theme::fg_subtle()),
        ));
    }

    left_spans.extend(shortcut_spans);

    // Right-aligned status info
    let mut right_spans = vec![];

    if !state.branch_name.is_empty() && width >= 65 {
        right_spans.push(Span::styled(
            format!("branch {}   ", state.branch_name),
            Style::default().fg(theme::cyan()),
        ));
    }

    if state.cost > 0.0 && width >= 45 {
        right_spans.push(Span::styled(
            format!("${:.4}   ", state.cost),
            Style::default().fg(theme::fg_dim()),
        ));
    }

    if state.context_usage > 0.0 && width >= 55 {
        let pct = (state.context_usage * 100.0).round() as u32;
        let filled = (pct / 10).clamp(0, 10) as usize;
        let empty = 10 - filled;
        right_spans.push(Span::styled("ctx ", Style::default().fg(theme::fg_dim())));
        right_spans.push(Span::styled(
            "▓".repeat(filled),
            Style::default().fg(theme::thinking_green()),
        ));
        right_spans.push(Span::styled(
            "░".repeat(empty),
            Style::default().fg(theme::fg_dim()),
        ));
        right_spans.push(Span::styled(
            format!(" {}%   ", pct),
            Style::default().fg(theme::fg_dim()),
        ));
    }

    if state.input_state.has_queued() && width >= 50 {
        let q = state.input_state.queued.len();
        right_spans.push(Span::styled(
            format!("{} queued   ", q),
            Style::default().fg(theme::clay()),
        ));
    }

    // Permission mode indicator — styled badge with distinct color per mode
    let (badge_text, badge_color) = match state.permission_mode {
        crate::display::state::PermissionMode::Default => (" MANUAL ", theme::fg_subtle()),
        crate::display::state::PermissionMode::AcceptEdits => (" EDITS ", theme::success()),
        crate::display::state::PermissionMode::Plan => (" PLAN ", theme::thinking_green()),
        crate::display::state::PermissionMode::Auto => (" AUTO ", theme::thinking_green()),
        crate::display::state::PermissionMode::DontAsk => (" YOLO ", theme::error()),
        crate::display::state::PermissionMode::BypassPermissions => (" BYPASS ", theme::error()),
    };
    let badge_hovered = matches!(
        state.hover_target,
        crate::display::state::HoverTarget::StatusBarMode
    );
    let badge_style = if badge_hovered {
        Style::default()
            .fg(badge_color)
            .bg(crate::display::theme::bg_elevated())
            .add_modifier(ratatui::style::Modifier::BOLD)
    } else {
        Style::default()
            .fg(badge_color)
            .add_modifier(ratatui::style::Modifier::BOLD)
    };
    // The permission badge is the one status element whose *text* changes
    // rarely and whose meaning matters, so it gets the shimmer rather than the
    // hint strip. Following Codex: the phase is derived from elapsed time, so
    // two frames of the same text are in step rather than restarting.
    //
    // A mode like BYPASS is exactly the state a user wants to notice, and a
    // static badge at the far right of a wide bar is the least-looked-at pixel
    // on screen.
    let badge_label = badge_text.trim();
    // Animate only while the run is actually doing something.
    //
    // The shimmer used to run on every frame, unconditionally — so a chat
    // sitting idle with a static permission badge repainted at 30fps forever.
    // Two costs: it burns CPU and battery to animate a permission the user
    // can already see, and it means the screen never goes quiescent, which
    // makes "wait until the UI settles" untestable. The headless PTY suite
    // asserted on exactly that and failed on CI while passing on any machine
    // with `NO_COLOR` set — the same env-dependence the Rust suite had.
    //
    // Motion should mean "something is happening". A permission badge at rest
    // is a fact, not an event.
    let animating = !matches!(state.run_state, crate::display::state::RunState::Idle);
    let shimmered = if animating {
        crate::display::motion::summary_shimmer_with(
            badge_label,
            state.tick as f64 / 30.0, // 30fps idle loop
            state.config.ui.reduced_motion || std::env::var_os("NIKI_REDUCED_MOTION").is_some(),
            crate::display::theme::supports_truecolor(),
        )
    } else {
        vec![ratatui::text::Span::styled(
            badge_label,
            Style::default()
                .fg(badge_color)
                .add_modifier(ratatui::style::Modifier::BOLD),
        )]
    };
    // The shimmer spans carry their own foreground, so a hovered badge would
    // otherwise lose its background and the hover would vanish. Re-apply the
    // badge background on top of each shimmer span.
    let spans = if badge_hovered {
        shimmered
            .into_iter()
            .map(|sp| {
                Span::styled(
                    sp.content,
                    sp.style.bg(crate::display::theme::bg_elevated()),
                )
            })
            .collect::<Vec<_>>()
    } else {
        shimmered
    };
    right_spans.push(Span::styled(" ", badge_style));
    right_spans.extend(spans);
    right_spans.push(Span::styled(" ", badge_style));

    if let Some(notice) = &state.notice {
        // Slide in over the first 150ms of life; static when reduced.
        let age_ms = notice.since.elapsed().as_millis() as u64;
        let slide = crate::display::motion::slide_prefix(
            age_ms,
            150,
            3,
            crate::display::motion::reduced(state.config.ui.reduced_motion),
        );
        right_spans.push(Span::styled(
            format!("· {}{} ", slide, notice.msg),
            Style::default().fg(theme::clay()),
        ));
    }

    let left_len: usize = left_spans.iter().map(|s| s.content.chars().count()).sum();

    // Build the final line greedily so it can never overflow the frame:
    // left shortcuts first, then as many right-aligned extras as fit, then a
    // trailing pad. This guarantees `total <= width` in every branch.
    let mut spans = left_spans;
    let mut used = left_len;
    let mut kept_right = Vec::new();
    for s in &right_spans {
        let n = s.content.chars().count();
        if used + n <= width {
            kept_right.push(s.clone());
            used += n;
        } else {
            break;
        }
    }
    if width > used {
        spans.push(Span::styled(" ".repeat(width - used), Style::default()));
    }
    spans.extend(kept_right);

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Hit-test a mouse position against status bar regions, returning the hover target.
pub fn hover_test(
    mouse_col: u16,
    area: Rect,
    state: &AppState,
) -> crate::display::state::HoverTarget {
    use crate::display::state::HoverTarget;
    let width = area.width as usize;
    let col = mouse_col.saturating_sub(area.x) as usize;

    // The status bar is a single line. We need to figure out where each region is.
    // Left side: keyboard shortcuts (tab, ctrl-p, esc)
    // Right side: branch, cost, ctx, queued, mode badge

    // Mode badge is always at the far right
    let (badge_text, _) = match state.permission_mode {
        crate::display::state::PermissionMode::Default => {
            (" MANUAL ", crate::display::state::PermissionMode::Default)
        }
        crate::display::state::PermissionMode::AcceptEdits => (
            " EDITS ",
            crate::display::state::PermissionMode::AcceptEdits,
        ),
        crate::display::state::PermissionMode::Plan => {
            (" PLAN ", crate::display::state::PermissionMode::Plan)
        }
        crate::display::state::PermissionMode::Auto => {
            (" AUTO ", crate::display::state::PermissionMode::Auto)
        }
        crate::display::state::PermissionMode::DontAsk => {
            (" YOLO ", crate::display::state::PermissionMode::DontAsk)
        }
        crate::display::state::PermissionMode::BypassPermissions => (
            " BYPASS ",
            crate::display::state::PermissionMode::BypassPermissions,
        ),
    };
    let badge_len = badge_text.len();
    if col >= width.saturating_sub(badge_len + 1) {
        return HoverTarget::StatusBarMode;
    }

    // Cost region (before badge)
    if state.cost > 0.0 && width >= 45 {
        let cost_str = format!("${:.4}   ", state.cost);
        let cost_end = width.saturating_sub(badge_len + 1);
        let cost_start = cost_end.saturating_sub(cost_str.len());
        if col >= cost_start && col < cost_end {
            return HoverTarget::StatusBarCost;
        }
    }

    // Context bar region
    if state.context_usage > 0.0 && width >= 55 {
        let pct = (state.context_usage * 100.0).round() as u32;
        let filled = (pct / 10).clamp(0, 10) as usize;
        let empty = 10 - filled;
        let ctx_str = format!(
            "ctx {}{} {}%   ",
            "▓".repeat(filled),
            "░".repeat(empty),
            pct
        );
        let cost_str = if state.cost > 0.0 && width >= 45 {
            format!("${:.4}   ", state.cost)
        } else {
            String::new()
        };
        let ctx_end = width
            .saturating_sub(badge_len + 1)
            .saturating_sub(cost_str.len());
        let ctx_start = ctx_end.saturating_sub(ctx_str.len());
        if col >= ctx_start && col < ctx_end {
            return HoverTarget::StatusBarCtx;
        }
    }

    // Branch region (leftmost of right side)
    if !state.branch_name.is_empty() && width >= 65 {
        let branch_str = format!("branch {}   ", state.branch_name);
        let badge_len_total = badge_len + 1;
        let cost_len = if state.cost > 0.0 && width >= 45 {
            format!("${:.4}   ", state.cost).len()
        } else {
            0
        };
        let ctx_len = if state.context_usage > 0.0 && width >= 55 {
            let pct = (state.context_usage * 100.0).round() as u32;
            let filled = (pct / 10).clamp(0, 10) as usize;
            let empty = 10 - filled;
            format!(
                "ctx {}{} {}%   ",
                "▓".repeat(filled),
                "░".repeat(empty),
                pct
            )
            .len()
        } else {
            0
        };
        let branch_end = width.saturating_sub(badge_len_total + cost_len + ctx_len);
        let branch_start = branch_end.saturating_sub(branch_str.len());
        if col >= branch_start && col < branch_end {
            return HoverTarget::StatusBarBranch;
        }
    }

    HoverTarget::None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NikiConfig;

    #[test]
    fn status_bar_line_width() {
        let config = NikiConfig::default();
        let state = AppState::new("test".to_string(), config, ".".into());
        assert_eq!(state.model, "claude-sonnet-4-20250514");
    }
}
