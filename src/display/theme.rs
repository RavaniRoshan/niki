//! Centralized color palette and style constants for the NIKI TUI.
//!
//! All pages import from here instead of defining their own constants.
//! Palette derived from the reference "home screen" aesthetic: deep charcoal
//! background, selective borders, muted accents.

use ratatui::style::{Color, Modifier, Style};

// ── Background & surface ────────────────────────────────────────────────
pub const BG: Color = Color::Rgb(13, 13, 13);
pub const BG_ELEVATED: Color = Color::Rgb(22, 22, 22);
pub const BG_HIGHLIGHT: Color = Color::Rgb(33, 33, 33);

// ── Borders ─────────────────────────────────────────────────────────────
pub const BORDER: Color = Color::Rgb(58, 58, 58);
pub const BORDER_ACTIVE: Color = Color::Rgb(177, 185, 249);
pub const BORDER_DIM: Color = Color::Rgb(33, 33, 33);

// ── Foreground ──────────────────────────────────────────────────────────
pub const FG: Color = Color::Rgb(204, 204, 204);
pub const FG_DIM: Color = Color::Rgb(102, 102, 102);
pub const FG_BRIGHT: Color = Color::Rgb(255, 255, 255);

// ── Accents ─────────────────────────────────────────────────────────────
pub const GREEN: Color = Color::Rgb(78, 186, 101);
pub const BLUE: Color = Color::Rgb(177, 185, 249);
pub const RED: Color = Color::Rgb(255, 107, 128);
pub const AMBER: Color = Color::Rgb(255, 193, 7);
pub const CYAN: Color = Color::Rgb(129, 200, 190);
pub const PURPLE: Color = Color::Rgb(198, 160, 246);

// ── Diff ────────────────────────────────────────────────────────────────
pub const DIFF_ADD_BG: Color = Color::Rgb(34, 92, 43);
pub const DIFF_DEL_BG: Color = Color::Rgb(122, 41, 54);

// ── Agent role colors ───────────────────────────────────────────────────
pub fn role_color(role: crate::artifacts::types::AgentRole) -> Color {
    match role {
        crate::artifacts::types::AgentRole::Planner => BLUE,
        crate::artifacts::types::AgentRole::Coder => PURPLE,
        crate::artifacts::types::AgentRole::Tester => GREEN,
        crate::artifacts::types::AgentRole::Reviewer => AMBER,
        crate::artifacts::types::AgentRole::Synthesizer => CYAN,
        crate::artifacts::types::AgentRole::SecurityAuditor => RED,
        crate::artifacts::types::AgentRole::Red => Color::Rgb(255, 99, 132),
    }
}

pub fn role_glyph(role: crate::artifacts::types::AgentRole) -> &'static str {
    match role {
        crate::artifacts::types::AgentRole::Planner => "◆",
        crate::artifacts::types::AgentRole::Coder => "⚡",
        crate::artifacts::types::AgentRole::Tester => "●",
        crate::artifacts::types::AgentRole::Reviewer => "◆",
        crate::artifacts::types::AgentRole::Synthesizer => "⧉",
        crate::artifacts::types::AgentRole::SecurityAuditor => "⚷",
        crate::artifacts::types::AgentRole::Red => "✗",
    }
}

pub fn role_name(role: crate::artifacts::types::AgentRole) -> &'static str {
    match role {
        crate::artifacts::types::AgentRole::Planner => "Planner",
        crate::artifacts::types::AgentRole::Coder => "Coder",
        crate::artifacts::types::AgentRole::Tester => "Tester",
        crate::artifacts::types::AgentRole::Reviewer => "Reviewer",
        crate::artifacts::types::AgentRole::Synthesizer => "Synthesizer",
        crate::artifacts::types::AgentRole::SecurityAuditor => "Security",
        crate::artifacts::types::AgentRole::Red => "Red",
    }
}

// ── Compound styles ─────────────────────────────────────────────────────
pub fn header_style() -> Style {
    Style::default().fg(FG_BRIGHT).add_modifier(Modifier::BOLD)
}

