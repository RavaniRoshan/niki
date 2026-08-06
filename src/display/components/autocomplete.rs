//! @ file autocomplete overlay.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::display::state::AppState;
use crate::display::theme;

/// Render the autocomplete overlay.
pub fn render_autocomplete(frame: &mut Frame, area: Rect, state: &AppState) {
    let Some(ref autocomplete) = state.input_state.autocomplete else {
        return;
    };

    if autocomplete.candidates.is_empty() {
        return;
    }

    let menu_width = 50u16.min(area.width.saturating_sub(4));
    let item_height = 1u16;
    let max_visible = 8usize;
    let visible = autocomplete.candidates.len().min(max_visible);
    let menu_height = (visible as u16) + 2;

    let x = (area.width - menu_width) / 2;
    let y = area.height.saturating_sub(menu_height + 3);

    let modal_area = Rect { x, y, width: menu_width, height: menu_height };

    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::border()))
        .style(Style::default().bg(theme::bg_elevated()));

    frame.render_widget(block, modal_area);

    let inner = Rect {
        x: modal_area.x + 2,
        y: modal_area.y + 1,
        width: menu_width.saturating_sub(4),
        height: menu_height.saturating_sub(2),
    };

    let mut lines = vec![];
    for (i, candidate) in autocomplete.candidates.iter().enumerate().take(visible) {
        let marker = if i == autocomplete.selected { "●" } else { " " };
        let color = if i == autocomplete.selected {
            theme::primary()
        } else {
            theme::text()
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", marker), color),
            Span::styled(candidate, Style::default().fg(color).add_modifier(Modifier::BOLD)),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), inner)
}

/// Build autocomplete candidates for a given prefix.
pub fn build_candidates(prefix: &str, project_files: &[String]) -> Vec<String> {
    let prefix_clean = prefix.trim_start_matches('@');
    project_files.iter()
        .filter(|f| f.contains(prefix_clean))
        .cloned()
        .take(20)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_candidates_test() {
        let files = vec![
            "src/main.rs".to_string(),
            "src/display/mod.rs".to_string(),
            "Cargo.toml".to_string(),
        ];
        let candidates = build_candidates("@src", &files);
        assert_eq!(candidates.len(), 2);
    }

    #[test]
    fn build_candidates_empty() {
        let files = vec!["Cargo.toml".to_string()];
        let candidates = build_candidates("@xyz", &files);
        assert!(candidates.is_empty());
    }
}
