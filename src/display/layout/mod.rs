//! Layout system — chat layout, page layout, and overlay rendering.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};

use crate::display::pages::chat;
use crate::display::state::{AppState, HoverTarget, PageId};
use crate::display::theme;

/// Render the main chat layout (conversational view).
pub fn render_chat(frame: &mut Frame, area: Rect, state: &AppState) {
    if area.height < 5 {
        return;
    }

    // Multi-line composer: grow the input region up to ~1/3 of the screen when
    // the buffer contains newlines (Shift+Enter), otherwise keep it compact.
    let input_lines = state.input_state.buffer.lines().count().max(1);
    let max_input = ((area.height as usize) / 3).max(3);
    let input_h = (input_lines + 2).min(max_input).max(3) as u16;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),          // messages area
            Constraint::Length(input_h), // input box (grows for multi-line)
        ])
        .split(area);

    // Reserve a 1-column scrollbar on the right of the message area.
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(chunks[0]);
    let msg_area = body[0];

    // Render messages using the existing build_chat_lines (handles stages,
    // progressive disclosure, chat log). Skip inline input — rendered below.
    let lines = chat::build_chat_lines(state, msg_area.width as usize, false);
    let visible = chunks[0].height as usize;
    let total = lines.len();
    let max_scroll = total.saturating_sub(visible);
    let scroll = state.scroll_offset.min(max_scroll);

    // Scroll indicator: show "↑ more" when scrolled up
    let mut display_lines: Vec<Line> = Vec::with_capacity(visible);
    if scroll > 0 {
        let indicator_text = format!("  ↑ {} lines above  ", scroll);
        let indicator_style = Style::default()
            .fg(theme::fg_subtle())
            .add_modifier(ratatui::style::Modifier::ITALIC);
        display_lines.push(Line::from(Span::styled(indicator_text, indicator_style)));
    }

    let visible_lines: Vec<Line> = lines
        .iter()
        .skip(scroll)
        .take(visible.saturating_sub(display_lines.len()))
        .map(|cl| {
            cl.rich
                .clone()
                .unwrap_or_else(|| Line::from(cl.text.clone()))
        })
        .collect();
    display_lines.extend(visible_lines);
    while display_lines.len() < visible {
        display_lines.push(Line::from(""));
    }
    frame.render_widget(Paragraph::new(display_lines), msg_area);

    // Visible scrollbar (product-gaps P0: "users can't navigate without it").
    if total > visible && visible > 0 {
        let mut sb_state = ScrollbarState::new(total)
            .position(scroll)
            .viewport_content_length(visible);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .thumb_style(Style::default().fg(theme::scrollbar_thumb()))
                .track_style(Style::default().fg(theme::text_dim())),
            body[1],
            &mut sb_state,
        );
    }

    // Render input box (Claude Code elevated capsule). Use the multi-line
    // renderer when the composer holds newlines so long prompts stay readable.
    if state.input_state.buffer.contains('\n') {
        super::components::render_input_box_multiline(frame, &state.input_state, chunks[1]);
    } else {
        super::components::render_input_box(frame, state, chunks[1]);
    }
}

/// Render the page layout (tab-based page view).
pub fn render_page(frame: &mut Frame, area: Rect, page_id: PageId, state: &AppState) {
    if area.height < 5 {
        return;
    }

    // Split area: tab bar + content + footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // tab bar
            Constraint::Min(3),    // content
            Constraint::Length(2), // footer
        ])
        .split(area);

    // Tab bar
    let tabs = render_tab_bar(state, area.width as usize);
    frame.render_widget(Paragraph::new(tabs), chunks[0]);

    // Page content — for now, show placeholder
    let page_content = render_page_content(page_id, state, chunks[1].width as usize);
    frame.render_widget(Paragraph::new(page_content), chunks[1]);

    // Footer
    let footer = Line::from(Span::styled(
        "[Space] Pause  [Tab] Chat  [Ctrl+P] Commands  [/] Menu",
        theme::text_dim(),
    ));
    frame.render_widget(Paragraph::new(footer), chunks[2]);
}

/// Render the tab bar.
fn render_tab_bar(state: &AppState, width: usize) -> Line<'_> {
    let pages = [
        PageId::Pipeline,
        PageId::Agents,
        PageId::Diff,
        PageId::Verdict,
        PageId::Cost,
        PageId::Artifacts,
    ];

    let mut spans = vec![];
    for page in &pages {
        let title = page.title();
        if spans.iter().map(|s: &Span| s.content.len()).sum::<usize>() + title.len() + 4 > width {
            break;
        }
        let is_hovered = matches!(state.hover_target, HoverTarget::TabBar(idx) if idx == pages.iter().position(|p| p == page).unwrap_or(0));
        let is_active =
            matches!(state.view, crate::display::state::ViewMode::Page(p) if p == *page);
        let style = if is_hovered {
            Style::default()
                .fg(theme::primary())
                .bg(ratatui::style::Color::Rgb(40, 44, 52))
                .add_modifier(Modifier::BOLD)
        } else if is_active {
            Style::default()
                .fg(theme::primary())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::text_dim())
        };
        spans.push(Span::styled(format!("[{}] ", title), style));
    }

    Line::from(spans)
}

