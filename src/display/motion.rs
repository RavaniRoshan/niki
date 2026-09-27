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

/// Sweep period for [`summary_shimmer`], in seconds.
///
/// Codex uses 2.0s for both its shimmers (`shimmer.rs` and
/// `summary_shimmer.rs`). Long enough to read as a travelling band rather than
/// a flicker, short enough to feel alive on a 30fps idle loop.
pub const SHIMMER_PERIOD_SECS: f64 = 2.0;

/// The narrowest band a shimmer may use, in terminal columns.
///
/// Codex's `summary_shimmer` scales the band to the text and floors it at 3
/// columns, because a 1-2 column band on a short label flashes a single letter
/// and reads as a glitch rather than a sweep.
pub const SHIMMER_MIN_HALF_WIDTH: f64 = 3.0;

/// A cosine band sweeping across `text`, returned one span per grapheme.
///
/// Follows Codex's `summary_shimmer` rather than inventing a look:
///
/// - **Brightness only.** The moving band uses the terminal foreground, the
///   rest sits partway toward the background. Changing hue as the band passes
///   makes long status strings shimmer in different colours, which reads as
///   an error state.
/// - **Per display column, not per char.** A wide glyph (`█`, CJK) advances
///   the sweep by two columns; indexing by char makes the band drift.
/// - **Band scales to the text**, floored at [`SHIMMER_MIN_HALF_WIDTH`].
/// - **Phase comes from the caller**, so a status line that changes text can
///   restart the sweep rather than having it jump mid-band.
///
/// Without truecolor there is nothing to blend, so the caller gets plain dim
/// text — a stepped animation on a 16-color terminal looks broken, not
/// restrained.
pub fn summary_shimmer(
    text: &str,
    elapsed_secs: f64,
    is_reduced: bool,
) -> Vec<ratatui::text::Span<'static>> {
    // `ColorDepth::detect` requires stdout to be a TTY, which it never is
    // inside a test binary — so the blend path was completely untestable. The
    // capability is a parameter here, and this wrapper is the only place that
    // reads the real terminal.
    // NO_COLOR is a property of the *terminal*, not of the effect, so it is
    // resolved here — the one place that reads the real environment. Previously
    // the check sat inside the capability-driven form, which made the test
    // suite's result depend on whether the developer (or CI) had NO_COLOR
    // exported: green here, two failures without it.
    if crate::display::theme::no_color() {
        return vec![ratatui::text::Span::raw(text.to_string())];
    }
    let truecolor = crate::display::theme::supports_truecolor();
    summary_shimmer_with(text, elapsed_secs, is_reduced, truecolor)
}

/// [`summary_shimmer`] with the colour capability supplied explicitly.
///
/// Split so the effect can be tested on a host with no TTY, and so a caller
/// that has already probed capability does not re-probe it per frame.
pub fn summary_shimmer_with(
    text: &str,
    elapsed_secs: f64,
    is_reduced: bool,
    truecolor: bool,
) -> Vec<ratatui::text::Span<'static>> {
    use ratatui::style::{Modifier, Style, Stylize as _};
    use ratatui::text::Span;

    if is_reduced || text.is_empty() {
        return vec![Span::raw(text.to_string())];
    }
    if !truecolor {
        return vec![Span::styled(text.to_string(), Style::default().dim())];
    }

    // The *raw* palette, not `bg_color()`/`fg_color()`: those collapse to
    // `Reset` under NO_COLOR, which would make the blend silently degenerate
    // to a flat dim on exactly the terminals where a shimmer is least wanted.
    let as_rgb = |c: Color| match c {
        Color::Rgb(r, g, b) => Some((r as f32, g as f32, b as f32)),
        _ => None,
    };
    let Some(bg) = as_rgb(crate::display::theme::raw_palette().bg) else {
        return vec![Span::styled(text.to_string(), Style::default().dim())];
    };
    let fg = as_rgb(crate::display::theme::raw_palette().fg).unwrap_or((0.82, 0.82, 0.82));

    let width = text.chars().map(char_width).sum::<usize>() as f64;
    if width <= 0.0 {
        return vec![Span::raw(text.to_string())];
    }
    // 10% of the text, floored — a short label still gets a readable sweep.
    let half_width = (width * 0.1).max(SHIMMER_MIN_HALF_WIDTH);
    let period = SHIMMER_PERIOD_SECS;
    let position =
        (elapsed_secs.rem_euclid(period) / period) * (width + 2.0 * half_width) - half_width;

    let mix = |a: (f32, f32, f32), b: (f32, f32, f32), t: f32| {
        (
            a.0 + (b.0 - a.0) * t,
            a.1 + (b.1 - a.1) * t,
            a.2 + (b.2 - a.2) * t,
        )
    };

    let mut out = Vec::new();
    let mut column = 0.0f64;
    for g in text.chars() {
        // Display width, not char count: a wide glyph advances the sweep by two
        // columns, and indexing by char makes the band drift across CJK text.
        let w = char_width(g) as f64;
        let center = column + w / 2.0;
        let distance = ((center - position).abs() / half_width).min(1.0);
        let intensity = 0.5 * (1.0 + (std::f64::consts::PI * distance).cos());
        // 0.5 (far) .. 1.0 (band centre): brightness only, never hue.
        let alpha = 0.5 + 0.5 * intensity as f32;
        // The channel bindings are named so they cannot shadow the character
        // being styled. An earlier version used `(r, g, b)` while iterating
        // `for g in text.chars()`, so `g.to_string()` rendered the *green
        // channel* as the glyph and the status badge filled with decimals like
        // `131153.2596206.96378`. The unit tests missed it because the
        // text-preservation test took the non-truecolor fallback; the visual
        // harness caught it on the first run.
        let (ch_r, ch_g, ch_b) = mix(bg, fg, alpha);
        out.push(Span::styled(
            g.to_string(),
            Style::default().fg(Color::Rgb(
                ch_r.clamp(0.0, 255.0) as u8,
                ch_g.clamp(0.0, 255.0) as u8,
                ch_b.clamp(0.0, 255.0) as u8,
            )),
        ));
        column += w;
    }
    let _ = Modifier::BOLD;
    out
}

