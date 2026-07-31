use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::display::theme;
use super::pages::Modal;

pub fn render_modal(frame: &mut Frame, modal: &Modal, area: Rect) {
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

    // Clear the area behind the modal
    frame.render_widget(Clear, popup_area);

    let (title, message_str, border_color) = match modal {
        Modal::Confirm { title, message } => {
            (title.as_str(), message.as_str(), theme::BLUE)
        }
        Modal::Error { stage, message, hint } => {
            let combined = format!("{}\n\n{}", message, hint);
            let leaked: &'static str = Box::leak(combined.into_boxed_str());
            (stage.as_str(), leaked, theme::RED)
        }
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            format!(" {} ", title),
            Style::default().fg(border_color).add_modifier(Modifier::BOLD),
        ));

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {}", message_str),
            Style::default().fg(theme::FG),
        )),
        Line::from(""),
    ];

    match modal {
        Modal::Confirm { .. } => {
            lines.push(Line::from(vec![
                Span::styled("      ", Style::default()),
                Span::styled("[Enter] confirm", Style::default().fg(theme::GREEN).add_modifier(Modifier::BOLD)),
                Span::styled("   [Esc] cancel", Style::default().fg(theme::FG_DIM)),
            ]));
        }
        Modal::Error { .. } => {
            lines.push(Line::from(vec![
                Span::styled("      ", Style::default()),
                Span::styled("[r]etry", Style::default().fg(theme::AMBER).add_modifier(Modifier::BOLD)),
                Span::styled("   [c]onfig   ", Style::default().fg(theme::BLUE)),
                Span::styled("[Esc] back", Style::default().fg(theme::FG_DIM)),
            ]));
        }
    }

    frame.render_widget(Paragraph::new(lines).block(block), popup_area);
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
