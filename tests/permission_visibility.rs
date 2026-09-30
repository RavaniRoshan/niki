// A permission decision nobody sees is not a permission decision.
//
// `tools.bash` defaults to `Ask` (permissions/mod.rs:83-93), and both sandbox
// backends fall back to Allow when no TUI is listening and
// `fail_closed_headless` is off — which is the default. So *every command in
// every default run* is auto-approved. That is a defensible default; being
// silent about it is not.
//
// The worktree backend prints to stderr. The Docker backend — **the default
// backend** — only called `tracing::warn!`, three lines below a comment
// claiming "Loud by design", and `tracing` is off unless `RUST_LOG` is set
// while `main.rs:114` installs an ERROR-only filter. A user whose run
// auto-approved a destructive command had no way to find out.
//
// These are source assertions, not behaviour: exercising a real
// `PermissionRequest` round-trip needs a live sandbox, and a test that cannot
// run in CI is not a gate.

use std::path::Path;

fn src(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
        .unwrap_or_else(|e| panic!("{rel} must be readable: {e}"))
}

#[test]
fn both_backends_report_a_headless_auto_approval_on_stderr() {
    for backend in ["src/sandbox/worktree.rs", "src/sandbox/docker.rs"] {
        let s = src(backend);
        let auto_approved = s.find("niki: auto-approved").unwrap_or_else(|| {
            panic!(
                "{backend} falls back to Allow with no TUI listening and says nothing on \
                     stderr. A user cannot find out that a command needing approval was \
                     auto-approved."
            )
        });
        // It must be an `eprintln!`, not a `tracing::warn!` — tracing is off
        // unless RUST_LOG is set, and the default filter is ERROR.
        let window = &s[auto_approved.saturating_sub(200)..auto_approved];
        assert!(
            window.contains("eprintln!"),
            "{backend}: the auto-approval notice must be on stderr, not only in the log"
        );
    }
}

#[test]
fn the_two_backends_report_it_identically() {
    let w = src("src/sandbox/worktree.rs");
    let d = src("src/sandbox/docker.rs");
    let notice = |s: &str| -> String {
        let i = s
            .find("niki: auto-approved")
            .expect("both backends report it");
        s[i..].lines().take(4).collect::<Vec<_>>().join(" ")
    };
    assert_eq!(
        notice(&w),
        notice(&d),
        "the same event must read the same way on both backends, or a user has to learn \
         which one they are on to interpret a warning"
    );
}

#[test]
fn the_default_log_filter_would_have_hidden_it() {
    // The reason a `tracing::warn!` is not enough. If this ever stops being
    // true, the stderr notice is belt-and-braces rather than load-bearing —
    // which is fine, but the comment in the code should say so.
    let s = src("src/main.rs");
    assert!(
        s.contains("from_default_env()"),
        "this test encodes why the stderr notice exists; if the log setup changed, \
         re-read the reasoning in src/sandbox/docker.rs before touching it"
    );
}