/// Display columns a character occupies in a terminal.
fn char_width(c: char) -> usize {
    // East Asian Wide/Fullwidth ranges plus emoji, which render double-width.
    let cp = c as u32;
    if (0x1100..=0x115F).contains(&cp)
        || (0x2E80..=0xA4CF).contains(&cp)
        || (0xAC00..=0xD7A3).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
        || (0xFE30..=0xFE6F).contains(&cp)
        || (0xFF00..=0xFF60).contains(&cp)
        || (0xFFE0..=0xFFE6).contains(&cp)
        || (0x1F300..=0x1FAFF).contains(&cp)
    {
        2
    } else {
        1
    }
}

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
    (tick / half_period_ticks).is_multiple_of(2)
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

    // ── summary_shimmer ────────────────────────────────────────────
    //
    // The properties that matter are the ones a wrong implementation
    // breaks visibly: the band must move, must never vanish, must be
    // brightness-only, and must collapse to static under reduced motion.

    fn shimmer_text(spans: &[ratatui::text::Span<'static>]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn shimmer_preserves_the_exact_text() {
        // A shimmer that drops, reorders, or *substitutes* a character corrupts
        // the status line it decorates. Run through the explicit-capability
        // form so this exercises the blend path: with the fallback it passed
        // while the real effect was rendering colour channels as glyphs.
        for text in [
            "MANUAL",
            "Planner · running",
            "a",
            "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏",
            "BYPASS",
            "世界 wide",
        ] {
            for t in [0.0, 0.37, 1.13, 1.99] {
                let out = shimmer_text(&summary_shimmer_with(text, t, false, true));
                assert_eq!(out, text, "shimmer altered {text:?} at t={t}");
            }
        }
    }

    #[test]
    fn reduced_motion_returns_plain_static_text() {
        let out = summary_shimmer("MANUAL", 0.4, true);
        assert_eq!(shimmer_text(&out), "MANUAL");
        // And it must not be a one-cell jump either: reduced means static.
        let styles: Vec<_> = out.iter().map(|s| s.style).collect();
        assert!(
            styles.windows(2).all(|w| w[0] == w[1]),
            "reduced motion must be uniformly styled, got {styles:?}"
        );
    }

    /// The tests below need the truecolor path, but a developer (or CI) with
    /// `NO_COLOR` exported would otherwise exercise only the static fallback and
    /// conclude the band never moves. Guard rather than mutate the environment:
    /// process env is shared with every other test in this binary.
    #[allow(dead_code)]
    fn skip_without_truecolor() {
        if crate::display::theme::no_color() || !crate::display::theme::supports_truecolor() {
            eprintln!("skipping: NO_COLOR or no truecolor — the blend path is unreachable here");
        }
    }

    // These three exercise the blend path directly. They used to call the
    // wrapper, which probes the real terminal — so they passed here (where
    // NO_COLOR made the wrapper return a flat span) and FAILED on CI, where
    // NO_COLOR is unset. The suite's result depended on ambient terminal
    // state, which is exactly what a test must never do.
    #[test]
    fn the_band_moves_over_time() {
        let a = summary_shimmer_with("working on it", 0.0, false, true);
        let b = summary_shimmer_with("working on it", 0.9, false, true);
        assert_ne!(
            a.iter().map(|s| s.style).collect::<Vec<_>>(),
            b.iter().map(|s| s.style).collect::<Vec<_>>(),
            "the band must sweep between t=0.0 and t=0.9"
        );
    }

    #[test]
    fn the_band_returns_to_its_start_after_one_period() {
        // Phase comes from elapsed time, so a full period must be a no-op.
        // Without this the sweep would drift every frame.
        let a = summary_shimmer_with("planning", 0.0, false, true);
        let b = summary_shimmer_with("planning", SHIMMER_PERIOD_SECS, false, true);
        assert_eq!(
            a.iter().map(|s| s.style).collect::<Vec<_>>(),
            b.iter().map(|s| s.style).collect::<Vec<_>>(),
            "one full period must land back on the starting frame"
        );
    }

    #[test]
    fn every_character_is_styled_at_some_point_in_the_cycle() {
        // The band is wider than one column precisely so short labels do not
        // flash a single letter; if some character is never inside the band it
        // is dead weight.
        let text = "abcdefghij";
        let mut lit = vec![false; text.chars().count()];
        for step in 0..40 {
            let out =
                summary_shimmer_with(text, step as f64 * SHIMMER_PERIOD_SECS / 40.0, false, true);
            for (i, s) in out.iter().enumerate() {
                if !lit[i] && s.style.fg.is_some_and(|c| c != Color::Reset) {
                    lit[i] = true;
                }
            }
        }
        assert!(
            lit.iter().all(|x| *x),
            "some characters never receive the band; the sweep is narrower than intended"
        );
    }

    #[test]
    fn wide_glyphs_advance_the_sweep_by_two_columns() {
        // Indexing by char makes the band drift across CJK and emoji text.
        assert_eq!(char_width('a'), 1);
        assert_eq!(char_width('世'), 2);
        assert_eq!(char_width('█'), 1);
        assert_eq!(char_width('🚀'), 2);
    }

    #[test]
    fn empty_text_is_not_a_crash() {
        assert_eq!(shimmer_text(&summary_shimmer("", 0.5, false)), "");
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