pub fn dim_style() -> Style {
    Style::default().fg(FG_DIM)
}

pub fn accent_style(color: Color) -> Style {
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

pub fn status_ok() -> Style {
    Style::default().fg(GREEN).add_modifier(Modifier::BOLD)
}

pub fn status_err() -> Style {
    Style::default().fg(RED).add_modifier(Modifier::BOLD)
}

pub fn status_warn() -> Style {
    Style::default().fg(AMBER).add_modifier(Modifier::BOLD)
}

pub fn status_running(color: Color) -> Style {
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

pub fn footer_style() -> Style {
    Style::default().fg(FG_DIM)
}

pub fn block_border() -> Style {
    Style::default().fg(BORDER)
}

pub fn block_border_active() -> Style {
    Style::default().fg(BORDER_ACTIVE)
}

// ── Legacy Theme struct (used by CLI output: agent_stream, pipeline_status, completion, banner) ──

use console::Style as ConsoleStyle;

#[derive(Clone)]
pub struct Theme {
    pub planner: AgentTheme,
    pub coder: AgentTheme,
    pub tester: AgentTheme,
    pub reviewer: AgentTheme,
    pub synthesizer: AgentTheme,
    pub security_auditor: AgentTheme,
    pub red: AgentTheme,
    pub border: ConsoleStyle,
    pub heading: ConsoleStyle,
    pub subtext: ConsoleStyle,
    pub success: ConsoleStyle,
    pub warning: ConsoleStyle,
    pub error: ConsoleStyle,
    pub diff_add: ConsoleStyle,
    pub diff_remove: ConsoleStyle,
    pub file_path: ConsoleStyle,
}

#[derive(Clone)]
pub struct AgentTheme {
    pub name: &'static str,
    pub icon: &'static str,
    pub color: ConsoleStyle,
    pub label_style: ConsoleStyle,
}

impl Default for Theme {
    fn default() -> Self {
        Self::new()
    }
}

impl Theme {
    pub fn new() -> Self {
        Theme {
            planner: AgentTheme {
                name: "Planner",
                icon: "◈",
                color: ConsoleStyle::new().blue(),
                label_style: ConsoleStyle::new().bold().blue(),
            },
            coder: AgentTheme {
                name: "Coder",
                icon: "⟠",
                color: ConsoleStyle::new().magenta(),
                label_style: ConsoleStyle::new().bold().magenta(),
            },
            tester: AgentTheme {
                name: "Tester",
                icon: "◉",
                color: ConsoleStyle::new().green(),
                label_style: ConsoleStyle::new().bold().green(),
            },
            reviewer: AgentTheme {
                name: "Reviewer",
                icon: "◆",
                color: ConsoleStyle::new().yellow(),
                label_style: ConsoleStyle::new().bold().yellow(),
            },
            synthesizer: AgentTheme {
                name: "Synthesizer",
                icon: "⧉",
                color: ConsoleStyle::new().cyan(),
                label_style: ConsoleStyle::new().bold().cyan(),
            },
            security_auditor: AgentTheme {
                name: "Security Auditor",
                icon: "⚷",
                color: ConsoleStyle::new().red(),
                label_style: ConsoleStyle::new().bold().red(),
            },
            red: AgentTheme {
                name: "Red",
                icon: "✗",
                color: ConsoleStyle::new().red(),
                label_style: ConsoleStyle::new().bold().red(),
            },
            border: ConsoleStyle::new().dim(),
            heading: ConsoleStyle::new().bold().white(),
            subtext: ConsoleStyle::new().dim(),
            success: ConsoleStyle::new().green(),
            warning: ConsoleStyle::new().yellow(),
            error: ConsoleStyle::new().red(),
            diff_add: ConsoleStyle::new().on_green().black(),
            diff_remove: ConsoleStyle::new().on_red().black(),
            file_path: ConsoleStyle::new().cyan().underlined(),
        }
    }
}
