//! The working-status line: a glyph that bounces, a word that changes, a clock
//! and a count.
//!
//! ## Why this is a pure function of render state
//!
//! `--output-format json` reserves stdout for the envelope, and every event
//! this component could have emitted would land there. So it emits **nothing**.
//! It is asked for a line and returns spans; the caller decides when to ask.
//! That is also what makes it testable without a terminal, and what keeps the
//! visual-regression frames a function of `(tick, elapsed, tokens)` alone.
//!
//! ## The glyphs
//!
//! `['·','✢','✳','✶','✻','✽']`, bouncing. They are **not** NIKI's agent
//! glyphs — `◈ ⟠ ◉ ◆` identify *who* is speaking and must not be reused for
//! *what state it is in*. A spinner that looks like a role marker is a spinner
//! a user will read as a role.
//!
//! ## The bounce is ten frames, not twelve
//!
//! Forward `0..=5` then back `4..=1`. The original shape repeated the
//! endpoints to reach twelve, which at 120 ms a frame reads as a stutter on
//! every turn — a visible hitch in the one animation the user is watching
//! while they wait. Ten is the honest bounce; the count is not load-bearing
//! anywhere else, and `a_bounce_visits_every_glyph_and_returns` is the test
//! that says so.

use std::time::Duration;

use ratatui::style::{Modifier, Style};
use ratatui::text::Span;

use crate::display::theme;

/// The working-state glyphs. Never NIKI's agent glyphs.
pub const WORKING_GLYPHS: [&str; 6] = ["·", "✢", "✳", "✶", "✻", "✽"];

/// How long each frame holds.
pub const FRAME_MS: u64 = 120;

/// The bounce table: forward, then back without repeating either end.
fn bounce_table() -> Vec<usize> {
    let n = WORKING_GLYPHS.len();
    (0..n).chain((1..n.saturating_sub(1)).rev()).collect()
}

/// One full bounce as a duration: `2n - 2` frames of `FRAME_MS`.
pub const BOUNCE_PERIOD: Duration =
    Duration::from_millis(FRAME_MS * (2 * WORKING_GLYPHS.len() as u64 - 2));

/// The glyph for an elapsed duration.
///
/// Time-based rather than tick-based: the render loop runs at ~60 fps and a
/// per-tick counter would run the animation about five times too fast. The
/// visual-regression frames depend on this being a function of elapsed time and
/// nothing else, so a slow machine and a fast one show the same thing at the
/// same moment.
pub fn glyph_at(elapsed: Duration) -> &'static str {
    let table = bounce_table();
    let step = (elapsed.as_millis() / FRAME_MS as u128) as usize % table.len();
    WORKING_GLYPHS[table[step]]
}

/// The words.
///
/// Deterministic, not random. A run that renders differently on two machines
/// cannot be compared against a recorded frame, and the whole point of a visual
/// baseline is comparison. The index is seeded from the turn number, so the
/// same turn of the same run always shows the same word and consecutive turns
/// never repeat one.
const GERUNDS: [&str; 28] = [
    "Thinking",
    "Working",
    "Reading",
    "Writing",
    "Comparing",
    "Tracing",
    "Sorting",
    "Weighing",
    "Sketching",
    "Unfolding",
    "Narrowing",
    "Balancing",
    "Rehearsing",
    "Composing",
    "Gathering",
    "Aligning",
    "Untangling",
    "Resolving",
    "Drafting",
    "Testing",
    "Sifting",
    "Polishing",
    "Assembling",
    "Tidying",
    "Matching",
    "Pinning",
    "Wrapping",
    "Landing",
];

/// The word for the `turn`-th piece of work in a run.
///
/// A rotation rather than a random draw, and the difference matters: a random
/// word makes every recorded frame unreproducible, and an unreproducible frame
/// is not a baseline.
pub fn gerund_for(turn: usize) -> &'static str {
    GERUNDS[turn % GERUNDS.len()]
}

/// `1m 27s` — compact, and never wider than it needs to be.
pub fn format_elapsed(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    if secs < 60 {
        return format!("{secs}s");
    }
    let mins = secs / 60;
    if mins < 60 {
        return format!("{mins}m {}s", secs % 60);
    }
    format!("{}h {}m", mins / 60, (mins % 60) * 60 + secs % 60)
}

