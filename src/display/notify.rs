//! OS-level notifications via `notify-rust`.
//!
//! Best-effort: failures are silently dropped so the pipeline never
//! depends on the user's desktop notification daemon.

use notify_rust::Notification;

/// Send a fire-and-forget desktop notification.
pub fn send(title: &str, body: &str) {
    let _ = Notification::new()
        .appname("niki")
        .summary(title)
        .body(body)
        .show();
}

/// Notify that the agent is waiting for user input (e.g. permission modal).
pub fn permission_needed(command: &str) {
    send(
        "Niki — permission needed",
        &format!("Agent requests approval to run:\n{}", command),
    );
}

/// Notify that the pipeline has finished.
pub fn pipeline_complete(success: bool, branch: &str) {
    if success {
        send(
            "Niki — pipeline complete",
            &format!("Branch `{}` is ready for review.", branch),
        );
    } else {
        send(
            "Niki — pipeline failed",
            "The pipeline did not complete successfully. Check the logs for details.",
        );
    }
}

/// Notify that the pipeline was cancelled.
pub fn pipeline_cancelled() {
    send(
        "Niki — pipeline cancelled",
        "The run was interrupted by the user.",
    );
}
