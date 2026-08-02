//! Theme function tests.
//!
//! Tests role_color, role_glyph, role_name, style builders, and color constants.

use niki::artifacts::types::AgentRole;
use niki::display::theme;

// ═══════════════════════════════════════════════════════════════════════════
// role_color
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn role_color_planner_is_blue() {
    assert_eq!(theme::role_color(AgentRole::Planner), theme::BLUE);
}

#[test]
fn role_color_coder_is_purple() {
    assert_eq!(theme::role_color(AgentRole::Coder), theme::PURPLE);
}

#[test]
fn role_color_tester_is_green() {
    assert_eq!(theme::role_color(AgentRole::Tester), theme::GREEN);
}

#[test]
fn role_color_reviewer_is_amber() {
    assert_eq!(theme::role_color(AgentRole::Reviewer), theme::AMBER);
}

#[test]
fn role_color_synthesizer_is_cyan() {
    assert_eq!(theme::role_color(AgentRole::Synthesizer), theme::CYAN);
}

#[test]
fn role_color_security_auditor_is_red() {
    assert_eq!(theme::role_color(AgentRole::SecurityAuditor), theme::RED);
}

#[test]
fn role_color_red_is_distinct() {
    let c = theme::role_color(AgentRole::Red);
    assert_ne!(c, theme::RED); // Red agent has its own shade
}

#[test]
fn role_color_all_roles_distinct() {
    let roles = [
        AgentRole::Planner,
        AgentRole::Coder,
        AgentRole::Tester,
        AgentRole::Reviewer,
        AgentRole::Synthesizer,
        AgentRole::SecurityAuditor,
        AgentRole::Red,
    ];
    let colors: Vec<_> = roles.iter().map(|r| theme::role_color(*r)).collect();
    // At least Planner, Coder, Tester, Reviewer should be distinct
    assert_ne!(colors[0], colors[1]); // Planner != Coder
    assert_ne!(colors[1], colors[2]); // Coder != Tester
    assert_ne!(colors[2], colors[3]); // Tester != Reviewer
}

// ═══════════════════════════════════════════════════════════════════════════
// role_glyph
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn role_glyph_returns_non_empty() {
    let roles = [
        AgentRole::Planner,
        AgentRole::Coder,
        AgentRole::Tester,
        AgentRole::Reviewer,
        AgentRole::Synthesizer,
        AgentRole::SecurityAuditor,
        AgentRole::Red,
    ];
    for role in &roles {
        let glyph = theme::role_glyph(*role);
        assert!(!glyph.is_empty(), "{role:?} has empty glyph");
    }
}

#[test]
fn role_glyph_planner_is_diamond() {
    assert_eq!(theme::role_glyph(AgentRole::Planner), "◆");
}

#[test]
fn role_glyph_coder_is_lightning() {
    assert_eq!(theme::role_glyph(AgentRole::Coder), "⚡");
}

#[test]
fn role_glyph_tester_is_circle() {
    assert_eq!(theme::role_glyph(AgentRole::Tester), "●");
}

// ═══════════════════════════════════════════════════════════════════════════
// role_name
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn role_name_all_correct() {
    assert_eq!(theme::role_name(AgentRole::Planner), "Planner");
    assert_eq!(theme::role_name(AgentRole::Coder), "Coder");
    assert_eq!(theme::role_name(AgentRole::Tester), "Tester");
    assert_eq!(theme::role_name(AgentRole::Reviewer), "Reviewer");
    assert_eq!(theme::role_name(AgentRole::Synthesizer), "Synthesizer");
    assert_eq!(theme::role_name(AgentRole::SecurityAuditor), "Security");
    assert_eq!(theme::role_name(AgentRole::Red), "Red");
}

// ═══════════════════════════════════════════════════════════════════════════
// Style builders
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn header_style_is_bold_and_bright() {
    use ratatui::style::Modifier;
    let s = theme::header_style();
    assert_eq!(s.fg, Some(theme::FG_BRIGHT));
    assert!(s.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn dim_style_is_dim() {
    let s = theme::dim_style();
    assert_eq!(s.fg, Some(theme::FG_DIM));
}

#[test]
fn accent_style_has_color_and_bold() {
    use ratatui::style::Modifier;
    let s = theme::accent_style(theme::GREEN);
    assert_eq!(s.fg, Some(theme::GREEN));
    assert!(s.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn status_styles_are_distinct() {
    let ok = theme::status_ok();
    let err = theme::status_err();
    let warn = theme::status_warn();
    assert_ne!(ok.fg, err.fg);
    assert_ne!(err.fg, warn.fg);
    assert_ne!(ok.fg, warn.fg);
}

#[test]
fn footer_style_is_dim() {
    let s = theme::footer_style();
    assert_eq!(s.fg, Some(theme::FG_DIM));
}

#[test]
fn block_border_is_border_color() {
    let s = theme::block_border();
    assert_eq!(s.fg, Some(theme::BORDER));
}

#[test]
fn block_border_active_is_active_color() {
    let s = theme::block_border_active();
    assert_eq!(s.fg, Some(theme::BORDER_ACTIVE));
}

// ═══════════════════════════════════════════════════════════════════════════
// Color constants
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn color_constants_are_rgb() {
    use ratatui::style::Color;
    assert!(matches!(theme::BG, Color::Rgb(_, _, _)));
    assert!(matches!(theme::BG_ELEVATED, Color::Rgb(_, _, _)));
    assert!(matches!(theme::BG_HIGHLIGHT, Color::Rgb(_, _, _)));
    assert!(matches!(theme::BORDER, Color::Rgb(_, _, _)));
    assert!(matches!(theme::BORDER_ACTIVE, Color::Rgb(_, _, _)));
    assert!(matches!(theme::FG, Color::Rgb(_, _, _)));
    assert!(matches!(theme::FG_DIM, Color::Rgb(_, _, _)));
    assert!(matches!(theme::FG_BRIGHT, Color::Rgb(_, _, _)));
    assert!(matches!(theme::GREEN, Color::Rgb(_, _, _)));
    assert!(matches!(theme::BLUE, Color::Rgb(_, _, _)));
    assert!(matches!(theme::RED, Color::Rgb(_, _, _)));
    assert!(matches!(theme::AMBER, Color::Rgb(_, _, _)));
    assert!(matches!(theme::CYAN, Color::Rgb(_, _, _)));
    assert!(matches!(theme::PURPLE, Color::Rgb(_, _, _)));
}

#[test]
fn dark_theme_background_is_dark() {
    // BG should be near-black (#0d0d0d = 13,13,13)
    if let ratatui::style::Color::Rgb(r, g, b) = theme::BG {
        assert!(r < 30, "BG red too bright: {r}");
        assert!(g < 30, "BG green too bright: {g}");
        assert!(b < 30, "BG blue too bright: {b}");
    }
}

#[test]
fn diff_colors_are_distinct() {
    assert_ne!(theme::DIFF_ADD_BG, theme::DIFF_DEL_BG);
}

// ═══════════════════════════════════════════════════════════════════════════
// Theme (legacy console)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn legacy_theme_default_creates_all_agents() {
    let t = theme::Theme::new();
    assert_eq!(t.planner.name, "Planner");
    assert_eq!(t.coder.name, "Coder");
    assert_eq!(t.tester.name, "Tester");
    assert_eq!(t.reviewer.name, "Reviewer");
    assert_eq!(t.synthesizer.name, "Synthesizer");
    assert_eq!(t.security_auditor.name, "Security Auditor");
    assert_eq!(t.red.name, "Red");
}
