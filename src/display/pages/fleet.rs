//! Fleet dashboard — mission control for multiple autonomous missions.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::mission::{AttentionPriority, Mission, MissionStatus};

#[derive(Debug)]
pub struct FleetState {
    pub missions: Vec<Mission>,
    pub selected: usize,
}

impl FleetState {
    pub fn new(missions: Vec<Mission>) -> Self {
        Self {
            missions,
            selected: 0,
        }
    }

    pub fn select_next(&mut self) {
        if self.selected + 1 < self.missions.len() {
            self.selected += 1;
        }
    }
    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }
    pub fn select_left(&mut self, cols: usize) {
        self.selected = self.selected.saturating_sub(cols);
    }
    pub fn select_right(&mut self, cols: usize) {
        let new = self.selected + cols;
        if new < self.missions.len() {
            self.selected = new;
        }
    }

    /// Handle a mouse click on the fleet grid, selecting the clicked card.
    pub fn handle_click(&mut self, mouse_col: u16, mouse_row: u16, area: Rect) -> bool {
        if self.missions.is_empty() {
            return false;
        }

        // Header is row 0, cards start at row 2
        if mouse_row < area.y + 2 {
            return false;
        }

        let cols: usize = if area.width >= 80 { 2 } else { 1 };
        let card_w = (area.width / cols as u16).saturating_sub(2);
        let card_h: u16 = 7;

        // Calculate which card was clicked
        let card_row = (mouse_row - area.y - 2) / (card_h + 1);
        let card_col = (mouse_col - area.x) / (card_w + 2);

        let idx = card_row as usize * cols + card_col as usize;
        if idx < self.missions.len() {
            self.selected = idx;
            return true;
        }

        false
    }
}

pub fn render_fleet(fleet: &FleetState, area: ratatui::layout::Rect, buf: &mut Buffer) {
    // Header
    let running = fleet
        .missions
        .iter()
        .filter(|m| m.status == MissionStatus::Running)
        .count();
    let total = fleet.missions.len();
    let total_cost: f64 = fleet.missions.iter().map(|m| m.cost_usd).sum();

    let header_text = format!(
        " NIKI / FLEET  {} missions · {} active · ${:.2}",
        total, running, total_cost
    );
    let header = Paragraph::new(header_text).style(
        Style::default()
            .fg(Color::White)
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

    // Mission grid (2 columns)
    if fleet.missions.is_empty() {
        let empty = Paragraph::new("No missions. Start one from Chat (press Tab).")
            .style(Style::default().fg(Color::DarkGray));
        empty.render(
            ratatui::layout::Rect {
                x: area.x + 2,
                y: area.y + 2,
                width: area.width.saturating_sub(4),
                height: 1,
            },
            buf,
        );
        return;
    }

    let cols: usize = if area.width >= 80 { 2 } else { 1 };
    let card_w = (area.width / cols as u16).saturating_sub(2);
    let card_h: u16 = 7;

    for (i, mission) in fleet.missions.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let x = area.x + col as u16 * (card_w + 2);
        let y = area.y + 2 + row as u16 * (card_h + 1);
        if y + card_h > area.y + area.height.saturating_sub(1) {
            break;
        }

        let card_area = ratatui::layout::Rect {
            x,
            y,
            width: card_w,
            height: card_h,
        };
        let is_selected = i == fleet.selected;
        render_mission_card(mission, card_area, buf, is_selected);
    }

    // Footer
    let footer = Paragraph::new(
        " ↑↓ Navigate · Enter Open · P Pause · R Resume · K Kill · V Diff · Esc Back",
    )
    .style(Style::default().fg(Color::DarkGray));
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

fn render_mission_card(
    mission: &Mission,
    area: ratatui::layout::Rect,
    buf: &mut Buffer,
    selected: bool,
) {
    let border_style = if selected {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let status_color = match mission.status {
        MissionStatus::Running => Color::Green,
        MissionStatus::Paused => Color::Yellow,
        MissionStatus::Completed => Color::DarkGray,
        MissionStatus::Failed => Color::Red,
        _ => Color::DarkGray,
    };

    let status_icon = match mission.status {
        MissionStatus::Running => "●",
        MissionStatus::Paused => "●",
        MissionStatus::Completed => "✓",
        MissionStatus::Failed => "✗",
        _ => "○",
    };

    let attention = match mission.attention {
        AttentionPriority::Normal => "",
        AttentionPriority::Waiting => " ◷",
        AttentionPriority::NeedsAttention => " !",
        AttentionPriority::Error => " x",
    };

    let elapsed = format_duration(mission.elapsed());
    let name_w = (area.width as usize).saturating_sub(6);
    let name = if mission.description.len() > name_w {
        format!("{}…", &mission.description[..name_w.saturating_sub(1)])
    } else {
        mission.description.clone()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(&name, Style::default().fg(Color::White)));

    let lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{}{}", status_icon, attention),
                Style::default().fg(status_color),
            ),
            Span::raw(" "),
            Span::styled(
                mission.status.status_str(),
                Style::default().fg(status_color),
            ),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            format!("{} agents", mission.sessions.len()),
            Style::default().fg(Color::DarkGray),
        )]),
        Line::from(vec![Span::styled(
            format!(
                "{}% · ${:.2} · {}",
                (mission.progress * 100.0) as u16,
                mission.cost_usd,
                elapsed
            ),
            Style::default().fg(Color::DarkGray),
        )]),
    ];

    let paragraph = Paragraph::new(lines).block(block);
    paragraph.render(area, buf);
}

fn format_duration(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {:02}s", secs / 60, secs % 60)
    } else {
        format!("{}h {:02}m", secs / 3600, (secs % 3600) / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission::MissionId;

    #[test]
    fn fleet_nav() {
        let missions = vec![
            Mission::new(MissionId("m1".into()), "a".into(), "s".into()),
            Mission::new(MissionId("m2".into()), "b".into(), "s".into()),
        ];
        let mut f = FleetState::new(missions);
        assert_eq!(f.selected, 0);
        f.select_next();
        assert_eq!(f.selected, 1);
        f.select_next();
        assert_eq!(f.selected, 1);
        f.select_prev();
        assert_eq!(f.selected, 0);
    }
}
