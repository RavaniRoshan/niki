//! W3 — a session exports, in both formats, and says so when there is nothing to export.
//!
//! Drives the real binary. A transcript renderer is exactly the kind of thing that is correct
//! for the happy path and wrong for a message containing a code fence, so the cases below are
//! the awkward ones.

use std::path::PathBuf;
use std::process::Command;

fn niki_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_niki"))
}

struct Project {
    dir: tempfile::TempDir,
}

impl Project {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp project");
        std::fs::create_dir_all(dir.path().join(".niki/sessions")).expect("sessions dir");
        Self { dir }
    }

    fn path(&self) -> &std::path::Path {
        self.dir.path()
    }

    /// Write a session with the given messages, without needing a model call.
    fn write_session(&self, id: &str, messages: &[(&str, &str)], tokens: (u64, u64), cost: f64) {
        let msgs: Vec<serde_json::Value> = messages
            .iter()
            .map(|(role, content)| {
                serde_json::json!({
                    "role": role,
                    "content": content,
                    "timestamp": "2026-10-05T00:00:00Z",
                })
            })
            .collect();
        let doc = serde_json::json!({
            "id": id,
            "project_path": self.dir.path().display().to_string(),
            "title": "An exported session",
            "messages": msgs,
            "model": "mock-model",
            "provider": "mock",
            "total_input_tokens": tokens.0,
            "total_output_tokens": tokens.1,
            "total_cost_usd": cost,
            "created_at": "2026-10-05T00:00:00Z",
            "updated_at": "2026-10-05T00:01:00Z",
            "checkpoints": [],
            "schema_version": 1,
        });
        std::fs::write(
            self.dir.path().join(format!(".niki/sessions/{id}.json")),
            serde_json::to_string_pretty(&doc).expect("json"),
        )
        .expect("write the session");
    }

    fn run(&self, args: &[&str]) -> (String, String, bool) {
        let out = Command::new(niki_bin())
            .arg("session")
            .args(args)
            .args(["--project", self.path().to_str().expect("utf-8")])
            .output()
            .expect("niki runs");
        (
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
            out.status.success(),
        )
    }
}

#[test]
fn a_session_exports_as_markdown_to_stdout() {
    let p = Project::new();
    p.write_session(
        "s1",
        &[
            ("user", "add a health endpoint"),
            ("assistant", "Done. Added `src/health.rs`."),
        ],
        (1200, 340),
        0.0042,
    );
    let (out, _err, ok) = p.run(&["export", "s1"]);
    assert!(ok, "export failed:\n{out}");

    assert!(out.contains("# An exported session"), "no title:\n{out}");
    assert!(
        out.contains("1200") && out.contains("340"),
        "the transcript must carry the real usage numbers:\n{out}"
    );
    assert!(out.contains("$0.0042"), "the cost is missing:\n{out}");
    assert!(
        out.contains("mock-model"),
        "a transcript without its model reads as a conversation nobody had:\n{out}"
    );
    let user_at = out.find("add a health endpoint").expect("the user turn");
    let asst_at = out.find("Done. Added").expect("the assistant turn");
    assert!(
        user_at < asst_at,
        "the transcript reordered the conversation"
    );
}

#[test]
fn a_message_containing_a_code_fence_stays_readable() {
    // The fence inside the message has to be widened, or the transcript ends its block early
    // and the rest of the message renders as prose.
    let p = Project::new();
    p.write_session(
        "s2",
        &[(
            "assistant",
            "Here is the function:\n```rust\nfn main() {}\n```\nand that is all.",
        )],
        (10, 10),
        0.0,
    );
    let (out, _err, ok) = p.run(&["export", "s2"]);
    assert!(ok, "export failed:\n{out}");
    assert!(
        out.contains("````"),
        "a message containing ``` must be wrapped in a wider fence:\n{out}"
    );
    assert!(
        out.contains("and that is all."),
        "the tail of the message was cut off by its own fence:\n{out}"
    );
}

