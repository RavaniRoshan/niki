//! The tool-card renderer is live, and the roadmap said it was not.
//!
//! `ROADMAP.md` §2 claimed:
//!
//! > Tool cards in the transcript, with arguments and results. The renderer
//! > (`components/tool_card.rs`, `tool_detail.rs`, the Enter hit-test) is fully
//! > built **and unreachable**, because the chat sends `tools: None`.
//!
//! **The renderer is reachable, and has been all along.** `ToolCard::new` is
//! called from two production sites — `display/state.rs` on
//! `DisplayEvent::ToolCall`, and `display/tui.rs` — and `tool_detail`'s four
//! entry points are called from `tui.rs`. The Coder's tool loop emits
//! `DisplayEvent::ToolCall` for every call it makes, so a run's tool calls
//! render with their arguments and their results today.
//!
//! The conflation is between two surfaces. The **chat** sends `tools: None`,
//! so a *conversation* turn never produces a card — and that is the owner's
//! §0a decision, not a defect: "`/run <task>` starts the pipeline, plain
//! messages stay conversation turns." The **pipeline** runs tools, and its cards
//! render.
//!
//! So the item is not a missing feature to build. It is a record entry that
//! reads as a bug and would invite someone to "fix" the chat by giving it
//! tools — which would undo §0a.

use std::path::Path;

fn read(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
        .unwrap_or_else(|e| panic!("{rel} must be readable: {e}"))
}

/// The renderer must stay wired to the pipeline's tool events.
#[test]
fn a_tool_call_becomes_a_card() {
    let state = read("src/display/state.rs");
    // A wide window: the first version cut the arm at its closing brace, which
    // lands *before* the `ToolCard::new` call a few lines further down, so the
    // window never contained the thing it was checking for.
    let arm = state
        .split("DisplayEvent::ToolCall {")
        .nth(1)
        .map(|r| r.chars().take(400).collect::<String>())
        .expect("the ToolCall arm must exist");
    // The card must be built *from this call's own name and summary*, not
    // merely built. The first version checked for `ToolCard::new(` and
    // therefore passed a sabotage that kept the call and replaced its
    // arguments — the card rendered, labelled "unwired".
    let call = arm
        .split("ToolCard::new(")
        .nth(1)
        .and_then(|r| r.split(')').next())
        .expect("a card must be built in the ToolCall arm");
    assert!(
        call.contains("tool_name") && call.contains("summary"),
        "the card must carry the tool call's own name and summary, or the \
         transcript shows every call with the same label: ToolCard::new({call})"
    );
    assert!(
        arm.contains("set_running()"),
        "and the card must show as running until its result arrives: {arm}"
    );
}

/// And the results must land on it.
#[test]
fn a_tool_result_lands_on_its_card() {
    let state = read("src/display/state.rs");
    assert!(
        state.contains("DisplayEvent::ToolResult {"),
        "a tool result must be handled, or a card runs for ever"
    );
    let card_src = read("src/display/components/tool_card.rs");
    // The real API: `set_success(output, duration_ms)` and `set_failed(error)`.
    // The first version guessed `set_error` and `set_output`, which do not exist.
    for setter in ["set_success", "set_failed", "status_glyph", "timing"] {
        assert!(
            card_src.contains(setter),
            "the card must support `{setter}`, or a result cannot be shown on it"
        );
    }
}

/// The detail overlay must stay reachable from the event loop, which is the
/// "Enter hit-test" the roadmap called unreachable.
#[test]
fn the_detail_overlay_is_reachable() {
    let tui = read("src/display/tui.rs");
    for entry in [
        "tool_detail::route_click",
        "render_tool_detail",
        "detail_viewport",
    ] {
        assert!(
            tui.contains(entry),
            "`{entry}` is no longer called from the event loop, so the detail \\
             overlay is dead code"
        );
    }
}

/// And the two surfaces must stay *different* on purpose. This is the part
/// that stops a well-meaning change from breaking the owner's decision: giving
/// the chat tools would make conversation turns able to edit the project, which
/// is exactly what §0a says they must not do.
#[test]
fn the_chat_still_sends_no_tools() {
    let chat = read("src/cli/chat.rs");
    let stream_reply = chat
        .split("async fn stream_reply(")
        .nth(1)
        .and_then(|r| r.split("\n}\n").next())
        .expect("stream_reply must exist");
    assert!(
        stream_reply.contains("tools: None"),
        "the chat must not send tools: a plain conversation turn is not \\
         permitted to edit the project (§0a). If this is being changed, that \\
         decision is being reversed and `ROADMAP.md` §0a must change with it."
    );
    // And the system prompt must not claim otherwise — batch 1 removed those
    // rules, and a description-like edit can put them back.
    let prompt = chat
        .split("const CHAT_SYSTEM_PROMPT: &str = \"")
        .nth(1)
        .and_then(|r| r.split("\";\n").next())
        .expect("the chat system prompt must exist");
    for lie in ["dedicated native tools", "read files", "use your tools"] {
        assert!(
            !prompt.to_lowercase().contains(lie),
            "the chat prompt claims a capability it was not given — {lie:?}. \\
             The chat sends no tools, so any such claim makes the model \\
             narrate work it cannot do. Prompt: {prompt}"
        );
    }
}

