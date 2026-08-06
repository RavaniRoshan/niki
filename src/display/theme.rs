//! Centralized color palette and style constants for the NIKI TUI.
//!
//! Dual-theme system aligned to product-site design tokens.
//! All pages import from here instead of defining their own constants.
//!
//! Architecture: `ThemeMode` enum + `Palette` struct + `static MODE: AtomicU8`
//! + mode-aware accessors. NO thread_local — CLI threads share mode; `Color` is Copy.

use std::sync::atomic::{AtomicU8, Ordering};

use ratatui::style::{Color, Modifier, Style};

// ── Theme mode ──────────────────────────────────────────────────────────

/// Theme mode: Auto (detect from terminal), Dark, or Light.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ThemeMode {
    Auto = 0,
    Dark = 1,
    Light = 2,
}

impl ThemeMode {
    /// Parse from a lowercase string (for config deserialization).
    pub fn from_str(s: &str) -> Self {
        match s {
            "dark" => ThemeMode::Dark,
            "light" => ThemeMode::Light,
            _ => ThemeMode::Auto,
        }
    }

    /// Serialize to a lowercase string.
    pub fn as_str(&self) -> &'static str {
        match self {
            ThemeMode::Auto => "auto",
            ThemeMode::Dark => "dark",
            ThemeMode::Light => "light",
        }
    }
}

/// Global theme mode — atomic so CLI threads see the same mode.
static MODE: AtomicU8 = AtomicU8::new(ThemeMode::Dark as u8);

/// Set the global theme mode.
pub fn set_mode(mode: ThemeMode) {
    MODE.store(mode as u8, Ordering::Relaxed);
}

/// Get the effective theme mode (resolves Auto → Dark as default fallback).
pub fn current_mode() -> ThemeMode {
    match MODE.load(Ordering::Relaxed) {
        0 => ThemeMode::Auto, // resolved via auto-detect at startup
        1 => ThemeMode::Dark,
        2 => ThemeMode::Light,
        _ => ThemeMode::Dark,
    }
}

/// Check if the effective mode is light.
pub fn is_light() -> bool {
    current_mode() == ThemeMode::Light
}

// ── NO_COLOR detection ──────────────────────────────────────────────────

/// Returns true if the user has requested no color output.
/// Respects the NO_COLOR convention (https://no-color.org/).
pub fn no_color() -> bool {
    std::env::var("NO_COLOR").is_ok()
}

/// Returns the appropriate foreground color, respecting NO_COLOR.
fn fg(color: Color) -> Color {
    if no_color() {
        Color::Reset
    } else {
        color
    }
}

/// Returns the appropriate background color, respecting NO_COLOR.
/// This fixes the bg leak: under NO_COLOR, background must also be Reset.
fn bg(color: Color) -> Color {
    if no_color() {
        Color::Reset
    } else {
        color
    }
}

// ── Palette ─────────────────────────────────────────────────────────────

/// A complete color palette for one theme variant.
pub struct Palette {
    // Backgrounds
    pub bg: Color,
    pub bg_deep: Color,
    pub bg_elevated: Color,
    pub bg_highlight: Color,

    // Borders
    pub border: Color,
    pub border_active: Color,
    pub border_dim: Color,

    // Foregrounds
    pub fg: Color,
    pub fg_dim: Color,
    pub fg_bright: Color,
    pub fg_subtle: Color,

    // Accents (per-theme, darkened for light bg)
    pub success: Color,
    pub error: Color,
    pub warning: Color,
    pub accent: Color,
    pub clay_orange: Color,
    pub cyan: Color,
    pub purple: Color,

    // Selection & diff
    pub selection_bg: Color,
    pub diff_add_bg: Color,
    pub diff_del_bg: Color,
    pub diff_add_fg: Color,
    pub diff_del_fg: Color,
    pub diff_hunk: Color,

    // Agent role colors (darkened for light bg)
    pub agent_red: Color,
    pub agent_blue: Color,
    pub agent_green: Color,
    pub agent_yellow: Color,
    pub agent_purple: Color,
    pub agent_orange: Color,
    pub agent_pink: Color,
    pub agent_cyan: Color,
}

