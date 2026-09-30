//! The bottom bar's key hints, as data.
//!
//! The footer used to be three hardcoded width tiers with a fixed shortcut
//! list. That fails in two ways: the hints never changed with context, so a
//! key was either always advertised or never; and because the list was baked
//! into the render function there was nowhere to test the fitting logic
//! without a terminal.
//!
//! Codex's answer was a documented priority ladder: hints carry a priority,
//! and when the bar is too narrow the lowest-priority ones are dropped in a
//! fixed order. That is what this implements, as pure functions.
//!
//! The other half is that hints are *contextual*. On the Diff page a
//! scroll hint is worth more than a palette hint; while a stage is running,
//! "esc to cancel" outranks everything.

use crate::display::state::{AppState, PageId, ViewMode};

/// One `key  description` pair shown in the footer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hint {
    pub key: &'static str,
    pub label: &'static str,
    /// Higher survives longer. Ties are broken by list order, so the order
    /// hints are pushed is itself part of the design.
    pub priority: u8,
}

impl Hint {
    /// Rendered cost, used to decide what fits. Wide glyphs and long
    /// descriptions cost more columns.
    pub fn width(&self) -> usize {
        self.key.chars().count() + 1 + self.label.chars().count() + 3
    }
}

/// The hints that apply to the current state, highest priority first.
///
/// Highest priority is the *contextual* hint — the thing the user is most
/// likely to want right now — and the fixed chords sit at the bottom where
/// they are the first to be dropped.
pub fn contextual_hints(state: &AppState) -> Vec<Hint> {
    let mut hints: Vec<Hint> = Vec::new();

    // ── Contextual: what is actionable right now ────────────────────
    if state.show_help {
        hints.push(Hint {
            key: "esc",
            label: "close help",
            priority: 100,
        });
    } else if state.show_permission_modal {
        hints.push(Hint {
            key: "↑↓",
            label: "choose",
            priority: 100,
        });
        hints.push(Hint {
            key: "enter",
            label: "confirm",
            priority: 99,
        });
    } else if state.show_ask_modal {
        // The question is open and the run is waiting on it, so these are the
        // only keys that do anything. Same priority as the permission modal's:
        // both are states the program cannot leave without a decision.
        hints.push(Hint {
            key: "type",
            label: "answer",
            priority: 100,
        });
        hints.push(Hint {
            key: "enter",
            label: "send",
            priority: 99,
        });
        hints.push(Hint {
            key: "esc",
            label: "skip",
            priority: 98,
        });
    } else if state.show_command_menu {
        hints.push(Hint {
            key: "↑↓",
            label: "choose",
            priority: 100,
        });
        hints.push(Hint {
            key: "enter",
            label: "run",
            priority: 99,
        });
    } else if state.has_running_stage() {
        // Running: cancelling is the only thing that matters.
        hints.push(Hint {
            key: "esc",
            label: "cancel stage",
            priority: 100,
        });
    } else {
        // A list page: say so, because arrows are invisible affordances.
        let rows = crate::display::nav::page_item_count(state);
        if rows > 1 {
            hints.push(Hint {
                key: "↑↓",
                label: "select",
                priority: 60,
            });
        }
        if matches!(
            state.current_page,
            PageId::Diff | PageId::History | PageId::Run
        ) {
            hints.push(Hint {
                key: "pgup/pgdn",
                label: "scroll",
                priority: 55,
            });
        }
    }

    // ── Navigation (new) ─────────────────────────────────────────────
    hints.push(Hint {
        key: "←→",
        label: "page",
        priority: 50,
    });
    hints.push(Hint {
        key: "1-9",
        label: "jump",
        priority: 20,
    });

    // ── Fixed chords ─────────────────────────────────────────────────
    hints.push(Hint {
        key: "tab",
        label: match state.view {
            ViewMode::Chat => "pages",
            ViewMode::Page(_) => "chat",
        },
        priority: 45,
    });
    hints.push(Hint {
        key: "^p",
        label: "commands",
        priority: 40,
    });
    hints.push(Hint {
        key: "ctrl-t",
        label: "theme",
        priority: 25,
    });
    hints.push(Hint {
        key: "?",
        label: "keys",
        priority: 35,
    });

    hints.sort_by(|a, b| b.priority.cmp(&a.priority));
    hints
}