/// A tool result must never vanish, whatever the interface saw beforehand.
///
/// `AppState`'s `ToolResult` arm matched a result to a pending card and had
/// **no `else`** — so a result whose `ToolCall` was never applied simply
/// disappeared. A tool that ran, did work and reported an outcome became a
/// card that never appeared: the same failure as `web_search` returning
/// `Success` with nothing in it, except here the user cannot even tell it
/// happened.
///
/// That is not hypothetical. `ROADMAP.md` §9.2a's live reading is "one call,
/// answered, and the interface still painting a question the tool already
/// answered" — and a dropped result is one way a run's output and its cards
/// drift apart.
#[test]
fn a_result_with_no_card_to_land_on_is_still_shown() {
    use niki::artifacts::types::AgentRole;
    use niki::display::tui::DisplayEvent;

    let config = niki::config::NikiConfig::default();
    let mut state = niki::display::state::AppState::new("t".to_string(), config, ".".into());

    // No ToolCall first: the result arrives on its own.
    state.apply_event(DisplayEvent::ToolResult {
        role: AgentRole::Coder,
        tool_name: "read_file".into(),
        success: true,
        error: None,
        output: Some("fn main() {}".into()),
        duration_ms: 12,
    });

    assert_eq!(
        state.tool_cards.len(),
        1,
        "the result must produce a card of its own, or the work is invisible: \
         {:?}",
        state.tool_cards
    );
    let card = &state.tool_cards[0];
    assert_eq!(card.tool_name, "read_file");
    assert_eq!(
        card.output.as_deref(),
        Some("fn main() {}"),
        "and it must carry the output the tool reported — a card that says \\
         only that a tool ran is not a result"
    );
}

/// The same for a failure, and the failure's own message.
#[test]
fn a_failure_with_no_card_to_land_on_is_still_shown() {
    use niki::artifacts::types::AgentRole;
    use niki::display::tui::DisplayEvent;

    let config = niki::config::NikiConfig::default();
    let mut state = niki::display::state::AppState::new("t".to_string(), config, ".".into());
    state.apply_event(DisplayEvent::ToolResult {
        role: AgentRole::Coder,
        tool_name: "bash".into(),
        success: false,
        error: Some("command not found".into()),
        output: None,
        duration_ms: 5,
    });
    assert_eq!(state.tool_cards.len(), 1, "a failure is still a result");
    assert_eq!(state.tool_cards[0].tool_name, "bash");
}

/// And the ordinary path is unchanged: a result lands on its call's card
/// rather than making a second one. A fix that always appended would turn
/// every tool call into two cards.
#[test]
fn a_result_still_lands_on_its_own_card() {
    use niki::artifacts::types::AgentRole;
    use niki::display::tui::DisplayEvent;

    let config = niki::config::NikiConfig::default();
    let mut state = niki::display::state::AppState::new("t".to_string(), config, ".".into());

    state.apply_event(DisplayEvent::ToolCall {
        role: AgentRole::Coder,
        tool_name: "grep".into(),
        summary: "grep in src".into(),
    });
    state.apply_event(DisplayEvent::ToolResult {
        role: AgentRole::Coder,
        tool_name: "grep".into(),
        success: true,
        error: None,
        output: Some("3 matches".into()),
        duration_ms: 7,
    });

    assert_eq!(
        state.tool_cards.len(),
        1,
        "a call and its result are one card, not two: {:?}",
        state.tool_cards
    );
    assert_eq!(state.tool_cards[0].output.as_deref(), Some("3 matches"));
}

/// Both sandboxes' **artifact** path must treat an already-applied edit as
/// done, and both must still refuse a wrong one.
///
/// Found by a live run (`stealth/space-bunny-alpha`): the Coder used the
/// `edit` tool — applying the change to the worktree — and then submitted a
/// `CodeDiff` describing the same change. The pipeline applies the artifact on
/// top of what the tools already did, so every block's `search` text was gone
/// and the run reported *"the patch did not apply"*. A model that uses the
/// tools *and* submits an artifact, which the protocol invites, produced an
/// unapplicable artifact by construction.
///
/// This is a source assertion rather than a behavioural one on purpose: both
/// sandboxes' apply loops are `async fn apply_patch` on the `Sandbox` trait,
/// and standing up a real worktree and a real container to drive them is what
/// `tests/worktree_policy.rs` and `tests/sandbox_teardown.rs` are for. What
/// has to be pinned *here* is that **both** call sites use the
/// already-done-aware function — a fix wired into one sandbox would leave a
/// default backend that still rejects a perfectly good artifact.
#[test]
fn both_sandboxes_treat_an_already_applied_artifact_edit_as_done() {
    for (file, plain) in [
        ("src/sandbox/worktree.rs", "apply_single_edit_block("),
        ("src/sandbox/docker.rs", "apply_single_edit_block("),
    ] {
        let src = read(file);
        let aware = src.matches("apply_edit_block_or_already_done(").count();
        assert_eq!(
            aware, 1,
            "{file} must call `apply_edit_block_or_already_done` exactly once, in \
             its `apply_patch`. Found {aware}."
        );
        // And the plain call must survive only where it belongs: the `edit`
        // tool's own matching, if this file has one.
        let plain_uses = src.matches(plain).count();
        assert!(
            plain_uses == 0 || plain_uses + aware == src.matches("apply_single_edit").count(),
            "{file}: sanity — the counts should add up"
        );
    }
}

/// And the `edit` tool must **not** get the lenient behaviour.
///
/// Replacing text with text that is already there is a no-op the user asked
/// for, and telling them it succeeded is the failure this whole feature is
/// about. The leniency belongs to the artifact path alone.
#[test]
fn the_edit_tool_itself_still_refuses_an_already_present_replacement() {
    let tools = read("src/runtime/tools.rs");
    assert!(
        !tools.contains("apply_edit_block_or_already_done"),
        "the `edit` tool must keep the strict matcher: a user who asks to \
         replace text with text that is already present should be told, not \
         quietly succeeded"
    );
}