/// Dark palette — Midnight Teal (#0d1117 base, teal primary, amber accent).
pub const DARK: Palette = Palette {
    bg: Color::Rgb(0x0d, 0x11, 0x17),         // #0d1117
    bg_deep: Color::Rgb(0x01, 0x04, 0x09),    // #010409
    bg_elevated: Color::Rgb(0x16, 0x1b, 0x22), // #161b22
    bg_highlight: Color::Rgb(0x1c, 0x21, 0x28), // #1c2128

    border: Color::Rgb(0x30, 0x36, 0x3d),      // #30363d
    border_active: Color::Rgb(0x0d, 0x94, 0x88), // #0d9488 (teal)
    border_dim: Color::Rgb(0x21, 0x26, 0x2d),  // #21262d

    fg: Color::Rgb(0xe6, 0xed, 0xf3),          // #e6edf3
    fg_dim: Color::Rgb(0x8b, 0x94, 0x9e),      // #8b949e
    fg_bright: Color::Rgb(0xf0, 0xf6, 0xfc),    // #f0f6fc
    fg_subtle: Color::Rgb(0x6e, 0x76, 0x81),   // #6e7681

    success: Color::Rgb(0x34, 0xd3, 0x99),     // #34d399
    error: Color::Rgb(0xf8, 0x71, 0x71),       // #f87171
    warning: Color::Rgb(0xfb, 0xbf, 0x24),     // #fbbf24
    accent: Color::Rgb(0x0d, 0x94, 0x88),      // #0d9488 (teal)
    clay_orange: Color::Rgb(0xf5, 0x9e, 0x0b), // #f59e0b (amber)
    cyan: Color::Rgb(0x22, 0xd3, 0xee),        // #22d3ee
    purple: Color::Rgb(0xa7, 0x8b, 0xfa),      // #a78bfa

    selection_bg: Color::Rgb(0x0d, 0x94, 0x88), // #0d9488 (teal)
    diff_add_bg: Color::Rgb(0x06, 0x4e, 0x3b), // #064e3b
    diff_del_bg: Color::Rgb(0x6b, 0x1d, 0x20), // #6b1d20
    diff_add_fg: Color::Rgb(0x34, 0xd3, 0x99), // #34d399
    diff_del_fg: Color::Rgb(0xf8, 0x71, 0x71), // #f87171
    diff_hunk: Color::Rgb(0xfb, 0xbf, 0x24),   // #fbbf24

    agent_red: Color::Rgb(0xff, 0x6b, 0x6b),   // #ff6b6b (coral red)
    agent_blue: Color::Rgb(0x38, 0xbd, 0xf8),  // #38bdf8
    agent_green: Color::Rgb(0x34, 0xd3, 0x99),  // #34d399
    agent_yellow: Color::Rgb(0xf5, 0x9e, 0x0b), // #f59e0b (amber)
    agent_purple: Color::Rgb(0xa7, 0x8b, 0xfa), // #a78bfa
    agent_orange: Color::Rgb(0xfb, 0x92, 0x3c), // #fb923c
    agent_pink: Color::Rgb(0xf4, 0x72, 0xb6),   // #f472b6
    agent_cyan: Color::Rgb(0x22, 0xd3, 0xee),   // #22d3ee
};

