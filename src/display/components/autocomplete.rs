//! @ file autocomplete overlay.

use nucleo::{Matcher, Utf32Str};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

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
    let _item_height = 1u16;
    let max_visible = 8usize;
    let visible = autocomplete.candidates.len().min(max_visible);
    let menu_height = (visible as u16) + 2;

    let x = (area.width - menu_width) / 2;
    let y = area.height.saturating_sub(menu_height + 3);

    let modal_area = Rect {
        x,
        y,
        width: menu_width,
        height: menu_height,
    };

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
        let marker = if i == autocomplete.selected {
            "●"
        } else {
            " "
        };
        let color = if i == autocomplete.selected {
            theme::primary()
        } else {
            theme::text()
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", marker), color),
            Span::styled(
                candidate,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), inner)
}

/// Maximum candidates shown/completed.
pub const MAX_CANDIDATES: usize = 20;

/// Score a candidate against the query with nucleo fuzzy matching
/// (subsequence + scoring, same engine as the slash menu). Returns `None`
/// for non-matches. Non-ASCII falls back to case-insensitive containment
/// (nucleo's fast path is ASCII-only).
fn fuzzy_score(candidate: &str, query: &str) -> Option<u16> {
    if query.is_empty() {
        return Some(u16::MAX);
    }
    if candidate.is_ascii() && query.is_ascii() {
        let mut m = Matcher::default();
        return m.fuzzy_match(
            Utf32Str::Ascii(candidate.as_bytes()),
            Utf32Str::Ascii(query.as_bytes()),
        );
    }
    if candidate.to_lowercase().contains(&query.to_lowercase()) {
        Some(1)
    } else {
        None
    }
}

/// Build autocomplete candidates for a given prefix.
///
/// Uses nucleo fuzzy ranking (so `@mc` matches `src/main.rs`) instead of
/// naive substring filtering. Empty prefix returns walk order. Ranked by
/// score, then shorter paths, then alphabetical for determinism.
pub fn build_candidates(prefix: &str, project_files: &[String]) -> Vec<String> {
    let query = prefix.trim_start_matches('@').to_ascii_lowercase();
    let mut scored: Vec<(u16, &String)> = project_files
        .iter()
        .filter_map(|f| fuzzy_score(&f.to_ascii_lowercase(), &query).map(|s| (s, f)))
        .collect();
    // Higher nucleo score = better; exact/prefix ties break short, then alpha.
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.len().cmp(&b.1.len()))
            .then_with(|| a.1.cmp(b.1))
    });
    scored
        .into_iter()
        .take(MAX_CANDIDATES)
        .map(|(_, f)| f.clone())
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

    #[test]
    fn build_candidates_fuzzy_noncontiguous() {
        let files = vec![
            "docs/readme.md".to_string(),
            "src/main.rs".to_string(),
            "src/display/mod.rs".to_string(),
        ];
        // "smn" is not a contiguous substring of anything — fuzzy still finds it.
        let candidates = build_candidates("@smn", &files);
        assert!(!candidates.is_empty());
        assert_eq!(candidates[0], "src/main.rs");
    }

    #[test]
    fn build_candidates_ranks_exact_first() {
        let files = vec![
            "src/display/mod.rs".to_string(),
            "src/main.rs".to_string(),
            "tests/main_test.rs".to_string(),
        ];
        let candidates = build_candidates("@main", &files);
        assert_eq!(candidates[0], "src/main.rs");
    }

    #[test]
    fn build_candidates_unicode_fallback() {
        let files = vec!["src/héllo.rs".to_string(), "src/world.rs".to_string()];
        let candidates = build_candidates("@héllo", &files);
        assert_eq!(candidates, vec!["src/héllo.rs".to_string()]);
    }

    #[test]
    fn build_candidates_bounded() {
        let files: Vec<String> = (0..100).map(|i| format!("src/file{i:03}.rs")).collect();
        assert_eq!(build_candidates("@", &files).len(), MAX_CANDIDATES);
    }
}