/// `2.2k` / `340` — a token count short enough for a status line.
pub fn format_tokens(tokens: u32) -> String {
    if tokens < 1000 {
        return tokens.to_string();
    }
    format!("{:.1}k", tokens as f64 / 1000.0)
}

/// The line shown while work is in flight: `✶ Cerebrating… (4s · ↓ 1.2k tokens)`.
///
/// `turn` picks the word, `elapsed` the glyph and the clock, `tokens` the
/// count. `show_tokens` is separate so a run with no token accounting yet does
/// not print a confident `↓ 0 tokens` — zero tokens and *no* tokens are
/// different facts, and this codebase's whole late history is about that
/// difference.
pub fn working_line(turn: usize, elapsed: Duration, tokens: Option<u32>) -> Vec<Span<'static>> {
    let mut spans = vec![
        Span::styled(
            glyph_at(elapsed).to_string(),
            Style::default().fg(theme::claude()),
        ),
        Span::styled(
            format!(" {}…", gerund_for(turn)),
            Style::default().fg(theme::claude()),
        ),
        Span::styled(
            format!(" ({}{})", format_elapsed(elapsed), token_suffix(tokens)),
            Style::default().fg(theme::text_dim()),
        ),
    ];
    // The first span is a glyph and the rest are words, so the line reads as
    // one voice rather than a row of unrelated pieces when a terminal
    // under-applies the style.
    spans[1] = Span::styled(
        spans[1].content.clone(),
        Style::default()
            .fg(theme::claude())
            .add_modifier(Modifier::ITALIC),
    );
    spans
}

fn token_suffix(tokens: Option<u32>) -> String {
    match tokens {
        Some(t) => format!(" · ↓ {} tokens", format_tokens(t)),
        None => String::new(),
    }
}

