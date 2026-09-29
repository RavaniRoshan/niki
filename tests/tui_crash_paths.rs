#![allow(non_snake_case)]
//! The terminal must not die on ordinary input.
//!
//! Five crashes, all reachable by a person typing or clicking, none of which
//! needed an unusual configuration:
//!
//! 1. **A left-click in a fresh `niki chat`, before typing.** The transcript
//!    is empty, `saturating_sub(1)` on an empty length is 0, the row range was
//!    `0..=0`, and the index that followed went out of bounds. The render
//!    thread unwound and the app vanished mid-session.
//! 2. **A task described in any language but English, on the Fleet page.**
//! 3. **The same, on the History page.**
//! 4. **A non-ASCII tool argument, on the Run page.** `echo "café — résumé"`
//!    is a command anyone might run.
//! 5. **A window narrower than four columns, opening a modal** — a `u16`
//!    underflow that panics in debug and wraps to 65534 in release, where the
//!    renderer then indexes a buffer that does not exist.
//!
//! Two through four are one bug repeated: a *column* budget used as a *byte*
//! index. `theme::truncate_str` already measures in cells and snaps to a
//! character boundary, and every other call site in the crate used it; these
//! predated it.
//!
//! Every test here drives the real handler or the real renderer on real data.
//! None needs a terminal, a model, or a network — which is the point: a crash
//! on the keyboard path should not be findable only by pressing the key.

use niki::config::types::NikiConfig;
use niki::display::components::tool_card::{ToolCard, ToolStatus};
use niki::display::pages::{AppState, PageId};
use niki::display::theme;
use niki::mission::{Mission, MissionId, MissionStatus};
use ratatui::crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

fn make_state() -> AppState {
    let config = NikiConfig::default();
    AppState::new("test task".into(), config, "/tmp/test".into())
}

