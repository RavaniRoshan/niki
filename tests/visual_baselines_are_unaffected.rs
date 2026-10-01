//! The working-status glyph change must not have moved a reference frame.
//!
//! `tests/visual/run.sh` says, in capitals, that blessing a reference locally
//! is not a local operation: the render depends on the machine, so locally
//! blessed frames leave every comparison failing. So the correct move is not to
//! regenerate anything — it is to establish that **nothing needs regenerating**.
//!
//! The activity line only draws while a stage is `Running`, and every tape
//! types a slash command into `niki chat` without ever starting a pipeline. So
//! no reference frame contains the line, and no baseline moves. That is a claim
//! about the tapes, and a claim about tapes rots exactly like a claim about
//! code — so it is pinned here rather than asserted in a commit message.
//!
//! If a tape ever grows a `/run`, this fails, and the right answer is to
//! re-bless on the runner — not to loosen this test.

use std::path::Path;

fn tapes() -> Vec<std::path::PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/visual/tapes");
    let mut found: Vec<_> = std::fs::read_dir(dir)
        .expect("tests/visual/tapes must exist — it is tracked in git")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "tape"))
        .collect();
    found.sort();
    assert!(!found.is_empty(), "no tapes found — the glob is wrong");
    found
}

#[test]
fn no_visual_tape_starts_a_pipeline() {
    for tape in tapes() {
        let body = std::fs::read_to_string(&tape).expect("tape must be readable");
        assert!(
            !body.contains("/run"),
            "{} types `/run`, so it renders a Running stage and the activity \\
             line — and every reference frame for it must be re-blessed **on \\
             the runner**, not locally. Blessing locally leaves every frame \\
             failing (see tests/visual/run.sh).",
            tape.display()
        );
    }
}

/// And the tapes are chat-surface only, which is the same claim from the other
/// side: they launch `chat`, so the pipeline pages are rendered from a state
/// no run ever populated.
#[test]
fn every_visual_tape_launches_the_chat_surface() {
    for tape in tapes() {
        let body = std::fs::read_to_string(&tape).expect("tape must be readable");
        assert!(
            body.contains("chat -p"),
            "{} does not launch `niki chat`, so the 'no baseline moves' claim \\
             needs re-checking: a tape that drives a different surface may \\
             render the activity line.",
            tape.display()
        );
    }
}
