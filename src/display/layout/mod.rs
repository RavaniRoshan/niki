//! Layout system — chat layout, page layout, and overlay rendering.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::display::state::{AppState, PageId};
use crate::display::theme;

/// Render the main chat layout (conversational view).
pub fn render_chat(frame: &mut Frame, area: Rect, state: &AppState) {
    if area.height < 5 {
        return;
    }

    // Split area: messages + status bar + input
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // messages area
            Constraint::Length(1), // status bar
            Constraint::Length(1), // input box
        ])
        .split(area);

    // Render messages
    render_messages(frame, chunks[0], state);

    // Render status bar
    super::components::render_status_bar(frame, state, chunks[1]);

    // Render input box
    super::components::render_input_box(frame, &state.input_state, chunks[2]);
}

/// Render messages in the chat view.
fn render_messages(frame: &mut Frame, area: Rect, state: &AppState) {
    let mut lines: Vec<Line> = vec![];

    // Welcome banner (if no messages)
    if state.messages.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  ✦ Welcome to NIKI",
            theme::primary(),
        )));
        lines.push(Line::from(Span::styled(
            "  Send /help for help information.",
            theme::text_dim(),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  Directory: {}", state.project_path.display()),
            theme::text_dim(),
        )));
        lines.push(Line::from(Span::styled(
            format!("  Model:     {}", state.model),
            theme::text_dim(),
        )));
        lines.push(Line::from(Span::styled(
            "  Version:   0.2.0",
            theme::text_dim(),
        )));
        lines.push(Line::from(""));
    } else {
        // Render each message
        for msg in &state.messages {
            lines.extend(crate::display::chat::message::render_message(
                &msg_content(msg),
                msg_role(msg),
                &[],
                &crate::display::chat::message::MessageRenderConfig::from_theme(
                    area.width as usize,
                ),
            ));
        }
    }

    // Apply scroll offset
    let visible_lines = area.height as usize;
    let scroll = state
        .scroll_offset
        .min(lines.len().saturating_sub(visible_lines));
    let visible: Vec<_> = lines
        .iter()
        .skip(scroll)
        .take(visible_lines)
        .cloned()
        .collect();

    // Fill remaining space
    let mut display_lines = visible;
    while display_lines.len() < visible_lines {
        display_lines.push(Line::from(""));
    }

    frame.render_widget(Paragraph::new(display_lines), area);
}

/// Get message content as string.
fn msg_content(msg: &crate::display::state::Message) -> String {
    match msg {
        crate::display::state::Message::User { content, .. } => content.clone(),
        crate::display::state::Message::Assistant { content, .. } => content.clone(),
        crate::display::state::Message::System { content, .. } => content.clone(),
    }
}

/// Get message role.
fn msg_role(msg: &crate::display::state::Message) -> crate::display::chat::message::MessageRole {
    match msg {
        crate::display::state::Message::User { .. } => {
            crate::display::chat::message::MessageRole::User
        }
        crate::display::state::Message::Assistant { role, .. } => {
            crate::display::chat::message::MessageRole::Assistant(*role)
        }
        crate::display::state::Message::System { level, .. } => {
            crate::display::chat::message::MessageRole::System(match level {
                crate::display::state::SystemLevel::Info => {
                    crate::display::chat::message::SystemLevel::Info
                }
                crate::display::state::SystemLevel::Warning => {
                    crate::display::chat::message::SystemLevel::Warning
                }
                crate::display::state::SystemLevel::Error => {
                    crate::display::chat::message::SystemLevel::Error
                }
            })
        }
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
        let style = if matches!(state.view, crate::display::state::ViewMode::Page(p) if p == *page)
        {
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

    if state.pipeline.stages.is_empty() {
        lines.push(Line::from(Span::styled(
            "No stages yet...",
            theme::text_dim(),
        )));
    } else {
        for stage in &state.pipeline.stages {
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

    for stage in &state.pipeline.stages {
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
    fn msg_content_test() {
        let msg = crate::display::state::Message::User {
            content: "hello".to_string(),
            timestamp: chrono::Utc::now(),
        };
        assert_eq!(msg_content(&msg), "hello");
    }

    #[test]
    fn msg_role_test() {
        let msg = crate::display::state::Message::Assistant {
            content: "test".to_string(),
            role: crate::artifacts::types::AgentRole::Planner,
            timestamp: chrono::Utc::now(),
            tool_calls: vec![],
            thinking: None,
        };
        match msg_role(&msg) {
            crate::display::chat::message::MessageRole::Assistant(r) => {
                assert_eq!(r, crate::artifacts::types::AgentRole::Planner);
            }
            _ => panic!("Expected Assistant role"),
        }
    }

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
