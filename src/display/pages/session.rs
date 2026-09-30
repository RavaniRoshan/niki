//! Session view — investigate and control one mission.

use ratatui::buffer::Buffer;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;

use crate::mission::{Agent, ChatMessage, Mission};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionTab {
    Conversation,
    Agents,
    Tools,
    Diff,
    Tests,
    Approvals,
    Evidence,
}

impl SessionTab {
    pub fn all() -> &'static [SessionTab] {
        &[
            Self::Conversation,
            Self::Agents,
            Self::Tools,
            Self::Diff,
            Self::Tests,
            Self::Approvals,
            Self::Evidence,
        ]
    }
    pub fn title(&self) -> &'static str {
        match self {
            Self::Conversation => "Conversation",
            Self::Agents => "Agents",
            Self::Tools => "Tools",
            Self::Diff => "Diff",
            Self::Tests => "Tests",
            Self::Approvals => "Approvals",
            Self::Evidence => "Evidence",
        }
    }
}

#[derive(Debug)]
pub struct SessionState {
    pub mission: Mission,
    pub agents: Vec<Agent>,
    pub messages: Vec<ChatMessage>,
    pub active_tab: SessionTab,
}

impl SessionState {
    pub fn new(mission: Mission) -> Self {
        Self {
            mission,
            agents: Vec::new(),
            messages: Vec::new(),
            active_tab: SessionTab::Conversation,
        }
    }

    /// Build a session view pre-populated with the mission's agents.
    pub fn with_agents(mission: Mission, agents: Vec<Agent>) -> Self {
        Self {
            mission,
            agents,
            messages: Vec::new(),
            active_tab: SessionTab::Conversation,
        }
    }
    pub fn next_tab(&mut self) {
        let tabs = SessionTab::all();
        let idx = tabs.iter().position(|t| *t == self.active_tab).unwrap_or(0);
        self.active_tab = tabs[(idx + 1) % tabs.len()];
    }
    pub fn prev_tab(&mut self) {
        let tabs = SessionTab::all();
        let idx = tabs.iter().position(|t| *t == self.active_tab).unwrap_or(0);
        self.active_tab = tabs[(idx + tabs.len() - 1) % tabs.len()];
    }
}

/// The Conversation tab reads the **live** chat log.
///
/// It used to read `SessionState::messages`, which nothing in the tree ever
/// writes — the page therefore rendered "No messages yet" on every mission,
/// permanently, while the conversation the user was looking at sat in
/// `AppState::chat_log`. `messages` stays as the mission-scoped store; the
/// tab shows what the chat view is actually showing.
pub fn render_session(
    state: &SessionState,
    chat_log: &[(String, String)],
    area: ratatui::layout::Rect,
    buf: &mut Buffer,
) {
    // Header (standard shape: bold title + dim meta; status word included).
    let header_text = format!(
        " session · {} · {}",
        state.mission.description,
        state.mission.status.status_str()
    );
    let header = Paragraph::new(header_text).style(
        Style::default()
            .fg(crate::display::theme::fg_color())
            .add_modifier(Modifier::BOLD),
    );
    header.render(
        ratatui::layout::Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1,
        },
        buf,
    );

    // Tabs
    let tab_line: Vec<Span> = SessionTab::all()
        .iter()
        .map(|t| {
            let style = if *t == state.active_tab {
                Style::default()
                    .fg(crate::display::theme::border_active())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(crate::display::theme::fg_dim())
            };
            Span::styled(format!(" {} ", t.title()), style)
        })
        .collect();
    let tabs = Paragraph::new(Line::from(tab_line));
    tabs.render(
        ratatui::layout::Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: 1,
        },
        buf,
    );

    // Content area
    let content_area = ratatui::layout::Rect {
        x: area.x,
        y: area.y + 3,
        width: area.width,
        height: area.height.saturating_sub(4),
    };

    match state.active_tab {
        SessionTab::Conversation => render_conversation(chat_log, content_area, buf),
        SessionTab::Agents => render_agents(state, content_area, buf),
        SessionTab::Tools => render_tools(state, content_area, buf),
        _ => {
            let p = Paragraph::new(format!("{} — placeholder", state.active_tab.title()))
                .style(Style::default().fg(crate::display::theme::fg_dim()));
            p.render(content_area, buf);
        }
    }

    // Footer
    let footer = Paragraph::new(" Tab Cycle · ←→ Switch · P Pause/Resume · Esc Back to Fleet")
        .style(Style::default().fg(crate::display::theme::fg_dim()));
    footer.render(
        ratatui::layout::Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(1),
            width: area.width,
            height: 1,
        },
        buf,
    );
}