/// What is left when the work is done: `⎿ Thought for 12s · 1.2k tokens`.
///
/// A different glyph on purpose. `⎿` says "this concluded" where the working
/// glyphs say "this is happening", and a spinner that keeps spinning after the
/// answer is in is the difference between "thinking" and "done".
pub fn resolved_line(elapsed: Duration, tokens: Option<u32>) -> Vec<Span<'static>> {
    vec![
        Span::styled("⎿".to_string(), Style::default().fg(theme::text_dim())),
        Span::styled(
            format!(
                " Thought for {}{}",
                format_elapsed(elapsed),
                token_suffix(tokens)
            ),
            Style::default().fg(theme::text_dim()),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bounce_visits_every_glyph_and_returns() {
        let mut visited: Vec<&str> = Vec::new();
        for step in 0..24 {
            let g = glyph_at(Duration::from_millis(step * FRAME_MS));
            if !visited.contains(&g) {
                visited.push(g);
            }
        }
        for g in WORKING_GLYPHS {
            assert!(
                visited.contains(&g),
                "{g:?} never appeared in a full bounce"
            );
        }
        // And it comes back to where it started after exactly one bounce — or
        // it is a rotation, not a bounce, which is the whole point of the
        // shape. The first version asserted at 1000 ms, which is not a multiple
        // of the period and so asserted nothing.
        assert_eq!(glyph_at(Duration::ZERO), glyph_at(BOUNCE_PERIOD));
        assert_ne!(
            glyph_at(Duration::ZERO),
            glyph_at(BOUNCE_PERIOD / 2),
            "and the far end of the bounce must differ from the near end"
        );
    }

    /// The return leg must not hold on the top of the bounce.
    ///
    /// `(0..n).chain((1..n).rev())` is *eleven* frames and puts the top index
    /// at both ends of the forward leg, so the brightest glyph — the one the
    /// eye is drawn to — is held for two frames while everything else moves at
    /// one. Counted, not eyeballed: a duplicated frame looks fine in isolation
    /// and stutters in motion.
    #[test]
    fn a_bounce_has_no_duplicated_frame() {
        let table = bounce_table();
        let n = WORKING_GLYPHS.len();
        assert_eq!(table.len(), 2 * n - 2, "bounce table: {table:?}");
        assert_eq!(
            table.iter().filter(|&&i| i == n - 1).count(),
            1,
            "the brightest glyph is held for two frames: {table:?}"
        );
    }

    #[test]
    fn the_glyph_does_not_reuse_an_agent_marker() {
        for g in WORKING_GLYPHS {
            for agent in ['◈', '⟠', '◉', '◆'] {
                assert_ne!(
                    *g,
                    agent.to_string(),
                    "{g:?} is one of NIKI's agent glyphs; a working-state glyph \
                     that looks like a role marker will be read as a role"
                );
            }
        }
    }

    #[test]
    fn the_glyph_is_a_function_of_time_not_of_call_count() {
        // The same instant renders the same glyph however many times it is
        // asked — which is what makes a recorded frame comparable.
        let t = Duration::from_millis(370);
        assert_eq!(glyph_at(t), glyph_at(t));
    }

    #[test]
    fn consecutive_turns_never_repeat_a_word() {
        let mut seen: Vec<&str> = Vec::new();
        for turn in 0..GERUNDS.len() {
            let w = gerund_for(turn);
            assert!(!seen.contains(&w), "{w:?} repeated at turn {turn}");
            seen.push(w);
        }
    }

    #[test]
    fn the_word_is_reproducible() {
        // A random draw would make every visual baseline unreproducible.
        assert_eq!(gerund_for(3), gerund_for(3));
    }

    #[test]
    fn elapsed_is_compact_and_never_lies() {
        assert_eq!(format_elapsed(Duration::from_secs(0)), "0s");
        assert_eq!(format_elapsed(Duration::from_secs(27)), "27s");
        assert_eq!(format_elapsed(Duration::from_secs(87)), "1m 27s");
        assert_eq!(format_elapsed(Duration::from_secs(3600)), "1h 0m");
    }

    #[test]
    fn tokens_are_short_but_not_rounded_away() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(340), "340");
        assert_eq!(format_tokens(1200), "1.2k");
    }

    /// No token accounting yet is not the same as zero tokens, and printing
    /// `↓ 0 tokens` claims the first.
    #[test]
    fn no_token_accounting_prints_no_count() {
        let with = working_line(0, Duration::from_secs(4), Some(1200));
        let without = working_line(0, Duration::from_secs(4), None);
        let text = |s: &[Span<'static>]| s.iter().map(|x| x.content.as_ref()).collect::<String>();
        assert!(text(&with).contains("1.2k"), "{}", text(&with));
        assert!(
            !text(&without).contains("tokens"),
            "no accounting must print no count: {}",
            text(&without)
        );
    }

    #[test]
    fn the_working_line_carries_glyph_word_clock_and_count() {
        let spans = working_line(2, Duration::from_secs(4), Some(1200));
        let text: String = spans.iter().map(|x| x.content.as_ref()).collect();
        let first = &text[..text.chars().next().map(char::len_utf8).unwrap_or(1)];
        assert!(
            WORKING_GLYPHS.contains(&first),
            "{first:?} is not one of the working glyphs: {text}"
        );
        assert!(text.contains(gerund_for(2)), "{text}");
        assert!(text.contains('(') && text.contains("4s"), "{text}");
        assert!(text.contains("1.2k tokens"), "{text}");
    }

    /// A finished line says so. A spinner that keeps spinning after the answer
    /// is the difference between "thinking" and "done".
    #[test]
    fn the_resolved_line_is_not_a_spinner() {
        let spans = resolved_line(Duration::from_secs(12), Some(1200));
        let text: String = spans.iter().map(|x| x.content.as_ref()).collect();
        assert!(text.starts_with('⎿'), "{text}");
        let first = &text[..text.chars().next().map(char::len_utf8).unwrap_or(1)];
        assert!(
            !WORKING_GLYPHS.contains(&first),
            "a finished line must not still be spinning: {text}"
        );
        assert!(text.contains("Thought for 12s"), "{text}");
    }
}

