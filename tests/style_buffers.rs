//! Style-fidelity tests: colored `Buffer` equality / per-cell fg+bg asserts
//! for the two most style-critical surfaces — the Codex-style side-by-side
//! diff renderer and the status bar.
//!
//! Cell-level snapshots (page_snapshots.rs) are symbol-only because ratatui's
//! Display drops styles (#1402); colors are asserted here directly against
//! `Cell::fg`/`bg` — the maintainer-endorsed approach from ratatui#1402.
//!
//! Expectations call the same `theme::*` functions the production code uses,
//! so these hold regardless of which palette `ColorDepth::detect()` picks.

use niki::config::types::NikiConfig;
use niki::display::components::status_bar::render_status_bar;
use niki::display::diff_display::render_diff;
use niki::display::pages::{AppState, PageId};
use niki::display::theme;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use ratatui::text::Text;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use ratatui::{Frame, Terminal};

const SAMPLE_DIFF: &str = "\
--- a/src/list.rs\n\
+++ b/src/list.rs\n\
@@ -28,7 +28,7 @@\n\
 fn paginate(items: &[Item], start: usize, size: usize) -> &[Item] {\n\
-    let end = start + size - 1;\n\
+    let end = start + size;\n\
     &items[start..end]\n\
 }";

fn paint_diff(diff: &str, width: u16) -> ratatui::buffer::Buffer {
    let lines = render_diff(diff, width);
    let height = lines.len() as u16;
    let area = ratatui::layout::Rect::new(0, 0, width, height.max(1));
    let mut buf = ratatui::buffer::Buffer::empty(area);
    Paragraph::new(Text::from(lines)).render(area, &mut buf);
    buf
}

fn seeded_state() -> AppState {
    let mut state = AppState::new(
        "style probe".to_string(),
        NikiConfig::default(),
        std::path::PathBuf::from("/tmp/niki-style"),
    );
    state.current_page = PageId::Chat;
    state.tick = 0;
    state.model = "mock-model".to_string();
    state.branch_name = "niki/e5c8f1".to_string();
    state
}

fn row_cells(buf: &ratatui::buffer::Buffer, y: u16) -> Vec<ratatui::buffer::Cell> {
    (0..buf.area.width).map(|x| buf[(x, y)].clone()).collect()
}

fn row_text(buf: &ratatui::buffer::Buffer, y: u16) -> String {
    row_cells(buf, y).iter().map(|c| c.symbol()).collect()
}

/// The diff gutter renders as `{:>4} {sign} ` — classify a row by its sign
/// column rather than by content, because word-level intra-line emphasis
/// interleaves +/- markers into the visible text of changed lines.
fn row_sign(row: &str) -> Option<char> {
    let t = row.trim_start();
    let digits: usize = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    let rest = &t[digits..];
    let rest = rest.trim_start_matches(' ');
    let mut chars = rest.chars();
    match chars.next()? {
        '+' => Some('+'),
        '-' => Some('-'),
        _ => None,
    }
}

fn diff_uses_backgrounds() -> bool {
    !matches!(
        theme::ColorDepth::detect(),
        theme::ColorDepth::NoColor | theme::ColorDepth::Ansi16
    )
}