#[test]
fn a_session_exports_as_atif_that_validates() {
    let p = Project::new();
    p.write_session(
        "s3",
        &[("user", "hello"), ("assistant", "hi")],
        (50, 20),
        0.001,
    );
    let (out, _err, ok) = p.run(&["export", "s3", "--format", "atif"]);
    assert!(ok, "atif export failed:\n{out}");

    let doc: serde_json::Value = serde_json::from_str(&out)
        .unwrap_or_else(|e| panic!("the ATIF export is not valid JSON: {e}\n{out}"));
    assert_eq!(doc["agent"]["name"], "niki");
    assert!(
        doc["atif_version"]
            .as_str()
            .is_some_and(|v| v.starts_with("ATIF-")),
        "no schema version declared:\n{out}"
    );

    let steps = doc["steps"].as_array().expect("steps array");
    assert_eq!(steps.len(), 2, "both turns must be present:\n{out}");
    for (i, s) in steps.iter().enumerate() {
        assert_eq!(
            s["step_id"].as_u64(),
            Some(i as u64 + 1),
            "ids must be sequential"
        );
    }
    assert_eq!(steps[0]["source"], "user");
    // The stored role for an assistant turn is "assistant", which is not one of ATIF's three
    // sources. Mapping it to `agent` rather than inventing a fourth source is the point.
    assert_eq!(
        steps[1]["source"], "agent",
        "an assistant turn must map onto a declared ATIF source:\n{out}"
    );
    assert_eq!(
        doc["final_metrics"]["total_prompt_tokens"].as_u64(),
        Some(50),
        "final_metrics must sum what the session recorded:\n{out}"
    );
}

#[test]
fn export_writes_to_a_file_when_asked() {
    let p = Project::new();
    p.write_session("s4", &[("user", "hello")], (1, 1), 0.0);
    let dest = p.path().join("out").join("transcript.md");
    let (out, err, ok) = p.run(&["export", "s4", "--out", dest.to_str().expect("utf-8")]);
    assert!(ok, "export to a file failed:\n{out}\n{err}");
    let body = std::fs::read_to_string(&dest).expect("the file exists");
    assert!(body.contains("hello"), "the file has no content:\n{body}");
}

#[test]
fn a_session_with_no_usage_says_nothing_about_cost() {
    // Zeros would read as "this cost nothing", which is a claim the session never made.
    let p = Project::new();
    p.write_session("s5", &[("user", "hi")], (0, 0), 0.0);
    let (out, _err, ok) = p.run(&["export", "s5"]);
    assert!(ok, "export failed:\n{out}");
    assert!(
        !out.contains("$0.0000"),
        "an unpriced session must not print a price:\n{out}"
    );
    assert!(
        !out.contains("Usage:"),
        "an unused session must not print usage:\n{out}"
    );
}

#[test]
fn exporting_nothing_fails_and_writes_no_file() {
    let p = Project::new();
    let dest = p.path().join("should-not-exist.md");
    let (out, err, ok) = p.run(&["export", "--out", dest.to_str().expect("utf-8")]);
    assert!(!ok, "exporting a nonexistent session must not succeed");
    assert!(
        err.contains("No session to export"),
        "the refusal must say what is missing:\n{out}\n{err}"
    );
    assert!(
        !dest.exists(),
        "a refused export must not leave an empty file, which would be indistinguishable from \
         a session with no messages"
    );
}

#[test]
fn a_session_with_no_messages_still_exports_and_says_so() {
    let p = Project::new();
    p.write_session("s6", &[], (0, 0), 0.0);
    let (out, _err, ok) = p.run(&["export", "s6"]);
    assert!(ok, "an empty session is still a session:\n{out}");
    assert!(
        out.contains("recorded no messages"),
        "an empty transcript must say it is empty, not just be empty:\n{out}"
    );
}
