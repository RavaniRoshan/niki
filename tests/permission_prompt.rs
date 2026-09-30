//! A timeout is not a refusal, and five seconds is not a decision window.
//
// Both sandbox backends did:
//
//     response_rx.recv_timeout(Duration::from_secs(5))
//         .unwrap_or(PermissionAction::Deny)
//     if matches!(action, PermissionAction::Deny) {
//         return Err(anyhow!("Command denied by user: '{}'", full));
//     }
//
// A timeout and a refusal are different events, and this collapsed them. A user
// who read the command for six seconds was told — in the failure that ended
// their run — that *they* had denied it. And `tools.bash` defaults to `Ask`
// (`permissions/mod.rs:83-93`), so this was the outcome for **every** command in
// every interactive run: read a `cargo build` line slowly, and the run fails as
// though you blocked it.

use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

fn src(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
        .unwrap_or_else(|e| panic!("{rel} must be readable: {e}"))
}

/// The refusal message is reserved for an actual refusal.
#[test]
fn only_a_refusal_is_reported_as_a_refusal() {
    for backend in ["src/sandbox/worktree.rs", "src/sandbox/docker.rs"] {
        let s = src(backend);
        assert!(
            !s.contains("recv_timeout(std::time::Duration::from_secs(5))"),
            "{backend} still hard-codes a five-second fuse"
        );
        assert!(
            s.contains("Command denied by user"),
            "{backend}: a refusal must still say so"
        );
        assert!(
            s.contains("That is a timeout, not a refusal"),
            "{backend}: an unanswered prompt must be reported as a timeout, and must say \\
             the command did not run"
        );
    }
}

/// Five seconds cannot be enough to read a command line and decide.
#[test]
fn the_default_window_is_long_enough_to_decide() {
    let d = niki::config::PermissionsConfig::default();
    assert!(
        d.prompt_timeout_seconds >= 30,
        "a permission prompt that closes in {}s denies commands the user was still reading",
        d.prompt_timeout_seconds
    );
}

/// And it is a setting, not a constant, because "long enough" depends on the
/// command and the reader.
#[test]
fn the_window_is_configurable() {
    let cfg: niki::config::NikiConfig = toml::from_str(
        r#"
[permissions]
prompt_timeout_seconds = 45
"#,
    )
    .expect("the snippet parses");
    assert_eq!(
        cfg.permissions.prompt_timeout_seconds, 45,
        "a user must be able to widen the window without editing the binary"
    );
}

/// The timeout is at least one second: a config of 0 must not become an instant
/// deny, which would reintroduce the original bug through the front door.
#[test]
fn a_zero_window_does_not_become_an_instant_deny() {
    let checker = niki::permissions::PermissionChecker::new(niki::permissions::PermissionConfig {
        prompt_timeout_seconds: 0,
        ..Default::default()
    });
    assert_eq!(
        checker.prompt_timeout(),
        Duration::from_secs(1),
        "a configured 0 must not mean 'no time to answer' — that is the defect in a          setting the user can now edit"
    );
    // And the backends read the floored value rather than the raw setting.
    for backend in ["src/sandbox/worktree.rs", "src/sandbox/docker.rs"] {
        let s = src(backend);
        assert!(
            s.contains("permission_checker.prompt_timeout()"),
            "{backend}: the floored window must come from the checker"
        );
    }
}

/// The end-to-end shape: no answer, and the error names what to change.
#[test]
fn the_timeout_error_names_the_way_out() {
    let s = src("src/sandbox/worktree.rs");
    for hint in [
        "prompt_timeout_seconds",
        "fail_closed_headless",
        "was NOT run",
    ] {
        assert!(
            s.contains(hint),
            "the timeout error must tell the user how to change it; missing {hint:?}"
        );
    }
}

/// A real prompt, answered inside the window, is still allowed. Without this a
/// "fix" that made every prompt time out would pass the tests above.
#[tokio::test(flavor = "multi_thread")]
async fn an_answered_prompt_is_honoured() {
    let (tx, rx) = mpsc::channel::<niki::permissions::PermissionAction>();
    tx.send(niki::permissions::PermissionAction::Allow)
        .expect("the receiver is alive");
    let action = tokio::task::block_in_place(|| {
        rx.recv_timeout(Duration::from_secs(30))
            .expect("an answer arrived")
    });
    assert!(matches!(action, niki::permissions::PermissionAction::Allow));
}