/// Light palette — Midnight Teal light (#f8fafc base, teal primary, amber accent).
/// Accents darkened for sufficient contrast on light background.
pub const LIGHT: Palette = Palette {
    bg: Color::Rgb(0xf8, 0xfa, 0xfc),          // #f8fafc
    bg_deep: Color::Rgb(0x0f, 0x17, 0x2a),     // #0f172a
    bg_elevated: Color::Rgb(0xff, 0xff, 0xff), // #ffffff
    bg_highlight: Color::Rgb(0xf1, 0xf5, 0xf9), // #f1f5f9

    border: Color::Rgb(0xcb, 0xd5, 0xe1),       // #cbd5e1
    border_active: Color::Rgb(0x0f, 0x76, 0x6e), // #0f766e (teal)
    border_dim: Color::Rgb(0xe2, 0xe8, 0xf0),   // #e2e8f0

    fg: Color::Rgb(0x1e, 0x29, 0x3b),          // #1e293b
    fg_dim: Color::Rgb(0x64, 0x74, 0x8b),      // #64748b
    fg_bright: Color::Rgb(0x0f, 0x17, 0x2a),   // #0f172a
    fg_subtle: Color::Rgb(0x94, 0xa3, 0xb8),   // #94a3b8

    success: Color::Rgb(0x05, 0x96, 0x69),     // #059669 (darkened for light)
    error: Color::Rgb(0xdc, 0x26, 0x28),       // #dc2628 (darkened for light)
    warning: Color::Rgb(0xd9, 0x77, 0x06),     // #d97706 (amber)
    accent: Color::Rgb(0x0f, 0x76, 0x6e),      // #0f766e (teal)
    clay_orange: Color::Rgb(0xd9, 0x77, 0x06), // #d97706 (amber)
    cyan: Color::Rgb(0x08, 0x91, 0xb2),        // #0891b2
    purple: Color::Rgb(0x7c, 0x3a, 0xed),      // #7c3aed

    selection_bg: Color::Rgb(0xcc, 0xf7, 0xf0), // #ccf7f0 (teal 8% tint)
    diff_add_bg: Color::Rgb(0xec, 0xfd, 0xf5), // #ecfdf5 (success 8% tint)
    diff_del_bg: Color::Rgb(0xfe, 0xf2, 0xf2), // #fef2f2 (error 8% tint)
    diff_add_fg: Color::Rgb(0x05, 0x96, 0x69), // #059669
    diff_del_fg: Color::Rgb(0xdc, 0x26, 0x28), // #dc2628
    diff_hunk: Color::Rgb(0xd9, 0x77, 0x06),   // #d97706

    agent_red: Color::Rgb(0xef, 0x44, 0x44),   // #ef4444 (coral red)
    agent_blue: Color::Rgb(0x08, 0x91, 0xb2),  // #0891b2
    agent_green: Color::Rgb(0x05, 0x96, 0x69),  // #059669
    agent_yellow: Color::Rgb(0xd9, 0x77, 0x06), // #d97706 (amber)
    agent_purple: Color::Rgb(0x7c, 0x3a, 0xed), // #7c3aed
    agent_orange: Color::Rgb(0xc2, 0x41, 0x0c), // #c2410c
    agent_pink: Color::Rgb(0xbe, 0x18, 0x5d),   // #be185d
    agent_cyan: Color::Rgb(0x0e, 0x74, 0x90),   // #0e7490
};

/// Get the current palette based on active theme mode.
#[inline]
fn palette() -> &'static Palette {
    match current_mode() {
        ThemeMode::Light => &LIGHT,
        _ => &DARK, // Dark + Auto fallback
    }
}

// ── New chat interface tokens (Kimi Code palette alignment) ─────────────

/// Primary color — links, inline code, focused elements (alias for accent).
#[inline] pub fn primary() -> Color { fg(palette().accent) }

/// Claude brand accent — spinners, logo (alias for purple).
#[inline] pub fn claude() -> Color { fg(palette().purple) }

/// Shell mode border/prompt color.
#[inline] pub fn shell() -> Color { fg(palette().purple) }

/// User message bullet color (gold).
#[inline] pub fn role_user() -> Color { fg(palette().agent_yellow) }

/// Assistant message label color.
#[inline] pub fn role_assistant() -> Color { fg(palette().accent) }

/// System message color.
#[inline] pub fn role_system() -> Color { fg(palette().fg_dim) }

/// Prompt cursor style (reversed foreground).
#[inline] pub fn prompt_cursor() -> Style {
    Style::default().fg(palette().bg).bg(palette().accent)
}

/// Input box border color.
#[inline] pub fn prompt_border() -> Color {
    fg(palette().border_active)
}

