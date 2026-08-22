//! Message rendering for conversational chat display.
//!
//! Renders user, assistant, and system messages with proper styling,
//! role-colored icons, and markdown content.

use chrono::{DateTime, Utc};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::artifacts::types::AgentRole;

use super::markdown::render_markdown;

/// Configuration for message rendering.
#[derive(Debug, Clone)]
pub struct MessageRenderConfig {
    pub width: usize,
    pub show_timestamps: bool,
    pub role_user_color: Color,
    pub role_assistant_color: Color,
    pub role_system_color: Color,
    pub text_color: Color,
    pub text_dim_color: Color,
    pub border_color: Color,
    pub success_color: Color,
    pub warning_color: Color,
    pub error_color: Color,
    pub claude_color: Color,
    pub primary_color: Color,
}

impl MessageRenderConfig {
    /// Create a config from the theme system.
    pub fn from_theme(width: usize) -> Self {
        Self {
            width,
            show_timestamps: false,
            role_user_color: crate::display::theme::role_user(),
            role_assistant_color: crate::display::theme::role_assistant(),
            role_system_color: crate::display::theme::text_dim(),
            text_color: crate::display::theme::text(),
            text_dim_color: crate::display::theme::text_dim(),
            border_color: crate::display::theme::border(),
            success_color: crate::display::theme::success(),
            warning_color: crate::display::theme::warning(),
            error_color: crate::display::theme::error(),
            claude_color: crate::display::theme::claude(),
            primary_color: crate::display::theme::primary(),
        }
    }
}

/// Render a user message.
pub fn render_user_message(
    content: &str,
    timestamp: &DateTime<Utc>,
    config: &MessageRenderConfig,
) -> Vec<Line<'static>> {
    let mut lines = vec![];

    // Header: ● user (gold bullet)
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(config.role_user_color)),
        Span::styled(
            "user",
            Style::default()
                .fg(config.role_user_color)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    // Separator
    lines.push(Line::from(""));

    // Content as markdown
    lines.extend(render_markdown(content, config.width, config, true));

    // Timestamp (if enabled)
    if config.show_timestamps {
        lines.push(Line::from(Span::styled(
            timestamp.format("%H:%M:%S").to_string(),
            Style::default().fg(config.text_dim_color),
        )));
    }

    lines
}

/// Render an assistant message with role-colored icon.
pub fn render_assistant_message(
    content: &str,
    role: AgentRole,
    tool_calls: &[ToolCallDisplay],
    config: &MessageRenderConfig,
) -> Vec<Line<'static>> {
    let mut lines = vec![];

    let (icon, color) = role_icon_and_color(role, config);

    // Header: ◈ planner (role-colored icon + label)
    lines.push(Line::from(vec![
        Span::styled(
            format!("{} ", icon),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            role_label(role).to_string(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ]));

    // Separator
    lines.push(Line::from(""));

    // Content as markdown
    lines.extend(render_markdown(content, config.width, config, true));

    // Tool calls — collapse consecutive read-only operations
    let groups = group_tool_calls(tool_calls);
    for group in groups {
        match group {
            ToolCallGroup::Single(idx) => {
                lines.extend(render_tool_call(&tool_calls[idx], config));
            }
            ToolCallGroup::CollapsedRead { count, tools } => {
                lines.extend(render_collapsed_read_group(count, &tools));
            }
        }
    }

    lines
}

/// Render a system message.
pub fn render_system_message(
    content: &str,
    level: SystemLevel,
    config: &MessageRenderConfig,
) -> Vec<Line<'static>> {
    let color = match level {
        SystemLevel::Info => config.text_dim_color,
        SystemLevel::Warning => config.warning_color,
        SystemLevel::Error => config.error_color,
    };

    let icon = match level {
        SystemLevel::Info => "ℹ ",
        SystemLevel::Warning => "⚠ ",
        SystemLevel::Error => "✗ ",
    };

    vec![
        Line::from(vec![
            Span::styled(icon, Style::default().fg(color)),
            Span::styled(content.to_string(), Style::default().fg(color)),
        ]),
        Line::from(""),
    ]
}

