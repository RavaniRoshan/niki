use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::pages::Modal;
use crate::display::theme;

pub fn render_modal(frame: &mut Frame, modal: &Modal, area: Rect) {
    // Dim scrim overlay — covers the entire screen behind the modal
    let scrim = Block::default().style(Style::default().bg(theme::surface_dark()));
    frame.render_widget(scrim, area);

    // `saturating_sub`, like every other overlay in this crate.
    //
    // This one subtracted raw `u16`. A terminal narrower than 4 columns
    // panicked immediately in a debug build — and in a *release* build, which
    // is what ships, `2u16 - 4` wraps to 65534, `min(50, …)` picks 50, and
    // `x` becomes 32767. The render then indexes a buffer 32767 columns wide,
    // which does not exist, and `Buffer::index_of` panics. So this was a
    // release-mode crash, reachable by resizing a window, and every other
    // overlay in the crate had already been fixed for exactly this.
    let popup_width = 50.min(area.width.saturating_sub(4));
    let popup_height = 10.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;

    let popup_area = Rect {
        x,
        y,
        width: popup_width,
        height: popup_height,
    };

    // Clear the popup area (on top of scrim)
    frame.render_widget(Clear, popup_area);

    // The combined error text is owned here and dropped at the end of the
    // frame. It used to be `Box::leak`ed to satisfy a `&'static str` the match
    // produced — harmless while nothing ever constructed `Modal::Error`, and
    // an unbounded leak once a stage failure actually opened this modal, since
    // every redraw leaked the same text again.
    let combined_error;
    let (title, message_str, border_color) = match modal {
        Modal::Confirm { title, message } => (title.as_str(), message.as_str(), theme::fg_color()),
        Modal::Error {
            stage,
            message,
            hint,
        } => {
            combined_error = format!("{message}\n\n{hint}");
            (stage.as_str(), combined_error.as_str(), theme::RED())
        }
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            format!(" {} ", title),
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
        ));

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {}", message_str),
            Style::default().fg(theme::fg_color()),
        )),
        Line::from(""),
    ];

    match modal {
        Modal::Confirm { .. } => {
            lines.push(Line::from(vec![
                Span::styled("      ", Style::default()),
                Span::styled(
                    "[Enter] confirm",
                    Style::default()
                        .fg(theme::GREEN())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("   [Esc] cancel", Style::default().fg(theme::fg_dim())),
            ]));
        }
        Modal::Error { .. } => {
            // No `[r]etry`. It was drawn as the primary, bold-amber action and
            // its only effect was `OverlayOutcome::Quit` — so the first thing a
            // user pressed on a failed run, having been told "retry", quit
            // NIKI and took the transcript with it. `ModalAction::Retry` is
            // still produced by the key and mouse handlers and is handled
            // explicitly below as a no-op, so re-adding a real retry later
            // cannot silently land on the quit path.
            lines.push(Line::from(vec![
                Span::styled("      ", Style::default()),
                Span::styled(
                    "[c]onfig",
                    Style::default()
                        .fg(theme::BLUE())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("   [Esc] back", Style::default().fg(theme::fg_dim())),
            ]));
        }
    }

    frame.render_widget(Paragraph::new(lines).block(block), popup_area);
}

/// Hit-test a mouse position against modal button regions.
pub fn modal_hit_test(
    mouse_col: u16,
    mouse_row: u16,
    area: Rect,
    modal: &Modal,
) -> Option<ModalAction> {
    // `saturating_sub`, like every other overlay in this crate.
    //
    // This one subtracted raw `u16`. A terminal narrower than 4 columns
    // panicked immediately in a debug build — and in a *release* build, which
    // is what ships, `2u16 - 4` wraps to 65534, `min(50, …)` picks 50, and
    // `x` becomes 32767. The render then indexes a buffer 32767 columns wide,
    // which does not exist, and `Buffer::index_of` panics. So this was a
    // release-mode crash, reachable by resizing a window, and every other
    // overlay in the crate had already been fixed for exactly this.
    let popup_width = 50.min(area.width.saturating_sub(4));
    let popup_height = 10.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;

    // Check if click is within the popup area
    if mouse_col < x
        || mouse_col >= x + popup_width
        || mouse_row < y
        || mouse_row >= y + popup_height
    {
        return None;
    }

    // The buttons are on the second-to-last line of the popup
    let button_row = y + popup_height - 2;
    if mouse_row != button_row {
        return None;
    }

    // Calculate relative column within the popup
    let rel_col = mouse_col - x;

    match modal {
        Modal::Confirm { .. } => {
            // "[Enter] confirm" starts at col 6, "[Esc] cancel" starts at col 24
            if (6..20).contains(&rel_col) {
                Some(ModalAction::Confirm)
            } else if (24..38).contains(&rel_col) {
                Some(ModalAction::Dismiss)
            } else {
                None
            }
        }
        Modal::Error { .. } => {
            // "[r]etry" at col 6, "[c]onfig" at col 15, "[Esc] back" at col 26
            if (6..13).contains(&rel_col) {
                // The retry button is gone; see `Modal::Error` rendering.
                Some(ModalAction::Dismiss)
            } else if (15..24).contains(&rel_col) {
                Some(ModalAction::Config)
            } else if (26..38).contains(&rel_col) {
                Some(ModalAction::Dismiss)
            } else {
                None
            }
        }
    }
}

pub fn handle_modal_key(key: KeyEvent, modal: &Modal) -> ModalAction {
    match key.code {
        KeyCode::Esc => ModalAction::Dismiss,
        KeyCode::Enter => match modal {
            Modal::Confirm { .. } => ModalAction::Confirm,
            Modal::Error { .. } => ModalAction::Dismiss,
        },
        KeyCode::Char('r') => match modal {
            Modal::Error { .. } => ModalAction::Retry,
            _ => ModalAction::None,
        },
        KeyCode::Char('c') => match modal {
            Modal::Error { .. } => ModalAction::Config,
            _ => ModalAction::None,
        },
        _ => ModalAction::None,
    }
}

pub enum ModalAction {
    None,
    Dismiss,
    Confirm,
    Retry,
    Config,
}