/// Dim text (alias for fg_dim).
#[inline] pub fn text_dim() -> Color { fg(palette().fg_dim) }

// ── Mode-aware accessors (the 11 core + 6 accent surfaces) ──────────────

#[inline] pub fn bg_color() -> Color { bg(palette().bg) }
#[inline] pub fn bg_deep() -> Color { bg(palette().bg_deep) }
#[inline] pub fn bg_elevated() -> Color { bg(palette().bg_elevated) }
#[inline] pub fn bg_highlight() -> Color { bg(palette().bg_highlight) }
#[inline] pub fn border_color() -> Color { fg(palette().border) }
#[inline] pub fn border_active() -> Color { fg(palette().border_active) }
#[inline] pub fn border_dim() -> Color { fg(palette().border_dim) }
#[inline] pub fn fg_color() -> Color { fg(palette().fg) }
#[inline] pub fn fg_dim() -> Color { fg(palette().fg_dim) }
#[inline] pub fn fg_bright() -> Color { fg(palette().fg_bright) }
#[inline] pub fn fg_subtle() -> Color { fg(palette().fg_subtle) }

#[inline] pub fn success() -> Color { fg(palette().success) }
#[inline] pub fn error() -> Color { fg(palette().error) }
#[inline] pub fn warning() -> Color { fg(palette().warning) }
#[inline] pub fn accent() -> Color { fg(palette().accent) }
#[inline] pub fn selection_bg() -> Color { palette().selection_bg }
#[inline] pub fn diff_add_bg() -> Color { palette().diff_add_bg }
#[inline] pub fn diff_del_bg() -> Color { palette().diff_del_bg }

// ── Product semantic aliases ────────────────────────────────────────────

/// Primary text color (alias for fg_color).
#[inline] pub fn text() -> Color { fg_color() }
/// Body text (alias for fg_color).
#[inline] pub fn text_body() -> Color { fg_color() }
/// Muted text (alias for fg_dim).
#[inline] pub fn text_muted() -> Color { fg_dim() }
/// Default border (alias for border_color).
#[inline] pub fn border() -> Color { border_color() }
/// Strong/focus border (alias for border_active).
#[inline] pub fn border_strong() -> Color { border_active() }
/// Surface background (alias for bg_color).
#[inline] pub fn surface() -> Color { bg_color() }
/// Soft surface (alias for bg_highlight).
#[inline] pub fn surface_soft() -> Color { bg_highlight() }
/// Card/elevated surface (alias for bg_elevated).
#[inline] pub fn surface_card() -> Color { bg_elevated() }
/// Deep ink (alias for bg_deep).
#[inline] pub fn ink_deep() -> Color { bg_deep() }
/// Ash/subtle text (alias for fg_subtle).
#[inline] pub fn ash() -> Color { fg_subtle() }
/// Charcoal text (alias for fg_dim).
#[inline] pub fn charcoal() -> Color { fg_dim() }
/// Stone text (alias for fg_subtle).
#[inline] pub fn stone() -> Color { fg_subtle() }
/// Text on dark backgrounds — returns fg_bright.
#[inline] pub fn on_dark() -> Color { fg_bright() }
/// Dimmed surface for modal scrim overlays.
#[inline] pub fn surface_dark() -> Color { bg_deep() }

// ── Backward-compat aliases (old const names → new palette fns) ─────────
// Mechanical sweep converts theme::OLD → theme::OLD() across all files.
// These will be removed once all sites use the semantic accessors directly.
#[allow(non_snake_case)]