/// Render a tool call.
pub fn render_tool_call(
    tool_call: &ToolCallDisplay,
    config: &MessageRenderConfig,
) -> Vec<Line<'static>> {
    let mut lines = vec![];

    let (icon, color) = match tool_call.status {
        ToolCallStatus::Running => ("⏵ ", crate::display::theme::warning()),
        ToolCallStatus::Done => ("⎿ ", config.success_color),
        ToolCallStatus::Failed => ("✗ ", config.error_color),
    };

    // Tool call header
    lines.push(Line::from(vec![
        Span::styled("  ", Style::default().fg(config.text_color)),
        Span::styled(icon, Style::default().fg(color)),
        Span::styled(
            tool_call.tool_name.clone(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ]));

    // Summary if available
    if let Some(ref summary) = tool_call.summary {
        lines.push(Line::from(vec![
            Span::styled("    ", Style::default().fg(config.text_color)),
            Span::styled(summary.clone(), Style::default().fg(config.text_dim_color)),
        ]));
    }

    lines
}

/// Main render dispatch for any message type.
pub fn render_message(
    content: &str,
    role: MessageRole,
    tool_calls: &[ToolCallDisplay],
    config: &MessageRenderConfig,
) -> Vec<Line<'static>> {
    match role {
        MessageRole::User => render_user_message(content, &Utc::now(), config),
        MessageRole::Assistant(agent_role) => {
            render_assistant_message(content, agent_role, tool_calls, config)
        }
        MessageRole::System(level) => render_system_message(content, level, config),
    }
}

/// Message role types for rendering.
#[derive(Debug, Clone)]
pub enum MessageRole {
    User,
    Assistant(AgentRole),
    System(SystemLevel),
}

/// Tool call display info.
#[derive(Debug, Clone)]
pub struct ToolCallDisplay {
    pub tool_name: String,
    pub status: ToolCallStatus,
    pub summary: Option<String>,
}

/// Tool call status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCallStatus {
    Running,
    Done,
    Failed,
}

/// Check if a tool is read-only (safe to collapse).
#[allow(dead_code)]
fn is_read_only_tool(name: &str) -> bool {
    matches!(name, "read" | "glob" | "grep" | "list")
}

/// Group consecutive read-only tool calls into collapsed summaries.
fn group_tool_calls(calls: &[ToolCallDisplay]) -> Vec<ToolCallGroup> {
    let mut groups: Vec<ToolCallGroup> = Vec::new();
    let mut read_only_buf: Vec<(usize, &'static str)> = Vec::new();

    for (i, tc) in calls.iter().enumerate() {
        if let Some(static_name) = read_only_tool_name(&tc.tool_name) {
            if tc.status == ToolCallStatus::Done {
                read_only_buf.push((i, static_name));
                continue;
            }
        }
        if !read_only_buf.is_empty() {
            groups.push(flush_read_only_group(&read_only_buf));
            read_only_buf.clear();
        }
        groups.push(ToolCallGroup::Single(i));
    }
    if !read_only_buf.is_empty() {
        groups.push(flush_read_only_group(&read_only_buf));
    }
    groups
}

/// Map a tool name to a `'static str` if it's a known read-only tool.
fn read_only_tool_name(name: &str) -> Option<&'static str> {
    match name {
        "read" => Some("read"),
        "glob" => Some("glob"),
        "grep" => Some("grep"),
        "list" => Some("list"),
        _ => None,
    }
}

/// A rendered tool call group — either a single call or a collapsed summary.
enum ToolCallGroup {
    Single(usize),
    CollapsedRead {
        count: usize,
        tools: Vec<&'static str>,
    },
}

/// Collapse a buffer of consecutive read-only calls into a summary.
fn flush_read_only_group(calls: &[(usize, &'static str)]) -> ToolCallGroup {
    let mut tool_counts: Vec<(&str, usize)> = Vec::new();
    for &(_, name) in calls {
        if let Some(entry) = tool_counts.iter_mut().find(|(t, _)| *t == name) {
            entry.1 += 1;
        } else {
            tool_counts.push((name, 1));
        }
    }
    let tools: Vec<&'static str> = tool_counts.into_iter().map(|(t, _)| t).collect();
    ToolCallGroup::CollapsedRead {
        count: calls.len(),
        tools,
    }
}

/// Render a collapsed read-only group as a single dimmed line.
fn render_collapsed_read_group(count: usize, tools: &[&str]) -> Vec<Line<'static>> {
    let tool_desc = if tools.len() == 1 {
        match tools[0] {
            "read" => format!("Read {} files", count),
            "glob" => format!("Searched {} patterns", count),
            "grep" => format!("Grep {} queries", count),
            "list" => format!("Listed {} directories", count),
            other => format!("{} × {} calls", other, count),
        }
    } else {
        format!("{} read-only operations", count)
    };
    vec![Line::from(Span::styled(
        format!("  ⎿ {} (collapsed)", tool_desc),
        Style::default().fg(Color::DarkGray),
    ))]
}

/// System level for system messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemLevel {
    Info,
    Warning,
    Error,
}