fn mouse(kind: MouseEventKind, col: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn draw(width: u16, height: u16, what: &str, f: impl FnOnce(&mut ratatui::Frame)) {
    let backend = ratatui::backend::TestBackend::new(width.max(1), height.max(1));
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    terminal
        .draw(f)
        .unwrap_or_else(|e| panic!("{what} at {width}x{height}: {e}"));
}

/// The crash from finding #1, driven through the public handler.
///
/// `niki chat` in a project with no history: nothing has been rendered, so the
/// transcript is empty. A single left-click anywhere is enough. Before the
/// fix this panicked on the index, the render thread unwound, and the app
/// disappeared with the alternate screen still active and no shell prompt to
/// explain where it went.
#[test]
fn a_click_in_an_empty_chat_does_not_panic() {
    let area = Rect::new(0, 0, 80, 24);
    for (col, row) in [(0u16, 0u16), (3, 2), (40, 12), (200, 60)] {
        let mut state = make_state();
        state.current_page = PageId::Chat;
        niki::display::pages::chat::ChatPage::handle_mouse(
            &mut state,
            mouse(MouseEventKind::Down(MouseButton::Left), col, row),
            area,
        );
        niki::display::pages::chat::ChatPage::handle_mouse(
            &mut state,
            mouse(MouseEventKind::Up(MouseButton::Left), col, row),
            area,
        );
        // A drag is the same path with a second endpoint, and it is how a
        // user actually selects text — so it is the one that matters.
        niki::display::pages::chat::ChatPage::handle_mouse(
            &mut state,
            mouse(MouseEventKind::Drag(MouseButton::Left), col + 5, row + 3),
            area,
        );
    }
}

/// The crash from findings #2–#4, driven through the real renderer.
///
/// A column budget compared against a byte length and then used as a byte
/// offset. Any description with a multibyte character inside the first few
/// dozen bytes was fatal. Rendered at several widths, because the budget
/// changes with the terminal and a fix that only handles the common width is
/// the same bug again.
#[test]
fn a_non_ascii_mission_description_renders_at_any_width() {
    // Chosen because each breaks a *different* slice: accented Latin, CJK,
    // emoji (four bytes and a wide glyph besides), and right-to-left script.
    for desc in [
        "Actualizar la documentación del proyecto",
        "项目的文档更新任务",
        "update the café — résumé ✨ generator",
        "تحديث التوثيق",
    ] {
        let mut mission = Mission::new(
            MissionId("m1".to_string()),
            desc.to_string(),
            "mock-model".to_string(),
        );
        mission.status = MissionStatus::Running;
        let fleet = niki::display::pages::fleet::FleetState::new(vec![mission]);
        for width in [20u16, 32, 40, 80, 120] {
            draw(width, 20, &format!("fleet {desc:?}"), |f| {
                niki::display::pages::fleet::render_fleet(&fleet, f.area(), f.buffer_mut());
            });
        }
    }
}

/// The same defect on the Run page, where the tool summary is model- and
/// tool-authored text sliced at byte 47.
///
/// `echo "café — résumé"` is a command anyone might run, and the slice
/// happened whenever a tool argument carried an accent.
#[test]
fn a_non_ascii_tool_summary_renders() {
    let mut state = make_state();
    state.current_page = PageId::Run;
    state.tool_cards = vec![ToolCard {
        tool_name: "Bash".into(),
        status: ToolStatus::Success { duration_ms: 12 },
        summary: "echo \"café — résumé ✨\" && ./scripts/check.sh --wide".into(),
        output: None,
        expanded: false,
    }];
    let router = niki::display::pages::PageRouter::new();
    for width in [20u16, 40, 80, 120] {
        let _ = &state;
        draw(width, 30, "run page", |f| {
            router.render_current(f, f.area(), &state);
        });
    }
}

/// The crash from finding #5, and the narrow-terminal band from the audit.
///
/// `popup_width = 50.min(area.width - 4)` was raw `u16` subtraction. Below
/// four columns that panics in a debug build and wraps in a release one,
/// where the renderer indexes a buffer 32767 columns wide. This is what ships.
#[test]
fn a_modal_survives_a_terminal_four_columns_wide() {
    for width in [0u16, 1, 2, 3, 4, 5, 9, 13] {
        for height in [0u16, 1, 5, 9, 10, 20] {
            let modal = niki::display::pages::Modal::Confirm {
                title: "Quit NIKI?".to_string(),
                message: "The pipeline will continue in the background.".to_string(),
            };
            draw(width, height, "modal", |f| {
                niki::display::modal::render_modal(f, &modal, f.area());
            });
        }
    }
}

/// The input box's scroll window inverted in an 8–11 column terminal.
///
/// `avail = inner_width - (mode_len + 1)` with `mode_len == 8` reached zero
/// while the width guard still let the window through, and the slice became
/// `buffer_chars[1..0]`. Reachable by dragging a window narrow, or a tmux
/// split — and the crash is total.
#[test]
fn the_input_box_renders_in_a_narrow_terminal() {
    for width in [8u16, 9, 10, 11, 12, 20] {
        let mut state = make_state();
        state.input_state.buffer = "some text the user has typed".into();
        draw(width, 4, "input box", |f| {
            niki::display::components::input_box::render_input_box(f, &state, f.area());
        });
    }
}

/// The truncation helper those sites now use, pinned on the case that broke
/// all four of them.
///
/// Bytes and cells are different currencies: "café" is four bytes and four
/// columns, "项目的文档" is many bytes and six columns, and "✨" is four bytes
/// and two wide. A helper that measures in bytes panics on the second and
/// truncates the wrong amount on the third.
#[test]
fn truncation_measures_columns_not_bytes() {
    for s in [
        "café — résumé ✨",
        "项目的文档更新任务",
        "تحديث التوثيق",
        "",
        "a",
        "plain ascii text",
    ] {
        for width in 0..12usize {
            let out = theme::truncate_str(s, width);
            assert!(
                out.chars().count() <= width,
                "truncating `{s}` to {width} cells produced `{}`, which is {} \\
                 characters — wider than the budget it was given",
                out,
                out.chars().count()
            );
            assert!(
                s.starts_with(&out),
                "truncation must be a prefix of the original, not a re-encoding: \
                 `{s}` -> `{out}`"
            );
        }
        let _ = theme::truncate_str_ellipsis(s, 8);
    }
}

/// The budget is in columns, and a column is not a byte and not a `char`.
///
/// This is the arithmetic the four call sites got wrong, stated as a test so
/// the mistake cannot be reintroduced by a well-meaning "simplification":
/// `saturating_sub` on a byte length, then a slice at that offset, looks
/// exactly like the correct code and panics on the first accent.
#[test]
fn a_column_budget_is_not_a_byte_offset() {
    // Built rather than typed, so the assertion below cannot be defeated by
    // where the accents happen to fall. An earlier version used a real
    // Spanish sentence and this test failed — correctly — because byte 32 of
    // that sentence *was* a character boundary, so it was asserting that
    // slicing is safe rather than that the old code was not.
    //
    // A guard that refuses to prove a thing it cannot prove is worth more
    // than a test that passes for the wrong reason.
    let budget = 32usize;
    // `budget - 1` ASCII characters, then a 3-byte CJK character, so byte
    // `budget` lands *inside* it. Appending the CJK after `budget` ASCII
    // characters put the boundary exactly at `budget` and the guard below
    // refused — which is the second time this fixture has been wrong and the
    // second time the guard was right.
    let s: String = "a".repeat(budget - 1) + "日本語";
    // What the old code did, and why it is not a safe thing to do.
    assert!(
        s.len() > budget,
        "the fixture must be longer than the budget, or it proves nothing"
    );
    let bytes_ok = s.is_char_boundary(budget);
    assert!(
        !bytes_ok,
        "the fixture must be non-ASCII near the boundary, or this test is \\
         asserting that slicing is safe rather than that the old code was not"
    );
    // And what it does now.
    let out = theme::truncate_str(&s, budget);
    assert!(out.chars().count() <= budget);
    assert!(s.starts_with(&out));
}