/// Hit-test a mouse position against tab bar regions.
pub fn tab_bar_hit_test(
    mouse_col: u16,
    area: Rect,
    _state: &AppState,
    width: usize,
) -> Option<PageId> {
    let pages = [
        PageId::Pipeline,
        PageId::Agents,
        PageId::Diff,
        PageId::Verdict,
        PageId::Cost,
        PageId::Artifacts,
    ];

    let mut col_offset = area.x as usize;
    for page in &pages {
        let title = page.title();
        let tab_width = title.len() + 4; // "[title] "
        if col_offset + tab_width > width {
            break;
        }
        if mouse_col >= col_offset as u16 && mouse_col < (col_offset + tab_width) as u16 {
            return Some(*page);
        }
        col_offset += tab_width;
    }
    None
}

use ratatui::style::Modifier;

/// Render page content (placeholder for now).
fn render_page_content(page_id: PageId, state: &AppState, width: usize) -> Vec<Line<'static>> {
    match page_id {
        PageId::Pipeline => render_pipeline_page(state, width),
        PageId::Agents => render_agents_page(state, width),
        PageId::Diff => render_diff_page(state, width),
        PageId::Verdict => render_verdict_page(state, width),
        PageId::Cost => render_cost_page(state, width),
        PageId::Artifacts => render_artifacts_page(state, width),
        _ => vec![Line::from(Span::styled(
            format!("{:?} page — press Tab to return to chat", page_id),
            theme::text_dim(),
        ))],
    }
}

/// Render pipeline page content.
fn render_pipeline_page(state: &AppState, _width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![];
    lines.push(Line::from(Span::styled(
        "Pipeline Status",
        theme::header_style(),
    )));
    lines.push(Line::from(""));

    if state.stages.is_empty() {
        lines.push(Line::from(Span::styled(
            "No stages yet...",
            theme::text_dim(),
        )));
    } else {
        for stage in &state.stages {
            let (icon, color) = match stage.status {
                crate::display::state::StageStatus::Running => ("◐", theme::primary()),
                crate::display::state::StageStatus::Done => ("✓", theme::success()),
                crate::display::state::StageStatus::Failed => ("✗", theme::error()),
                crate::display::state::StageStatus::Queued => ("○", theme::text_dim()),
            };
            let role_name = format!("{:?}", stage.role);
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", icon), color),
                Span::styled(role_name, color),
            ]));
        }
    }

    lines
}

/// Render agents page content.
fn render_agents_page(state: &AppState, _width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![];
    lines.push(Line::from(Span::styled(
        "Agent Configuration",
        theme::header_style(),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("Model: {}", state.model),
        theme::text(),
    )));
    lines.push(Line::from(Span::styled(
        format!(
            "Revision: {}/{}",
            state.revision_round, state.max_revision_rounds
        ),
        theme::text_dim(),
    )));
    lines
}

/// Render diff page content.
fn render_diff_page(state: &AppState, _width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![];
    lines.push(Line::from(Span::styled("Diff", theme::header_style())));
    lines.push(Line::from(""));

    if let Some(ref diff) = state.diff_content {
        for line in diff.lines().take(20) {
            let style = if line.starts_with('+') {
                Style::default().fg(theme::success())
            } else if line.starts_with('-') {
                Style::default().fg(theme::error())
            } else {
                Style::default().fg(theme::text())
            };
            lines.push(Line::from(Span::styled(line.to_string(), style)));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "No diff available yet...",
            theme::text_dim(),
        )));
    }

    lines
}

/// Render verdict page content.
fn render_verdict_page(state: &AppState, _width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![];
    lines.push(Line::from(Span::styled("Verdict", theme::header_style())));
    lines.push(Line::from(""));

    if let Some(ref report) = state.report_content {
        for line in report.lines().take(20) {
            lines.push(Line::from(Span::styled(line.to_string(), theme::text())));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "Waiting for review...",
            theme::text_dim(),
        )));
    }

    lines
}

/// Render cost page content.
fn render_cost_page(state: &AppState, _width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![];
    lines.push(Line::from(Span::styled(
        "Cost Breakdown",
        theme::header_style(),
    )));
    lines.push(Line::from(""));

    let (in_t, out_t, cost, _) = state.totals();
    lines.push(Line::from(Span::styled(
        format!("Input tokens:  {}", in_t),
        theme::text(),
    )));
    lines.push(Line::from(Span::styled(
        format!("Output tokens: {}", out_t),
        theme::text(),
    )));
    lines.push(Line::from(Span::styled(
        format!("Total cost:    ${:.4}", cost),
        theme::success(),
    )));
    lines.push(Line::from(""));

    for stage in &state.stages {
        let role_name = format!("{:?}", stage.role);
        lines.push(Line::from(Span::styled(
            format!(
                "{}: ${:.4} (in: {}, out: {})",
                role_name, stage.cost_usd, stage.input_tokens, stage.output_tokens
            ),
            theme::text_dim(),
        )));
    }

    lines
}

/// Render artifacts page content.
fn render_artifacts_page(state: &AppState, _width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![];
    lines.push(Line::from(Span::styled("Artifacts", theme::header_style())));
    lines.push(Line::from(""));

    if let Some(ref dir) = state.artifacts_dir {
        lines.push(Line::from(Span::styled(
            format!("Directory: {}", dir.display()),
            theme::text(),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "No artifacts directory set...",
            theme::text_dim(),
        )));
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_bar_renders() {
        let config = crate::config::NikiConfig::default();
        let state = AppState::new("test".to_string(), config, ".".into());
        let _line = render_tab_bar(&state, 80);
    }

    #[test]
    fn pipeline_page_empty() {
        let config = crate::config::NikiConfig::default();
        let state = AppState::new("test".to_string(), config, ".".into());
        let lines = render_pipeline_page(&state, 80);
        assert!(!lines.is_empty());
    }
}