#[test]
fn diff_added_line_uses_add_styling() {
    let buf = paint_diff(SAMPLE_DIFF, 80);
    let height = buf.area.height;
    let add_row = (0..height)
        .find(|&y| row_sign(&row_text(&buf, y)) == Some('+'))
        .unwrap_or_else(|| {
            panic!(
                "added line not rendered; rows:\n{}",
                (0..height)
                    .map(|y| row_text(&buf, y))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        });
    let cells = row_cells(&buf, add_row);
    if diff_uses_backgrounds() {
        let styled: Vec<_> = cells.iter().filter(|c| c.bg != Color::Reset).collect();
        assert!(
            !styled.is_empty(),
            "added row carries no background styling"
        );
        let expected = theme::DIFF_ADD_BG();
        for c in styled {
            assert_eq!(c.bg, expected, "cell {:?} has wrong add-bg", c.symbol());
        }
    } else {
        // Foreground-only mode: the sign/content must still carry the add fg.
        let expected = theme::DIFF_ADD_FG();
        assert!(
            cells.iter().any(|c| c.fg == expected && c.symbol() != " "),
            "added row not painted with DIFF_ADD_FG in no-bg mode"
        );
    }
}

#[test]
fn diff_removed_line_uses_del_styling() {
    let buf = paint_diff(SAMPLE_DIFF, 80);
    let height = buf.area.height;
    let del_row = (0..height)
        .find(|&y| row_sign(&row_text(&buf, y)) == Some('-'))
        .expect("removed line not rendered");
    let cells = row_cells(&buf, del_row);
    if diff_uses_backgrounds() {
        let styled: Vec<_> = cells.iter().filter(|c| c.bg != Color::Reset).collect();
        assert!(
            !styled.is_empty(),
            "removed row carries no background styling"
        );
        let expected = theme::DIFF_DEL_BG();
        for c in styled {
            assert_eq!(c.bg, expected, "cell {:?} has wrong del-bg", c.symbol());
        }
    } else {
        let expected = theme::DIFF_DEL_FG();
        assert!(
            cells.iter().any(|c| c.fg == expected && c.symbol() != " "),
            "removed row not painted with DIFF_DEL_FG in no-bg mode"
        );
    }
}

#[test]
fn diff_hunk_header_is_styled_distinctly() {
    let buf = paint_diff(SAMPLE_DIFF, 80);
    let height = buf.area.height;
    let hunk_row = (0..height)
        .find(|&y| row_text(&buf, y).contains("@@"))
        .expect("hunk header not rendered");
    let expected = theme::DIFF_HUNK();
    let cells = row_cells(&buf, hunk_row);
    let styled: Vec<_> = cells.iter().filter(|c| c.fg != Color::Reset).collect();
    assert!(!styled.is_empty(), "hunk header carries no fg styling");
    for c in styled {
        assert_eq!(c.fg, expected, "cell {:?} has wrong hunk fg", c.symbol());
    }
}

#[test]
fn added_and_removed_palettes_differ_when_color_available() {
    // Guard against a degenerate palette silently passing the two tests above.
    if matches!(
        theme::ColorDepth::detect(),
        theme::ColorDepth::NoColor | theme::ColorDepth::Ansi16
    ) {
        return;
    }
    assert_ne!(
        theme::DIFF_ADD_BG(),
        theme::DIFF_DEL_BG(),
        "add/remove backgrounds must be visually distinct"
    );
}

#[test]
fn status_bar_renders_mode_badge_and_hint_colors() {
    let width = 100u16;
    let height = 3u16;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    let state = seeded_state();
    terminal
        .draw(|f: &mut Frame| {
            let area = f.area();
            render_status_bar(f, &state, area);
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();

    // Paragraph paints its single line at the TOP of the given area.
    let bar_row = 0u16;
    let text = row_text(&buf, bar_row);
    assert!(text.contains("MANUAL"), "mode badge missing: {text}");

    // Key hints use the subtle foreground — verify at least one cell does.
    let subtle = theme::fg_subtle();
    let hint_cells: Vec<_> = row_cells(&buf, bar_row)
        .into_iter()
        .filter(|c| c.fg == subtle && c.symbol() != " ")
        .collect();
    assert!(
        !hint_cells.is_empty(),
        "no status-bar hints painted with fg_subtle"
    );
}

#[test]
fn status_bar_context_meter_reflects_usage_color_thresholds() {
    // The meter must change color as context fills — pin the two extremes.
    let mut low = seeded_state();
    low.context_usage = 0.05;
    let mut high = seeded_state();
    high.context_usage = 0.95;

    fn meter_fg(state: &AppState) -> Vec<Color> {
        let width = 100u16;
        let backend = TestBackend::new(width, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f: &mut Frame| {
                render_status_bar(f, state, f.area());
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        row_cells(&buf, 0).into_iter().map(|c| c.fg).collect()
    }

    let low_fgs = meter_fg(&low);
    let high_fgs = meter_fg(&high);
    // Somewhere in the row the accent color differs once context is nearly
    // exhausted; if both states paint identical fg sequences the threshold
    // wiring regressed.
    assert_ne!(
        low_fgs, high_fgs,
        "context-meter coloring did not react to usage level"
    );
}