/// Get the icon and color for an agent role.
fn role_icon_and_color(role: AgentRole, config: &MessageRenderConfig) -> (&'static str, Color) {
    match role {
        AgentRole::Planner => ("◈", config.primary_color),
        AgentRole::Coder => ("⟠", config.claude_color),
        AgentRole::Tester => ("◉", config.success_color),
        AgentRole::Reviewer => ("◆", config.warning_color),
        AgentRole::Synthesizer => ("⧉", config.primary_color),
        AgentRole::SecurityAuditor => ("⚷", config.error_color),
        AgentRole::Red => ("✗", config.error_color),
    }
}

/// Get the display label for an agent role.
fn role_label(role: AgentRole) -> &'static str {
    match role {
        AgentRole::Planner => "planner",
        AgentRole::Coder => "coder",
        AgentRole::Tester => "tester",
        AgentRole::Reviewer => "reviewer",
        AgentRole::Synthesizer => "synthesizer",
        AgentRole::SecurityAuditor => "security",
        AgentRole::Red => "red",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> MessageRenderConfig {
        MessageRenderConfig {
            width: 80,
            show_timestamps: false,
            role_user_color: Color::Yellow,
            role_assistant_color: Color::Blue,
            role_system_color: Color::Gray,
            text_color: Color::White,
            text_dim_color: Color::Gray,
            border_color: Color::DarkGray,
            success_color: Color::Green,
            warning_color: Color::Yellow,
            error_color: Color::Red,
            claude_color: Color::Magenta,
            primary_color: Color::Cyan,
        }
    }

    #[test]
    fn render_user_message_test() {
        let config = test_config();
        let lines = render_user_message("Hello, NIKI!", &Utc::now(), &config);
        assert!(lines.len() >= 3); // header, separator, content
    }

    #[test]
    fn render_assistant_message_test() {
        let config = test_config();
        let lines = render_assistant_message("Let me help.", AgentRole::Planner, &[], &config);
        assert!(lines.len() >= 3);
    }

    #[test]
    fn render_system_message_info() {
        let config = test_config();
        let lines = render_system_message("Pipeline started", SystemLevel::Info, &config);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn render_tool_call_done() {
        let config = test_config();
        let tool = ToolCallDisplay {
            tool_name: "bash".to_string(),
            status: ToolCallStatus::Done,
            summary: Some("Build succeeded".to_string()),
        };
        let lines = render_tool_call(&tool, &config);
        assert_eq!(lines.len(), 2); // header + summary
    }

    #[test]
    fn render_message_dispatch() {
        let config = test_config();
        let lines = render_message("Hi", MessageRole::User, &[], &config);
        assert!(lines.len() >= 3);

        let lines = render_message(
            "Working",
            MessageRole::Assistant(AgentRole::Coder),
            &[],
            &config,
        );
        assert!(lines.len() >= 3);

        let lines = render_message("Done", MessageRole::System(SystemLevel::Info), &[], &config);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn role_label_test() {
        assert_eq!(role_label(AgentRole::Planner), "planner");
        assert_eq!(role_label(AgentRole::Coder), "coder");
        assert_eq!(role_label(AgentRole::Reviewer), "reviewer");
    }

    #[test]
    fn role_icon_and_color_test() {
        let config = test_config();
        let (icon, _) = role_icon_and_color(AgentRole::Planner, &config);
        assert_eq!(icon, "◈");
        let (icon, _) = role_icon_and_color(AgentRole::Coder, &config);
        assert_eq!(icon, "⟠");
    }
}
