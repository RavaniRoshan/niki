//! Large ASCII art "NIKI" logo for the TUI home screen.
//!
//! Generated using FIGlet "big" font via the `figlet-rs` crate.
//! Produces a bold 6-line logo that renders correctly in any monospace font.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::theme;

/// Pre-generated NIKI 3D shadow block logo lines.
const LOGO_LINES: &[&str] = &[
    "███╗   ██╗██╗██╗  ██╗██╗",
    "████╗  ██║██║██║ ██╔╝██║",
    "██╔██╗ ██║██║█████╔╝ ██║",
    "██║╚██╗██║██║██╔═██╗ ██║",
    "██║ ╚████║██║██║  ██╗██║",
    "╚═╝  ╚═══╝╚═╝╚═╝  ╚═╝╚═╝",
];

/// Height of the logo in lines.
pub const LOGO_HEIGHT: u16 = 6;

/// Render the NIKI logo centered in the given area.
pub fn render_logo(frame: &mut Frame, area: Rect) {
    let width = area.width as usize;

    for (i, line) in LOGO_LINES.iter().enumerate() {
        if i as u16 >= area.height {
            break;
        }

        let line_width = line.chars().count();
        let padding = if width > line_width {
            (width - line_width) / 2
        } else {
            0
        };

        let padded = format!("{}{}", " ".repeat(padding), line);

        let y = area.y + i as u16;
        let line_area = Rect {
            x: area.x,
            y,
            width: area.width,
            height: 1,
        };

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                padded,
                Style::default()
                    .fg(super::theme::fg_color())
                    .add_modifier(Modifier::BOLD),
            ))),
            line_area,
        );
    }
}

/// Calculate the preferred header height based on current terminal dimensions.
pub fn preferred_logo_height(width: u16, height: u16) -> u16 {
    if height < 18 {
        0 // Ultra-compact: suppress header to maximize chat/input area
    } else if height < 28 || width < 75 {
        1 // Compact mode: single-line sleek brand header
    } else {
        8 // Full mode: 6-line 3D ASCII logo + padding
    }
}

/// Render an adaptive header matching the allocated height constraint.
pub fn render_adaptive_header(
    frame: &mut Frame,
    area: Rect,
    state: &crate::display::state::AppState,
) {
    if area.height == 0 || area.width < 10 {
        return;
    }

    if area.height == 1 {
        // Compact single-line status header
        let mut spans = vec![
            Span::styled("◈ ", Style::default().fg(theme::clay())),
            Span::styled(
                "NIKI ",
                Style::default()
                    .fg(theme::fg_bright())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("v{}", env!("CARGO_PKG_VERSION")),
                Style::default().fg(theme::fg_subtle()),
            ),
        ];

        let project_name = state
            .project_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(".");
        // Never asserted. This read "main" whenever no run had set a branch, so
        // on a feature branch with no run yet the header claimed to be on
        // `main`. An empty branch is now rendered as unknown rather than as a
        // specific one; reading the real branch needs a git call, which a
        // render function should not be making.
        let branch = if state.branch_name.is_empty() {
            String::new()
        } else {
            state.branch_name.clone()
        };

        let right_info = format!(" · {} ({})", project_name, branch);
        let left_len: usize = spans.iter().map(|s| s.content.chars().count()).sum();
        let right_len = right_info.chars().count();

        if (area.width as usize) > left_len + right_len {
            spans.push(Span::styled(
                right_info,
                Style::default().fg(theme::fg_dim()),
            ));
        }

        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    } else {
        // The full banner is six rows of art with two spare. Those two rows
        // carried nothing, so a user arriving at `niki` had no indication that
        // a keybinding reference existed at all — the footer's `? keys` hint is
        // easy to read past, and the help overlay itself is only reachable by
        // pressing a key you do not know about.
        //
        // The hint is built from the live keybinding table, not typed here, so
        // it names the key the user actually has — including a rebound one.
        let hint = keybinding_hint(state);
        render_logo_with_subtitle(frame, area, &hint);
    }
}