/// Background (backward-compat).
#[allow(non_snake_case)]
pub fn BG() -> Color { bg_color() }
/// Deeper background (backward-compat).
#[allow(non_snake_case)]
pub fn BG_DEEP() -> Color { bg_deep() }
/// Elevated surface (backward-compat).
#[allow(non_snake_case)]
pub fn BG_ELEVATED() -> Color { bg_elevated() }
/// Highlighted surface (backward-compat).
#[allow(non_snake_case)]
pub fn BG_HIGHLIGHT() -> Color { bg_highlight() }
/// Default border (backward-compat).
#[allow(non_snake_case)]
pub fn BORDER() -> Color { border_color() }
/// Active border (backward-compat).
#[allow(non_snake_case)]
pub fn BORDER_ACTIVE() -> Color { border_active() }
/// Dim border (backward-compat).
#[allow(non_snake_case)]
pub fn BORDER_DIM() -> Color { border_dim() }
/// Primary text (backward-compat).
#[allow(non_snake_case)]
pub fn FG() -> Color { fg_color() }
/// Dimmed text (backward-compat).
#[allow(non_snake_case)]
pub fn FG_DIM() -> Color { fg_dim() }
/// Bright text (backward-compat).
#[allow(non_snake_case)]
pub fn FG_BRIGHT() -> Color { fg_bright() }
/// Subtle text (backward-compat).
#[allow(non_snake_case)]
pub fn FG_SUBTLE() -> Color { fg_subtle() }

// Accent aliases
#[allow(non_snake_case)]
pub fn GREEN() -> Color { palette().success }
#[allow(non_snake_case)]
pub fn RED() -> Color { palette().error }
#[allow(non_snake_case)]
pub fn AMBER() -> Color { palette().warning }
#[allow(non_snake_case)]
pub fn BLUE() -> Color { palette().agent_blue }
#[allow(non_snake_case)]
pub fn PURPLE() -> Color { palette().purple }
#[allow(non_snake_case)]
pub fn CYAN() -> Color { palette().cyan }
#[allow(non_snake_case)]
pub fn CLAY_ORANGE() -> Color { palette().clay_orange }
#[allow(non_snake_case)]
pub fn SELECTION_BG() -> Color { palette().selection_bg }

// Diff aliases
#[allow(non_snake_case)]
pub fn DIFF_ADD_BG() -> Color { palette().diff_add_bg }
#[allow(non_snake_case)]
pub fn DIFF_DEL_BG() -> Color { palette().diff_del_bg }
#[allow(non_snake_case)]
pub fn DIFF_ADD_FG() -> Color { palette().diff_add_fg }
#[allow(non_snake_case)]
pub fn DIFF_DEL_FG() -> Color { palette().diff_del_fg }
#[allow(non_snake_case)]
pub fn DIFF_WORD_ADD() -> Color { palette().diff_add_fg }
#[allow(non_snake_case)]
pub fn DIFF_WORD_DEL() -> Color { palette().diff_del_fg }
#[allow(non_snake_case)]
pub fn DIFF_HUNK() -> Color { palette().diff_hunk }

// Agent color aliases
#[allow(non_snake_case)]
pub fn AGENT_RED() -> Color { palette().agent_red }
#[allow(non_snake_case)]
pub fn AGENT_BLUE() -> Color { palette().agent_blue }
#[allow(non_snake_case)]
pub fn AGENT_GREEN() -> Color { palette().agent_green }
#[allow(non_snake_case)]
pub fn AGENT_YELLOW() -> Color { palette().agent_yellow }
#[allow(non_snake_case)]
pub fn AGENT_PURPLE() -> Color { palette().agent_purple }
#[allow(non_snake_case)]
pub fn AGENT_ORANGE() -> Color { palette().agent_orange }
#[allow(non_snake_case)]
pub fn AGENT_PINK() -> Color { palette().agent_pink }
#[allow(non_snake_case)]
pub fn AGENT_CYAN() -> Color { palette().agent_cyan }

// ── Role colors ─────────────────────────────────────────────────────────