/// Render the live transcript: `(role, text)` pairs, newest last, one row each.
///
/// Public so a test can drive it without the Session page's mission state —
/// the empty-vs-populated distinction is the whole point of this change.
pub fn render_conversation(
    chat_log: &[(String, String)],
    area: ratatui::layout::Rect,
    buf: &mut Buffer,
) {
    if chat_log.is_empty() {
        let p = Paragraph::new("No messages yet — start from Chat (press Tab).")
            .style(Style::default().fg(crate::display::theme::fg_dim()));
        p.render(area, buf);
        return;
    }
    // The tail, so a long conversation shows its most recent turns rather than
    // the first — the same choice the chat view makes.
    let rows: Vec<(String, String)> = chat_log
        .iter()
        .rev()
        .take(area.height as usize)
        .map(|(r, t)| (r.clone(), t.clone()))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    for (row, (role, text)) in rows.into_iter().enumerate() {
        let y = area.y + row as u16;
        if y >= area.y + area.height {
            break;
        }
        // `AppState::chat_log` roles are the lowercase strings the chat view
        // pushes: "user", "assistant", "system", "error", "notice".
        let (label, color) = match role.as_str() {
            "user" => ("User", crate::display::theme::accent()),
            "assistant" => ("NIKI", crate::display::theme::accent()),
            "system" | "notice" => ("System", crate::display::theme::warning()),
            _ => ("", crate::display::theme::fg_bright()),
        };
        let label_w = label.len() as u16 + 1;
        let max_content = area.width.saturating_sub(label_w + 2) as usize;
        let content = if text.chars().count() > max_content {
            let head: String = text.chars().take(max_content.saturating_sub(1)).collect();
            format!("{head}\u{2026}")
        } else {
            text.clone()
        };
        let mut spans = Vec::new();
        if !label.is_empty() {
            spans.push(Span::styled(
                format!("{label} "),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ));
        }
        spans.push(Span::styled(
            content,
            Style::default().fg(crate::display::theme::fg_bright()),
        ));
        let p = Paragraph::new(Line::from(spans));
        p.render(
            ratatui::layout::Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            },
            buf,
        );
    }
}

fn render_agents(state: &SessionState, area: ratatui::layout::Rect, buf: &mut Buffer) {
    if state.agents.is_empty() {
        let p = Paragraph::new("No agents active — agents appear here once a run starts.")
            .style(Style::default().fg(crate::display::theme::fg_dim()));
        p.render(area, buf);
        return;
    }
    for (row, agent) in state.agents.iter().enumerate() {
        let y = area.y + row as u16;
        if y >= area.y + area.height {
            break;
        }
        let sc = if agent.state.needs_attention() {
            crate::display::theme::warning()
        } else if agent.state.is_active() {
            crate::display::theme::success()
        } else {
            crate::display::theme::fg_dim()
        };
        let line = Line::from(vec![
            Span::styled(format!("{} ", agent.state.icon()), Style::default().fg(sc)),
            Span::styled(
                &agent.role,
                Style::default()
                    .fg(crate::display::theme::fg_bright())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" · {}", agent.state.label()),
                Style::default().fg(sc),
            ),
            Span::styled(
                format!(" · {} tool calls", agent.tool_calls.len()),
                Style::default().fg(crate::display::theme::fg_dim()),
            ),
        ]);
        let p = Paragraph::new(line);
        p.render(
            ratatui::layout::Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            },
            buf,
        );
    }
}

fn render_tools(state: &SessionState, area: ratatui::layout::Rect, buf: &mut Buffer) {
    let mut y = area.y;
    let mut any = false;
    for agent in &state.agents {
        for tc in &agent.tool_calls {
            if y >= area.y + area.height {
                break;
            }
            let tool_status = if tc.success {
                crate::display::components::status::UnifiedStatus::Done
            } else {
                crate::display::components::status::UnifiedStatus::Failed
            };
            let icon = crate::display::components::status::glyph(tool_status);
            let ic = crate::display::components::status::color(tool_status);
            let line = Line::from(vec![
                Span::styled(format!("{} ", icon), Style::default().fg(ic)),
                Span::styled(
                    &tc.tool_name,
                    Style::default()
                        .fg(crate::display::theme::fg_bright())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" · {}", tc.input_summary),
                    Style::default().fg(crate::display::theme::fg_dim()),
                ),
            ]);
            let p = Paragraph::new(line);
            p.render(
                ratatui::layout::Rect {
                    x: area.x,
                    y,
                    width: area.width,
                    height: 1,
                },
                buf,
            );
            y += 1;
            any = true;
        }
    }
    if !any {
        let p = Paragraph::new("No tool calls yet — calls stream here as agents work.")
            .style(Style::default().fg(crate::display::theme::fg_dim()));
        p.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission::MissionId;

    #[test]
    fn session_tab_cycle() {
        let m = Mission::new(MissionId("m1".into()), "t".into(), "s".into());
        let mut s = SessionState::new(m);
        assert_eq!(s.active_tab, SessionTab::Conversation);
        s.next_tab();
        assert_eq!(s.active_tab, SessionTab::Agents);
        s.prev_tab();
        assert_eq!(s.active_tab, SessionTab::Conversation);
        s.prev_tab();
        assert_eq!(s.active_tab, SessionTab::Evidence);
    }
}