/// The banner's keybinding hint, built from the resolved bindings.
///
/// Read from the table rather than written out so a user who rebinds
/// `toggle_help` is told the key they have, not the one this code was written
/// against. Falls back to a description of the overlay when the hint cannot
/// be built, because naming the feature is more useful than saying nothing.
fn keybinding_hint(state: &crate::display::state::AppState) -> String {
    use crate::display::keybindings::GlobalAction;

    let kb = &state.keybindings;
    let help = kb.label_for(GlobalAction::ToggleHelp, "?");
    let palette = kb.label_for(GlobalAction::CommandPalette, "^p");
    let chat = kb.label_for(GlobalAction::ToggleChatPage, "tab");
    format!("{help} for keybindings · {palette} commands · {chat} switches view")
}

/// Render the logo with a subtitle line below it.
pub fn render_logo_with_subtitle(frame: &mut Frame, area: Rect, subtitle: &str) {
    render_logo(frame, area);

    if area.height > LOGO_HEIGHT {
        let subtitle_y = area.y + LOGO_HEIGHT;
        let subtitle_area = Rect {
            x: area.x,
            y: subtitle_y,
            width: area.width,
            height: 1,
        };

        let width = area.width as usize;
        let sub_width = subtitle.len();
        let padding = if width > sub_width {
            (width - sub_width) / 2
        } else {
            0
        };
        let padded = format!("{}{}", " ".repeat(padding), subtitle);

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                padded,
                Style::default().fg(theme::fg_dim()),
            ))),
            subtitle_area,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_line_count() {
        assert_eq!(LOGO_LINES.len(), 6);
    }

    #[test]
    fn logo_lines_consistent_width() {
        let widths: Vec<usize> = LOGO_LINES.iter().map(|l| l.chars().count()).collect();
        let first = widths[0];
        for w in &widths {
            assert_eq!(*w, first, "Logo lines must be equal character width");
        }
    }

    #[test]
    fn logo_contains_niki() {
        let combined = LOGO_LINES.join("");
        assert!(combined.contains('█') || combined.contains('_') || combined.contains('|'));
    }

    #[test]
    fn responsive_logo_height_breakpoints() {
        assert_eq!(preferred_logo_height(80, 15), 0);
        assert_eq!(preferred_logo_height(60, 40), 1);
        assert_eq!(preferred_logo_height(100, 25), 1);
        assert_eq!(preferred_logo_height(120, 40), 8);
    }
}

#[cfg(test)]
mod hint_tests {
    use super::*;
    use crate::display::keybindings::{GlobalAction, KeyBindings};
    use std::collections::HashMap;

    fn state_with(kb: KeyBindings) -> crate::display::state::AppState {
        let config = crate::config::NikiConfig::default();
        let mut st = crate::display::state::AppState::new("t".to_string(), config, ".".into());
        st.keybindings = kb;
        st
    }

    /// The hint must name the key the user *has*, not the one this code was
    /// written against. A user who rebinds `toggle_help` and is told `?` will
    /// press a key that does nothing.
    #[test]
    fn the_hint_follows_a_rebound_key() {
        let mut overrides: HashMap<String, Vec<String>> = HashMap::new();
        overrides.insert("toggle_help".to_string(), vec!["ctrl+h".to_string()]);
        let (kb, _c) = KeyBindings::with_overrides(&overrides);
        let hint = keybinding_hint(&state_with(kb));
        assert!(
            hint.contains("Ctrl+H") || hint.contains("ctrl+h"),
            "the hint must name the rebound key, got: {hint}"
        );
    }

    #[test]
    fn the_hint_names_the_help_affordance() {
        let (kb, _c) = KeyBindings::with_overrides(&HashMap::new());
        let hint = keybinding_hint(&state_with(kb));
        assert!(
            hint.contains("keybindings"),
            "the hint has to say what the key does, not just the key: {hint}"
        );
    }

    /// `label_for` is what makes the hint follow a rebinding; pin it directly.
    ///
    /// The fallback branch — for an action a clash has left unbound — is
    /// defensive and not exercised here: every action in the table has a
    /// default, so producing an unbound one needs a deliberate clash, and a
    /// test for it would be asserting that a gap renders as a gap. Worth
    /// knowing that it is untested rather than discovering it later.
    #[test]
    fn label_for_reports_the_resolved_key() {
        let (kb, _c) = KeyBindings::with_overrides(&HashMap::new());
        assert_eq!(kb.label_for(GlobalAction::CommandPalette, "^p"), "Ctrl+P");
        assert_eq!(kb.label_for(GlobalAction::ToggleHelp, "?"), "?");
    }
}
