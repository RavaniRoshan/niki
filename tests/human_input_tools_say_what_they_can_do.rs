//! Two tools the model is offered, and what it is told about them.
//!
//! `ROADMAP.md` §2 said:
//!
//! > `ask_user` / `approval` return "cannot ask" in any TUI run, because
//! > `TUI_OWNS_STDIN` makes `is_interactive_stdin()` false. **No modal exists.**
//!
//! **The first half is not a defect, and the record said it was.** Failing
//! closed there is deliberate, and `runtime/tools.rs` documents why at length:
//! the TUI holds stdin in raw mode and runs its own `event::read()` on it, so
//! `read_line` would race the interface's event loop and could take a stray
//! `y` — typed at the *interface*, for something it never showed the user —
//! as consent to a command. That is a serious failure mode, and denying is
//! the right answer to it.
//!
//! So the fix is not to change the behaviour. It is to make sure the **model**
//! is told something it can act on. Both descriptions already said "when stdin
//! is not interactive" — a condition the model cannot check, so it had to
//! guess. They now name the situation: unavailable in a TUI run, always denies,
//! do not plan around it, useful only on a bare terminal.
//!
//! What remains is a capability gap, not a correctness one: **in a TUI run
//! there is no way to ask the user anything.** Recorded in `ROADMAP.md`, with
//! what closing it would cost.

use niki::runtime::tools::{ApprovalTool, AskUserTool, Tool};

/// Every registered tool's description is what the model reads before calling.
/// A description that omits a known precondition is the defect.
#[test]
fn ask_user_says_it_is_unavailable_in_a_tui_run() {
    let d = AskUserTool.def().description.to_lowercase();
    // Lower-cased: the exact casing of prose is brittle for no gain, and
    // the first version matched "TUI" against a lower-cased string.
    assert!(
        d.contains("tui"),
        "the model must be told *when* this cannot work, not just a \\
         condition it cannot check: {d}"
    );
    assert!(
        d.contains("niki chat") && d.contains("--tui"),
        "and the concrete situations, so it does not have to infer them: {d}"
    );
    // **What replaced "do not call".**
    //
    // This asserted a literal phrase, which was the right instruction when
    // `ask_user` genuinely could not work in a TUI run. That is no longer true:
    // it now has a modal of its own and is answered there, so telling the model
    // never to call it would be wrong.
    //
    // What still has to be true — and is what the model actually needs — is
    // that an **unattended** run is named as the case where it cannot ask, and
    // that the model must not invent an answer when nobody answers. Pinned here
    // in substance rather than in one phrasing: the phrase changed, the
    // behaviour did not.
    assert!(
        d.contains("unattended"),
        "and it must name the case where there is nobody to ask, which the \
         model cannot otherwise infer: {d}"
    );
    assert!(
        d.contains("never invent") || d.contains("decide yourself"),
        "and what to do when nobody answers — invent nothing: {d}"
    );
}

/// `approval` is a *denial*, not an error, and the model must know that
/// planning around it will not work.
#[test]
fn approval_says_it_always_denies_in_a_tui_run() {
    // Lower-cased: asserting the exact casing of prose is brittle for no
    // gain, and the first version matched "ALWAYS DENIES" against a
    // description that says "Always DENIES".
    let d = ApprovalTool.def().description.to_lowercase();
    assert!(
        d.contains("denies") && d.contains("unattended"),
        "`approval` fail-closes, and the model must be told it is not a \\
         question it can retry: {d}"
    );
    // The description says "denies" and "there is nobody to ask"; it does not use
    // the words "fail-closed", and does not need to — the model is told the
    // behaviour, not the vocabulary. What has to survive a description rewrite
    // is that it is told **not to plan around it**, in whatever words, so the
    // instruction is pinned to the advice rather than to a phrase that has
    // already been reworded once.
    assert!(
        d.contains("do not plan around") || d.contains("report what you need"),
        "and what to do instead, so the model does not build a plan that waits \
         on a human who is not there: {d}"
    );
}

/// The **behaviour** must not move. These are the fail-closed paths, and a
/// description change is exactly the kind of edit that could quietly relax one.
#[test]
fn the_fail_closed_behaviour_is_untouched() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/runtime/tools.rs"),
    )
    .expect("tools.rs must be readable");
    let body = src
        .split("fn is_interactive_stdin()")
        .nth(1)
        .and_then(|r| r.split("\n}\n").next())
        .expect("is_interactive_stdin must exist");
    assert!(
        body.contains("stdin_owned_by_tui()"),
        "the TUI must still claim stdin, or a stray keypress could be read as \\
         consent"
    );
    // And the strays are still refused.
    assert!(
        body.contains("NIKI_NON_INTERACTIVE"),
        "an explicitly non-interactive run must still be refused: {body}"
    );
}

/// Nothing else may call them internally — a tool the product itself depends on
/// would make "always fails" a much larger problem than two advertised tools.
#[test]
fn nothing_in_the_product_relies_on_them_working() {
    // The only callers are their own `execute` bodies and the two unit tests
    // that assert the fail-closed path. If a real caller appears, the roadmap
    // entry about the capability gap is wrong and this fails.
    let mut callers = Vec::new();
    for entry in std::fs::read_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"))
        .expect("src must be readable")
        .flatten()
    {
        let p = entry.path();
        if p.extension().is_none_or(|x| x != "rs") {
            continue;
        }
        let Ok(body) = std::fs::read_to_string(&p) else {
            continue;
        };
        for (i, line) in body.lines().enumerate() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            if (t.contains("AskUserTool") || t.contains("ApprovalTool"))
                && !t.contains("pub struct")
                && !t.contains("Box::new(")
                && !t.contains("impl Tool")
            {
                callers.push(format!("{}:{}", p.display(), i + 1));
            }
        }
    }
    assert!(
        callers.is_empty(),
        "something now calls the human-input tools: {callers:?}. They always \\
         fail in a TUI run, so a real caller would be a broken feature rather \\
         than an advertised stub."
    );
}
