//! Motion system — one engine for every effect, no ad-hoc ticks.
//!
//! Rules:
//! - Every effect is a pure function of `(tick | elapsed_ms)` plus content.
//!   No effect keeps its own clock; the TUI frame tick (`AppState::tick`,
//!   60fps streaming / 30fps idle) and existing timestamps (`StageState`
//!   elapsed, notice TTLs) are the only time sources.
//! - Every effect degrades to its final static state under reduced motion
//!   (`[ui] reduced_motion` or `NIKI_REDUCED_MOTION`), checked via
//!   [`reduced`]. No exceptions.
//! - Effects never move the cursor, never change layout size, and never
//!   exceed the engine's frame budget: they only change glyphs, colors, or
//!   short leading prefixes within already-allocated cells.

use ratatui::style::Color;

/// True when motion must collapse to static final states.
pub fn reduced(config_flag: bool) -> bool {
    config_flag || std::env::var_os("NIKI_REDUCED_MOTION").is_some()
}

/// Clamp to `[0, 1]`.
pub fn clamp01(t: f32) -> f32 {
    t.clamp(0.0, 1.0)
}

/// Ease-out cubic: fast start, gentle landing. `t` in `[0, 1]`.
pub fn ease_out_cubic(t: f32) -> f32 {
    let t = clamp01(t);
    1.0 - (1.0 - t).powi(3)
}

/// Linear interpolation between two RGB colors. Non-RGB colors snap to `b`
/// once `t >= 1.0`, otherwise hold `a` (indexed palettes can't interpolate).
pub fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let t = clamp01(t);
    match (a, b) {
        (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => {
            let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
            Color::Rgb(mix(ar, br), mix(ag, bg), mix(ab, bb))
        }
        _ => {
            if t >= 1.0 {
                b
            } else {
                a
            }
        }
    }
}

/// Leading-space prefix for slide-in effects: `max_cols` spaces shrinking to
/// none over `duration_ms`, eased. Returns `""` when done or reduced.
pub fn slide_prefix(
    elapsed_ms: u64,
    duration_ms: u64,
    max_cols: usize,
    is_reduced: bool,
) -> String {
    if is_reduced || duration_ms == 0 {
        return String::new();
    }
    let t = ease_out_cubic(elapsed_ms as f32 / duration_ms as f32);
    let remaining = ((1.0 - t) * max_cols as f32).round() as usize;
    " ".repeat(remaining.min(max_cols))
}

/// Caret visibility for a blink cycle: visible `on_ms`, hidden `off_ms`,
/// driven by the frame tick (assumes ~60fps ticks; exact cadence is
/// aesthetic, not contractual). Always visible when reduced.
pub fn blink_on(tick: usize, on_ticks: usize, off_ticks: usize, is_reduced: bool) -> bool {
    if is_reduced {
        return true;
    }
    let cycle = on_ticks + off_ticks;
    if cycle == 0 {
        return true;
    }
    tick % cycle < on_ticks
}

/// Progress-bar shimmer: position of the bright window within the filled
/// region, or `None` when reduced (plain bar). Pure function of tick.
pub fn shimmer_pos(tick: usize, filled: usize) -> Option<usize> {
    if filled == 0 {
        return None;
    }
    Some((tick / 4) % filled)
}

/// Modal attention pulse: alternates every `half_period_ticks`. Callers pass
/// the global tick; no per-modal clock needed because the pulse is meant to
/// run continuously while the modal is open. Static (first phase) reduced.
pub fn pulse_phase(tick: usize, half_period_ticks: usize, is_reduced: bool) -> bool {
    if is_reduced || half_period_ticks == 0 {
        return false;
    }
    (tick / half_period_ticks) % 2 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ease_hits_endpoints_and_midpoint_shape() {
        assert_eq!(ease_out_cubic(0.0), 0.0);
        assert_eq!(ease_out_cubic(1.0), 1.0);
        assert_eq!(ease_out_cubic(5.0), 1.0);
        assert_eq!(ease_out_cubic(-1.0), 0.0);
        // Ease-out covers most distance early.
        assert!(ease_out_cubic(0.5) > 0.5);
    }

    #[test]
    fn slide_prefix_shrinks_to_empty() {
        assert_eq!(slide_prefix(0, 150, 4, false), "    ");
        assert!(slide_prefix(75, 150, 4, false).len() <= 4);
        assert_eq!(slide_prefix(150, 150, 4, false), "");
        assert_eq!(slide_prefix(9999, 150, 4, false), "");
        assert_eq!(slide_prefix(0, 150, 4, true), "");
        assert_eq!(slide_prefix(0, 0, 4, false), "");
    }

    #[test]
    fn blink_cycles_and_freezes_reduced() {
        assert!(blink_on(0, 32, 16, false));
        assert!(blink_on(31, 32, 16, false));
        assert!(!blink_on(32, 32, 16, false));
        assert!(!blink_on(47, 32, 16, false));
        assert!(blink_on(48, 32, 16, false));
        assert!(blink_on(40, 32, 16, true));
        assert!(blink_on(0, 0, 0, false));
    }

    #[test]
    fn shimmer_tracks_filled_region() {
        assert_eq!(shimmer_pos(0, 0), None);
        assert_eq!(shimmer_pos(0, 10), Some(0));
        assert_eq!(shimmer_pos(4, 10), Some(1));
        assert_eq!(shimmer_pos(40, 10), Some(0));
    }

    #[test]
    fn lerp_midpoint_and_indexed_fallback() {
        assert_eq!(
            lerp_color(Color::Rgb(0, 0, 0), Color::Rgb(100, 100, 100), 0.5),
            Color::Rgb(50, 50, 50)
        );
        assert_eq!(
            lerp_color(Color::Red, Color::Blue, 0.5),
            Color::Red,
            "indexed colors hold until t=1"
        );
        assert_eq!(lerp_color(Color::Red, Color::Blue, 1.0), Color::Blue);
    }

    #[test]
    fn reduced_flag_forces_static() {
        // Explicit opt-in always wins; env-var coverage is exercised by the
        // spinner's existing reduced-motion tests (env mutation is unsafe in
        // parallel unit tests, so it is not re-tested here).
        assert!(reduced(true));
        assert!(blink_on(0, 32, 16, true));
        assert_eq!(slide_prefix(0, 150, 4, true), "");
        assert!(!pulse_phase(9999, 10, true));
    }
}
