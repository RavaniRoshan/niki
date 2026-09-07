//! Session view — investigate and control one mission.

use ratatui::buffer::Buffer;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;

use crate::mission::{Agent, ChatMessage, ChatRole, Mission};

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

pub fn render_session(state: &SessionState, area: ratatui::layout::Rect, buf: &mut Buffer) {
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
        SessionTab::Conversation => render_conversation(state, content_area, buf),
        SessionTab::Agents => render_agents(state, content_area, buf),
        SessionTab::Tools => render_tools(state, content_area, buf),
        _ => {
            let p = Paragraph::new(format!("{} — placeholder", state.active_tab.title()))
                .style(Style::default().fg(crate::display::theme::fg_dim()));
            p.render(content_area, buf);
        }
    }

    // Footer
    let footer = Paragraph::new(" Tab Cycle · ←→ Switch · Esc Back to Fleet · P Pause · R Resume")
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

#[allow(clippy::explicit_counter_loop)]
fn render_conversation(state: &SessionState, area: ratatui::layout::Rect, buf: &mut Buffer) {
    if state.messages.is_empty() {
        let p = Paragraph::new("No messages yet — start from Chat (press Tab).")
            .style(Style::default().fg(crate::display::theme::fg_dim()));
        p.render(area, buf);
        return;
    }
    let mut y = area.y;
    for msg in &state.messages {
        if y >= area.y + area.height {
            break;
        }
        let (role, color) = match msg.role {
            ChatRole::User => ("User", crate::display::theme::accent()),
            ChatRole::Assistant => ("NIKI", crate::display::theme::accent()),
            ChatRole::System => ("System", crate::display::theme::warning()),
        };
        let role_w = role.len() as u16;
        let max_content = area.width.saturating_sub(role_w + 2);
        let content = if msg.content.len() > max_content as usize {
            format!("{}…", &msg.content[..max_content as usize - 1])
        } else {
            msg.content.clone()
        };
        let line = Line::from(vec![
            Span::styled(
                format!("{} ", role),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                content,
                Style::default().fg(crate::display::theme::fg_bright()),
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
    }
}

#[allow(clippy::explicit_counter_loop)]
fn render_agents(state: &SessionState, area: ratatui::layout::Rect, buf: &mut Buffer) {
    if state.agents.is_empty() {
        let p = Paragraph::new("No agents active — agents appear here once a run starts.")
            .style(Style::default().fg(crate::display::theme::fg_dim()));
        p.render(area, buf);
        return;
    }
    let mut y = area.y;
    for agent in &state.agents {
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
        y += 1;
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