pub fn role_color(role: crate::artifacts::types::AgentRole) -> Color {
    if no_color() {
        return Color::Reset;
    }
    let p = palette();
    match role {
        crate::artifacts::types::AgentRole::Planner => p.agent_blue,
        crate::artifacts::types::AgentRole::Coder => p.purple,
        crate::artifacts::types::AgentRole::Tester => p.agent_green,
        crate::artifacts::types::AgentRole::Reviewer => p.warning,
        crate::artifacts::types::AgentRole::Synthesizer => p.cyan,
        crate::artifacts::types::AgentRole::SecurityAuditor => p.error,
        crate::artifacts::types::AgentRole::Red => p.agent_red,
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
    Style::default().fg(fg(palette().fg_bright)).add_modifier(Modifier::BOLD)
}

pub fn dim_style() -> Style {
    Style::default().fg(fg(palette().fg_dim))
}

pub fn accent_style(color: Color) -> Style {
    Style::default().fg(fg(color)).add_modifier(Modifier::BOLD)
}

pub fn status_ok() -> Style {
    Style::default().fg(fg(palette().success)).add_modifier(Modifier::BOLD)
}

pub fn status_err() -> Style {
    Style::default().fg(fg(palette().error)).add_modifier(Modifier::BOLD)
}

pub fn status_warn() -> Style {
    Style::default().fg(fg(palette().warning)).add_modifier(Modifier::BOLD)
}

pub fn status_running(color: Color) -> Style {
    Style::default().fg(fg(color)).add_modifier(Modifier::BOLD)
}

pub fn footer_style() -> Style {
    Style::default().fg(fg(palette().fg_dim))
}

pub fn block_border() -> Style {
    Style::default().fg(fg(palette().border))
}

pub fn block_border_active() -> Style {
    Style::default().fg(fg(palette().border_active))
}

/// Clay orange accent style — the signature brand look.
pub fn clay_accent() -> Style {
    Style::default().fg(fg(palette().clay_orange)).add_modifier(Modifier::BOLD)
}

// ── Unicode-aware text utilities ────────────────────────────────────────

/// Truncate a string to fit within `max_width` terminal cells.
pub fn truncate_str(s: &str, max_width: usize) -> String {
    use unicode_truncate::UnicodeTruncateStr;
    let (truncated, _) = s.unicode_truncate(max_width);
    truncated.to_string()
}

/// Truncate a string with an ellipsis indicator when truncated.
pub fn truncate_str_ellipsis(s: &str, max_width: usize) -> String {
    use unicode_truncate::UnicodeTruncateStr;
    if max_width < 4 {
        let (truncated, _) = s.unicode_truncate(max_width);
        return truncated.to_string();
    }
    let (truncated, _) = s.unicode_truncate(max_width - 3);
    if truncated.len() == s.len() {
        s.to_string()
    } else {
        format!("{}...", truncated)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_palettes_defined() {
        // Both palettes must exist and have distinct bg colors
        assert_ne!(format!("{:?}", DARK.bg), format!("{:?}", LIGHT.bg));
        assert_ne!(format!("{:?}", DARK.fg), format!("{:?}", LIGHT.fg));
    }

    #[test]
    fn theme_mode_roundtrip() {
        assert_eq!(ThemeMode::from_str("dark"), ThemeMode::Dark);
        assert_eq!(ThemeMode::from_str("light"), ThemeMode::Light);
        assert_eq!(ThemeMode::from_str("auto"), ThemeMode::Auto);
        assert_eq!(ThemeMode::from_str("unknown"), ThemeMode::Auto);
        assert_eq!(ThemeMode::Dark.as_str(), "dark");
        assert_eq!(ThemeMode::Light.as_str(), "light");
        assert_eq!(ThemeMode::Auto.as_str(), "auto");
    }

    #[test]
    fn set_mode_and_current() {
        let original = current_mode();
        set_mode(ThemeMode::Light);
        assert_eq!(current_mode(), ThemeMode::Light);
        set_mode(ThemeMode::Dark);
        assert_eq!(current_mode(), ThemeMode::Dark);
        // Restore
        set_mode(original);
    }

    #[test]
    fn accessors_return_distinct_values_per_mode() {
        let original = current_mode();

        set_mode(ThemeMode::Dark);
        let dark_bg = bg_color();
        let dark_fg = fg_color();

        set_mode(ThemeMode::Light);
        let light_bg = bg_color();
        let light_fg = fg_color();

        assert_ne!(format!("{:?}", dark_bg), format!("{:?}", light_bg),
            "bg must differ between dark and light");
        assert_ne!(format!("{:?}", dark_fg), format!("{:?}", light_fg),
            "fg must differ between dark and light");

        set_mode(original);
    }

    #[test]
    fn semantic_aliases_match_core() {
        assert_eq!(format!("{:?}", text()), format!("{:?}", fg_color()));
        assert_eq!(format!("{:?}", text_body()), format!("{:?}", fg_color()));
        assert_eq!(format!("{:?}", text_muted()), format!("{:?}", fg_dim()));
        assert_eq!(format!("{:?}", border()), format!("{:?}", border_color()));
        assert_eq!(format!("{:?}", border_strong()), format!("{:?}", border_active()));
        assert_eq!(format!("{:?}", surface()), format!("{:?}", bg_color()));
        assert_eq!(format!("{:?}", surface_soft()), format!("{:?}", bg_highlight()));
        assert_eq!(format!("{:?}", surface_card()), format!("{:?}", bg_elevated()));
        assert_eq!(format!("{:?}", ink_deep()), format!("{:?}", bg_deep()));
    }

    #[test]
    fn no_color_env_respected() {
        let _ = no_color();
    }

    #[test]
    fn role_colors_are_distinct() {
        let roles = [
            crate::artifacts::types::AgentRole::Planner,
            crate::artifacts::types::AgentRole::Coder,
            crate::artifacts::types::AgentRole::Tester,
            crate::artifacts::types::AgentRole::Reviewer,
            crate::artifacts::types::AgentRole::Synthesizer,
            crate::artifacts::types::AgentRole::SecurityAuditor,
            crate::artifacts::types::AgentRole::Red,
        ];
        let mut colors = std::collections::HashSet::new();
        for role in &roles {
            colors.insert(format!("{:?}", role_color(*role)));
        }
        assert_eq!(colors.len(), 7, "All role colors should be distinct");
    }

    #[test]
    fn truncate_str_basic() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world", 5), "hello");
        assert_eq!(truncate_str("", 5), "");
    }

    #[test]
    fn truncate_str_ellipsis_basic() {
        assert_eq!(truncate_str_ellipsis("hello", 10), "hello");
        assert_eq!(truncate_str_ellipsis("hello world", 8), "hello...");
        assert_eq!(truncate_str_ellipsis("", 5), "");
    }

    #[test]
    fn truncate_str_ellipsis_short_width() {
        assert_eq!(truncate_str_ellipsis("hello", 3), "hel");
        assert_eq!(truncate_str_ellipsis("hello", 0), "");
    }

    #[test]
    fn compound_styles_use_no_color_guard() {
        let _ = header_style();
        let _ = dim_style();
        let _ = accent_style(palette().clay_orange);
        let _ = status_ok();
        let _ = status_err();
        let _ = status_warn();
        let _ = footer_style();
        let _ = block_border();
        let _ = block_border_active();
        let _ = clay_accent();
    }

    #[test]
    fn sub_agent_colors_are_distinct() {
        let p = &DARK;
        let colors = [
            p.agent_red, p.agent_blue, p.agent_green, p.agent_yellow,
            p.agent_purple, p.agent_orange, p.agent_pink, p.agent_cyan,
        ];
        let mut set = std::collections::HashSet::new();
        for c in &colors {
            set.insert(format!("{:?}", c));
        }
        assert_eq!(set.len(), 8, "All sub-agent colors should be distinct");
    }

    #[test]
    fn light_accent_contrast_sufficient() {
        // Light-mode success/error/warning must differ from dark
        assert_ne!(format!("{:?}", LIGHT.success), format!("{:?}", DARK.success),
            "light success must be darkened");
        assert_ne!(format!("{:?}", LIGHT.error), format!("{:?}", DARK.error),
            "light error must be darkened");
        assert_ne!(format!("{:?}", LIGHT.warning), format!("{:?}", DARK.warning),
            "light warning must be darkened");
    }
}
