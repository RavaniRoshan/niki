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

    let popup_width = 50.min(area.width - 4);
    let popup_height = 10.min(area.height - 4);
    let x = (area.width - popup_width) / 2;
    let y = (area.height - popup_height) / 2;

    let popup_area = Rect {
        x,
        y,
        width: popup_width,
        height: popup_height,
    };

    // Clear the popup area (on top of scrim)
    frame.render_widget(Clear, popup_area);

    let (title, message_str, border_color) = match modal {
        Modal::Confirm { title, message } => (title.as_str(), message.as_str(), theme::fg_color()),
        Modal::Error {
            stage,
            message,
            hint,
        } => {
            let combined = format!("{}\n\n{}", message, hint);
            let leaked: &'static str = Box::leak(combined.into_boxed_str());
            (stage.as_str(), leaked, theme::RED())
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
            lines.push(Line::from(vec![
                Span::styled("      ", Style::default()),
                Span::styled(
                    "[r]etry",
                    Style::default()
                        .fg(theme::AMBER())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("   [c]onfig   ", Style::default().fg(theme::BLUE())),
                Span::styled("[Esc] back", Style::default().fg(theme::fg_dim())),
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
    let popup_width = 50.min(area.width - 4);
    let popup_height = 10.min(area.height - 4);
    let x = (area.width - popup_width) / 2;
    let y = (area.height - popup_height) / 2;

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
                Some(ModalAction::Retry)
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
    Skip,
}
