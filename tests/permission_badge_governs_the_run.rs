//! The permission badge must govern a run, not describe one.
//!
//! `state.permission_mode` had six variants and **one reader**: the status
//! bar. Shift+Tab cycled it, `/plan` set it, and the badge printed it — and
//! none of that reached a single tool call. The posture that actually governs a
//! stage is `config.permissions.mode`, which `ToolContext` reads when it
//! builds that stage's context (`orchestrator/pipeline.rs:1263`), so it is read
//! **per stage**.
//!
//! That is what makes the badge fixable rather than retractable: the value
//! travels with `ChatSubmit` and lands on every stage that has not started. The
//! stage in flight keeps the posture it was built with, and both notices say
//! so, because a control that appears broken and one that works are
//! indistinguishable from the outside.
//!
//! The alternative was deleting the badge, which is what batch 1's default
//! ("retracting the claim") would have suggested. This is the better outcome
//! and it is a small one: the plumbing already existed, it just stopped at the
//! TUI.

use niki::config::types::NikiConfig;
use niki::display::state::{AppState, PermissionMode};
use niki::runtime::ToolContext;

/// A whole function body, brace-matched.
///
/// The first version of this file cut at the first `\n}\n`, which lands at the
/// end of the function's *first* inner block. The two lines it asserts on were
/// after that, so `the_processor_applied_the_badge` passed with the
/// application **deleted** — a test that could not fail on the one thing it
/// was written for.
fn fn_body(src: &str, header: &str) -> String {
    let start = src
        .find(header)
        .unwrap_or_else(|| panic!("{header} must exist"));
    let rest = &src[start..];
    let bytes = rest.as_bytes();
    let mut depth = 0usize;
    for (i, b) in bytes.iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return rest[..i + 1].to_string();
                }
            }
            _ => {}
        }
    }
    panic!("{header} is not brace-balanced");
}

fn state() -> AppState {
    AppState::new(
        "test task".into(),
        NikiConfig::default(),
        "/tmp/test".into(),
    )
}

/// Every badge label must map to a posture `parse_permission_mode` honours.
///
/// The labels are not the values — the badge says "don't ask" and the config
/// says `dontask` — and `parse_permission_mode` coerces anything it does not
/// recognise to `manual`. A badge whose value silently coerces is a badge
/// showing one thing and doing another.
#[test]
fn every_badge_maps_to_a_posture_the_tool_loop_honours() {
    for mode in [
        PermissionMode::Default,
        PermissionMode::AcceptEdits,
        PermissionMode::Plan,
        PermissionMode::Auto,
        PermissionMode::DontAsk,
        PermissionMode::BypassPermissions,
    ] {
        let value = mode.config_value();
        let parsed = ToolContext::parse_permission_mode(value);
        assert_eq!(
            parsed,
            value,
            "the {} badge sends `{value}`, which the tool loop turns into \
             `{parsed}` — the badge says one thing and the run does another",
            mode.label()
        );
    }
}

/// And the four that are real postures are the four `parse_permission_mode`
/// acts on. A fifth would be a badge promising a policy nothing implements.
#[test]
fn the_badges_cover_exactly_the_implemented_postures() {
    let mut sent: Vec<&str> = [
        PermissionMode::Default,
        PermissionMode::AcceptEdits,
        PermissionMode::Plan,
        PermissionMode::Auto,
        PermissionMode::DontAsk,
        PermissionMode::BypassPermissions,
    ]
    .iter()
    .map(|m| m.config_value())
    .collect();
    sent.sort_unstable();
    sent.dedup();
    assert_eq!(
        sent,
        vec!["auto", "bypass", "dontask", "manual"],
        "the badge sends a posture the tool loop does not implement, or has \\
         lost one it does. `parse_permission_mode` acts on exactly these four."
    );
}

/// The posture must travel with the submit, or the badge is decoration again.
#[test]
fn the_badge_travels_with_the_submit() {
    let tui = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/display/tui.rs"),
    )
    .expect("tui.rs must be readable");
    let def = tui
        .split("pub struct ChatSubmit {")
        .nth(1)
        .and_then(|r| r.split("\n}").next())
        .expect("ChatSubmit must exist");
    assert!(
        def.contains("permission_mode"),
        "ChatSubmit must carry the posture, or the badge cannot reach a run: {def}"
    );
    // And the sender must fill it from the badge rather than leaving it None,
    // which is the headless path's answer and not the TUI's.
    let send = tui
        .split("let _ = tx.send(ChatSubmit {")
        .nth(1)
        .and_then(|r| r.split("});").next())
        .expect("the TUI must send a ChatSubmit");
    assert!(
        send.contains("permission_mode: Some(") && send.contains("config_value()"),
        "the TUI must send the posture it is displaying: {send}"
    );
}

/// And the processor must apply it, rather than taking the config as gospel.
#[test]
fn the_processor_applies_the_badge() {
    let chat = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cli/chat.rs"),
    )
    .expect("chat.rs must be readable");
    let body = fn_body(&chat, "fn run_task_from_chat(");
    assert!(
        body.contains("effective.permissions.mode"),
        "the posture the badge shows must be written onto the config the run \
         uses, or the badge changes a label and nothing else: {body}"
    );
    assert!(
        body.contains("parse_permission_mode"),
        "and it must go through the same coercion the tool loop uses, so the \\
         badge cannot send a value nothing would honour"
    );
}

/// The stage in flight keeps its posture. Asserted because it is the one
/// honest limitation of this change, and an unasserted limitation is a lie
/// waiting to be discovered by a user.
#[test]
fn the_notice_says_when_the_change_lands() {
    let chat_page = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/display/pages/chat.rs"),
    )
    .expect("chat.rs must be readable");
    let tui = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/display/tui.rs"),
    )
    .expect("tui.rs must be readable");
    for (name, body) in [("chat", chat_page.as_str()), ("tui", tui.as_str())] {
        assert!(
            body.contains("applies from the next stage"),
            "the {name} notice must say when the change lands. A stage already \\
             in flight keeps the posture it was built with, and a user who \\
             cannot tell that from a broken control is the problem this slice \\
             exists to fix."
        );
    }
}

/// The default posture must still be `manual` — a change that made the badge
/// real must not make the product *less* safe by accident.
#[test]
fn the_default_posture_is_still_manual() {
    let st = state();
    assert_eq!(st.permission_mode, PermissionMode::Default);
    assert_eq!(st.permission_mode.config_value(), "manual");
    let config = NikiConfig::default();
    assert_eq!(
        ToolContext::parse_permission_mode(&config.permissions.mode),
        "manual",
        "the shipped default must fail closed"
    );
}