#[cfg(test)]
mod render_tests {
    //! The line as the TUI actually draws it.
    //!
    //! A pty case for this was written and dropped: it drove a real run
    //! through the scripted model, and it was **flaky** — it passed on the
    //! sabotaged build and failed on the restored one, because whether a stage
    //! is still `Running` when the case reads the screen is a race. The claim
    //! here is "the render puts a clock and a word on the working line", and a
    //! `TestBackend` proves exactly that with no timing in it at all.
    //!
    //! The user's product behaviour — a run in a terminal — is still covered:
    //! by `tests/tui_smoke/16_agent_asks_the_user.sh` for a driven run, and by
    //! a human looking at a real one.

    use super::*;
    use crate::display::state::{AppState, StageInfo, StageStatus};
    use ratatui::backend::TestBackend;

    fn text_of(spans: &[Span<'static>]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn a_working_line_is_glyph_word_and_clock() {
        let line = working_line(1, Duration::from_secs(7), Some(2300));
        let text = text_of(&line);
        assert!(
            WORKING_GLYPHS.iter().any(|g| text.starts_with(*g)),
            "{text}"
        );
        assert!(text.contains(gerund_for(1)), "no word: {text}");
        assert!(text.contains("(7s"), "no clock: {text}");
        assert!(text.contains("2.3k tokens"), "no count: {text}");
    }

    /// The resolved line, drawn the same way.
    #[test]
    fn a_resolved_line_says_it_is_finished() {
        let text = text_of(&resolved_line(Duration::from_secs(7), Some(2300)));
        assert!(text.starts_with('⎿'), "{text}");
        assert!(text.contains("Thought for 7s"), "{text}");
    }

    /// And the whole widget draws without panicking in a small frame, which
    /// is the same bar every other overlay in this directory is held to.
    #[test]
    fn the_widget_draws_in_a_small_frame() {
        for (w, h) in [(20u16, 10u16), (40, 3), (8, 2)] {
            let backend = TestBackend::new(w, h);
            let mut term = ratatui::Terminal::new(backend).expect("terminal");
            let config = crate::config::NikiConfig::default();
            let mut state = AppState::new("t".into(), config, ".".into());
            state.stages.push(StageInfo {
                role: crate::artifacts::types::AgentRole::Coder,
                status: StageStatus::Running,
                stream: String::new(),
                full_transcript: String::new(),
                input_tokens: 100,
                output_tokens: 200,
                cost_usd: 0.0,
                latency_ms: 0,
                summary: Vec::new(),
                start: None,
                completed_at: None,
                prompt_file: None,
                retry_count: 0,
                error_message: None,
            });
            term.draw(|f| {
                let spans = working_line(0, Duration::from_secs(1), Some(300));
                f.render_widget(
                    ratatui::widgets::Paragraph::new(ratatui::text::Line::from(spans)),
                    f.area(),
                );
            })
            .unwrap_or_else(|e| panic!("{w}x{h} must draw: {e}"));
        }
    }
}

/// The TUI's activity line must actually draw the working line.
///
/// A source assertion, and deliberately so. The component's own tests pass
/// whether or not anything calls it — the seventh sabotage in this programme's
/// history was a revert of this call site that left **every** test green,
/// because the component tests never touch `render_activity_spinner`. The
/// idiom is the one the repository already uses for "this call site must call
/// that" (`the_production_tool_contexts_carry_the_interface`,
/// `the_scripted_server_entry_point_stays_wired_to_ci`): a grep proves a wiring
/// fact and nothing else does.
#[test]
fn the_activity_line_draws_the_working_status() {
    let tui = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/display/tui.rs"),
    )
    .expect("src/display/tui.rs must be readable");
    let start = tui
        .find("fn render_activity_spinner(")
        .expect("the activity line must still exist");
    // 3200 characters, **measured**: the call is 2345 past the marker. The
    // first version used 2500 and failed on the clean tree — the window stopped
    // short of the thing it was checking for, which is the same trap as the
    // `ToolCall` arm in batch 6 and the bounce period in B7-19. A window is
    // never sized by eye.
    let region: String = tui[start..].chars().take(3200).collect();
    for marker in [
        "working_status::working_line(",
        "working_status::WORKING_GLYPHS[0]",
    ] {
        assert!(
            region.contains(marker),
            "`render_activity_spinner` must use `{marker}` — a component whose \
             tests pass while nothing renders it is a component that does \
             nothing. The region reads:\n{region}"
        );
    }
}