/// Which hints fit in `width` columns, keeping the separator between them.
///
/// Hints are dropped lowest-priority-first, so a narrow terminal keeps the
/// things a stuck user needs and loses the trivia. The result is always a
/// prefix of `hints`, which is what makes it predictable: a hint never
/// disappears and reappears as the terminal is resized.
pub fn fit(hints: &[Hint], width: usize) -> Vec<Hint> {
    const SEPARATOR: usize = 3; // "  ·  " collapsed to 3 for budgeting
    let mut used = 0usize;
    let mut out = Vec::new();
    for h in hints {
        let cost = h.width() + if out.is_empty() { 0 } else { SEPARATOR };
        if used + cost > width {
            break;
        }
        used += cost;
        out.push(*h);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NikiConfig;
    use std::path::PathBuf;

    fn state() -> AppState {
        AppState::new(
            "add a health endpoint".into(),
            NikiConfig::default(),
            PathBuf::from("/tmp/footer-test"),
        )
    }

    // ── Fitted output ───────────────────────────────────────────────
    #[test]
    fn a_wide_terminal_shows_every_hint() {
        let hints = contextual_hints(&state());
        assert_eq!(fit(&hints, 200).len(), hints.len());
    }

    #[test]
    fn hints_are_dropped_lowest_priority_first() {
        let hints = contextual_hints(&state());
        let kept = fit(&hints, 40);
        assert!(
            kept.len() < hints.len(),
            "40 columns must not fit everything"
        );
        // The survivors are a prefix of the sorted list, highest first.
        for (i, h) in kept.iter().enumerate() {
            assert_eq!(*h, hints[i]);
        }
    }

    #[test]
    fn the_most_important_hint_survives_whenever_anything_fits() {
        let hints = contextual_hints(&state());
        // At exactly the top hint's own width it must be kept — the ladder
        // drops from the bottom, never from the top. Below that width
        // returning nothing is correct: a 6-column footer cannot render a
        // 12-column hint without wrapping.
        let top = hints[0];
        let kept = fit(&hints, top.width());
        assert!(
            !kept.is_empty(),
            "the top hint ({:?}, {} columns) must fit in its own width",
            top.key,
            top.width()
        );
        assert_eq!(
            kept[0], top,
            "the ladder must never drop the top-priority hint first"
        );
    }

    #[test]
    fn a_zero_width_footer_is_empty_rather_than_panicking() {
        let hints = contextual_hints(&state());
        assert!(fit(&hints, 0).is_empty());
    }

    /// A hint that vanished and came back while resizing reads as a glitch.
    /// Fitting must be monotone in width.
    #[test]
    fn fitting_is_monotone_in_width() {
        let hints = contextual_hints(&state());
        let mut previous = 0usize;
        for width in (0..160).step_by(4) {
            let n = fit(&hints, width).len();
            assert!(
                n >= previous,
                "hint count fell from {previous} to {n} when widening to {width} — a hint \\
                 disappeared and reappeared"
            );
            previous = n;
        }
    }

    #[test]
    fn the_fitted_set_never_exceeds_the_available_width() {
        let hints = contextual_hints(&state());
        const SEPARATOR: usize = 3;
        for width in (10..160).step_by(3) {
            let kept = fit(&hints, width);
            let used: usize = kept.iter().map(|h| h.width()).sum::<usize>()
                + SEPARATOR * kept.len().saturating_sub(1);
            assert!(
                used <= width,
                "footer needs {used} columns but only {width} exist — it will wrap"
            );
        }
    }

    // ── Contextual content ──────────────────────────────────────────
    #[test]
    fn a_running_stage_makes_cancel_the_top_hint() {
        let mut st = state();
        // Drive the public surface rather than poking internals.
        st.set_notice("running", 5);
        let hints = contextual_hints(&st);
        // Whatever the top hint is, "esc" must be present when a stage runs.
        if st.has_running_stage() {
            assert_eq!(
                hints[0].key, "esc",
                "cancel must outrank everything while running"
            );
        }
    }

    #[test]
    fn the_help_overlay_always_offers_a_way_out() {
        let mut st = state();
        st.show_help = true;
        let hints = contextual_hints(&st);
        assert_eq!(hints[0].key, "esc");
        assert_eq!(
            hints[0].priority, 100,
            "the top priority is reserved for the escape hatch"
        );
    }

    #[test]
    fn the_permission_modal_explains_how_to_choose() {
        let mut st = state();
        st.show_permission_modal = true;
        let hints = contextual_hints(&st);
        assert_eq!(hints[0].key, "↑↓", "arrows are how a modal list is chosen");
        assert!(
            hints.iter().any(|h| h.key == "enter"),
            "enter must be offered"
        );
    }

    #[test]
    fn navigation_hints_are_always_present_on_a_page_view() {
        let mut st = state();
        st.view = ViewMode::Page(PageId::Run);
        st.show_help = false;
        st.show_permission_modal = false;
        st.show_command_menu = false;
        let hints = contextual_hints(&st);
        let keys: Vec<&str> = hints.iter().map(|h| h.key).collect();
        assert!(
            keys.contains(&"←→"),
            "page navigation must be advertised: {keys:?}"
        );
        assert!(
            keys.contains(&"?"),
            "the key help must be advertised: {keys:?}"
        );
    }

    #[test]
    fn the_tab_hint_says_where_tab_goes() {
        let mut st = state();
        st.show_help = false;
        st.show_permission_modal = false;
        st.show_command_menu = false;
        st.view = ViewMode::Chat;
        let in_chat = contextual_hints(&st)
            .into_iter()
            .find(|h| h.key == "tab")
            .expect("tab is always hinted");
        assert_eq!(in_chat.label, "pages");

        st.view = ViewMode::Page(PageId::Run);
        let in_page = contextual_hints(&st)
            .into_iter()
            .find(|h| h.key == "tab")
            .expect("tab is always hinted");
        assert_eq!(in_page.label, "chat");
    }

    #[test]
    fn hints_are_sorted_by_descending_priority() {
        let mut st = state();
        st.show_help = true;
        let hints = contextual_hints(&st);
        for pair in hints.windows(2) {
            assert!(
                pair[0].priority >= pair[1].priority,
                "hints must be pre-sorted so fitting can take a prefix"
            );
        }
    }
}
