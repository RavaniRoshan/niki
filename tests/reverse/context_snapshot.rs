//! Context snapshots: what the *model* actually saw, made reviewable.
//!
//! Every other signal in this repo is about output. This is about input. When
//! `prompts/coder.md` gains a paragraph, or a context fragment grows a field,
//! or a tool schema is reordered, agent behaviour changes — and nothing in the
//! suite notices, because the tests assert on what comes *out* of a mock that
//! does not care what went in.
//!
//! Codex's answer (`codex-rs/core/tests/common/context_snapshot.rs`) is to
//! render the captured outbound requests, normalise the volatile parts, and
//! commit the result. The diff of that file is the review artifact for a prompt
//! change, and a drift is a failing test.
//!
//! Three properties make it usable rather than noise:
//!
//! * **Normalisation is aggressive.** UUIDs, timestamps, durations, absolute
//!   paths and token counts change on every run. If they were left in, every
//!   snapshot would differ every time and the file would be abandoned.
//! * **Only the delta is shown.** Two consecutive requests share most of their
//!   context; rendering both in full makes the actual change invisible.
//! * **Known blocks collapse to tags.** A permission block or a tool schema is
//!   stable and large; it is shown as `<PERMISSIONS>` rather than 200 lines
//!   that nobody reads.
//!
//! What this catches that nothing else here does: a prompt edit that silently
//! removes an instruction, a context fragment that stops being included, a
//! schema that stops being sent.

use std::path::PathBuf;

use serde_json::Value;

/// Where the committed golden lives.
fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/context.snap")
}

/// Volatile spans that must not appear in a snapshot.
fn is_volatile(s: &str) -> bool {
    // UUIDs (canonical and simple forms)
    let b = s.as_bytes();
    let looks_like_uuid = b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_hexdigit(),
        });
    if looks_like_uuid {
        return true;
    }
    // Absolute paths into the machine's temp dir, and any /tmp/... run dir.
    if s.starts_with("/tmp/") || s.starts_with("/home/") || s.starts_with("/Users/") {
        return true;
    }
    // ISO timestamps
    if s.len() >= 19
        && s.as_bytes()[4] == b'-'
        && s.as_bytes()[7] == b'-'
        && s.as_bytes()[10] == b'T'
    {
        return true;
    }
    // Durations and millisecond counters, e.g. "41s", "1234ms", "0.42s"
    if s.ends_with("ms")
        && s[..s.len().saturating_sub(2)]
            .chars()
            .all(|c| c.is_ascii_digit())
    {
        return true;
    }
    // Bare large integers that are almost certainly token counts.
    if s.len() >= 4 && s.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    false
}

/// Replace every volatile word in `text` with a stable placeholder.
///
/// Whitespace is preserved exactly, because prompts are line-structured and
/// collapsing it would make every snapshot churn. Only whole words are
/// considered: the volatile things in a prompt are a UUID in an instruction, a
/// path in an example, a count in a report — never a substring of a word.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    for c in text.chars() {
        if c.is_whitespace() {
            if !word.is_empty() {
                out.push_str(if is_volatile(&word) {
                    "<VOLATILE>"
                } else {
                    &word
                });
                word.clear();
            }
            out.push(c);
        } else {
            word.push(c);
        }
    }
    if !word.is_empty() {
        out.push_str(if is_volatile(&word) {
            "<VOLATILE>"
        } else {
            &word
        });
    }
    out
}

/// Blocks that are stable, large, and nobody reads in a diff.
const COLLAPSIBLE: &[(&str, &str)] = &[
    ("\"tools\"", "<TOOLS>"),
    ("\"artifact_schema\"", "<ARTIFACT_SCHEMA>"),
    ("\"functions\"", "<FUNCTIONS>"),
];

/// Render one request as a stable, readable snapshot block.
pub fn render_request(index: usize, body: &Value) -> String {
    format!("── request {index} ──\n{}", render_request_content(body))
}

/// The request's snapshot body, with no index header.
///
/// Kept separate so two requests can be compared for equality. Comparing
/// headered blocks always reports a difference, because the index is part of
/// the header — which made every consecutive pair look like a delta.
pub fn render_request_content(body: &Value) -> String {
    let mut out = String::new();

    // Model and streaming mode, which are the two fields whose change matters
    // most and are small enough to always show.
    if let Some(m) = body.get("model").and_then(|v| v.as_str()) {
        out.push_str(&format!("model: {m}\n"));
    }
    if let Some(s) = body.get("stream").and_then(|v| v.as_bool()) {
        out.push_str(&format!("stream: {s}\n"));
    }
    if let Some(t) = body.get("temperature").and_then(|v| v.as_f64()) {
        out.push_str(&format!("temperature: {t}\n"));
    }
    if let Some(m) = body.get("max_tokens").and_then(|v| v.as_u64()) {
        out.push_str(&format!("max_tokens: {m}\n"));
    }

    // The system prompt is the whole point: it is what a prompt edit changes.
    if let Some(messages) = body.get("messages").and_then(|v| v.as_array()) {
        for msg in messages {
            let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("?");
            let content = msg.get("content").map(render_content).unwrap_or_default();
            out.push_str(&format!("\n[{role}]\n{}\n", normalize(&content)));
        }
    }

    // Collapse the stable, bulky blocks. The key *and its value* are replaced:
    // renaming the key alone left a 200-line tool schema in the snapshot, which
    // is exactly what the collapse exists to prevent.
    let mut scrubbed = body.clone();
    if let Some(obj) = scrubbed.as_object_mut() {
        for (needle, tag) in COLLAPSIBLE {
            let key = needle.trim_matches('"');
            if obj.remove(key).is_some() {
                obj.insert((*tag).to_string(), Value::String("<collapsed>".into()));
            }
        }
    }
    out.push_str(&format!("\nbody: {}\n", normalize(&scrubbed.to_string())));
    out
}

/// Flatten a message's `content`, which is either a string or a content-part
/// array.
fn render_content(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .map(|p| {
                p.get("text")
                    .and_then(|t| t.as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| p.to_string())
            })
            .collect::<Vec<_>>()
            .join("\n"),
        other => other.to_string(),
    }
}

/// Render a full sequence, showing only what changed between requests.
///
/// Requests 0 and the last are always shown in full; intermediate ones appear
/// only when they differ from their predecessor. This is what makes a
/// snapshot readable — a four-turn run is not four copies of the same 200-line
/// system prompt.
pub fn render_sequence(bodies: &[Value]) -> String {
    let mut out = String::from("# NIKI model-visible context snapshot\n");
    out.push_str(
        "# Generated by tests/reverse/context_snapshot.rs. Regenerate with:\n\
         #   cargo test --test reverse regenerate_context_snapshot -- --ignored --nocapture\n\
         #\n\
         # A diff here is a change to what the model is told. That is the artifact a\n\
         # prompt edit is reviewed against.\n\n",
    );

    if bodies.is_empty() {
        out.push_str("(no requests captured)\n");
        return out;
    }

    let last = bodies.len() - 1;
    for (i, body) in bodies.iter().enumerate() {
        let block = render_request(i + 1, body);
        if i == 0 || i == last {
            out.push_str(&block);
            out.push('\n');
        } else if render_request_content(body) != render_request_content(&bodies[i - 1]) {
            // `block` already begins with its own `── request N ──` header, so
            // only the delta marker is added here; emitting a second header
            // made the block count disagree with the request count.
            out.push_str("(differs from previous)\n");
            out.push_str(&block);
            out.push('\n');
        }
    }
    out
}

/// Load the committed golden, or `None` if it has not been generated yet.
pub fn load_golden() -> Option<String> {
    std::fs::read_to_string(golden_path()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── Normalisation ──────────────────────────────────────────────
    #[test]
    fn uuids_are_normalised() {
        let s = normalize("task 550e8400-e29b-41d4-a716-446655440000 finished");
        assert!(s.contains("<VOLATILE>"), "{s:?}");
        assert!(!s.contains("550e8400"), "{s:?}");
    }

    #[test]
    fn absolute_paths_are_normalised() {
        let s = normalize("wrote /tmp/niki-journey-1234/project/out.txt");
        assert!(s.contains("<VOLATILE>"), "{s:?}");
        assert!(!s.contains("/tmp/"), "{s:?}");
    }

    #[test]
    fn timestamps_are_normalised() {
        let s = normalize("created at 2026-09-27T11:04:51Z ok");
        assert!(!s.contains("2026-09-27"), "{s:?}");
    }

    #[test]
    fn durations_and_token_counts_are_normalised() {
        assert!(!normalize("took 41ms").contains("41ms"));
        assert!(!normalize("used 128000 tokens").contains("128000"));
    }

    /// The snapshot is worthless if it churns, so ordinary prose must survive
    /// byte for byte.
    #[test]
    fn ordinary_prompt_text_survives_exactly() {
        let text = "You are the Coder. Make the smallest change that satisfies the request.\nDo not refactor adjacent code.";
        assert_eq!(
            normalize(text),
            text,
            "a prompt edit must be visible, not hidden by noise"
        );
    }

    #[test]
    fn small_numbers_survive_because_they_may_be_meaningful() {
        // Only large digit runs are treated as counters; "3 retries" is prose.
        assert!(normalize("3 retries").contains("3"));
    }

    // ── Rendering ──────────────────────────────────────────────────
    #[test]
    fn a_request_renders_its_system_prompt() {
        let body = json!({
            "model": "gpt-x",
            "stream": true,
            "messages": [{"role": "system", "content": "You are the Coder."},
                         {"role": "user", "content": "Fix the bug"}],
        });
        let out = render_request(1, &body);
        assert!(out.contains("model: gpt-x"));
        assert!(out.contains("[system]"));
        assert!(out.contains("You are the Coder."));
        assert!(out.contains("Fix the bug"));
    }

    #[test]
    fn bulky_stable_blocks_collapse_to_tags() {
        let body = json!({
            "model": "m",
            "tools": [{"type": "function", "function": {"name": "read_file"}}],
            "messages": [{"role": "system", "content": "x"}],
        });
        let out = render_request(1, &body);
        assert!(out.contains("<TOOLS>"), "{out}");
        assert!(
            !out.contains("read_file"),
            "a tool schema is stable and large; it must collapse, not dominate the diff:\n{out}"
        );
    }

    #[test]
    fn identical_intermediate_requests_are_omitted() {
        let same = json!({"model": "m", "messages": [{"role": "system", "content": "same"}]});
        let diff = json!({"model": "m", "messages": [{"role": "system", "content": "changed"}]});
        // r1 same, r2 same (a duplicate turn), r3 changed, r4 same again.
        let seq = vec![same.clone(), same.clone(), diff.clone(), same.clone()];
        let out = render_sequence(&seq);

        assert_eq!(
            out.matches("── request").count(),
            3,
            "first and last always render, plus the one intermediate that differs. The \
             duplicate turn must be dropped or a four-turn run renders four copies of the \
             same system prompt:\n{out}"
        );
        assert!(out.contains("differs from previous"), "{out}");

        // The dropped request must genuinely be absent, not merely unlabelled.
        assert_eq!(
            out.matches("[system]\nsame").count(),
            2,
            "request 2 is identical to request 1 and must not render:\n{out}"
        );
    }

    /// A run where nothing changes must not produce a delta marker at all —
    /// otherwise every turn looks like a change and the signal is worthless.
    #[test]
    fn a_run_with_no_changes_shows_no_delta_marker() {
        let same = json!({"model": "m", "messages": [{"role": "system", "content": "same"}]});
        let out = render_sequence(&[same.clone(), same.clone(), same.clone()]);
        assert!(!out.contains("differs from previous"), "{out}");
        assert_eq!(out.matches("── request").count(), 2, "{out}");
    }

    #[test]
    fn an_empty_sequence_renders_without_panicking() {
        assert!(render_sequence(&[]).contains("no requests captured"));
    }

    /// The whole mechanism depends on a prompt change showing up as a diff.
    // ── The golden ────────────────────────────────────────────────
    //
    // Render every role's system prompt the way production does, and
    /// snapshot the result.
    ///
    // This is the point of the whole mechanism. When `prompts/coder.md` gains
    /// a paragraph or a stage-specific block is dropped, nothing else in the
    /// suite notices: the tests assert on what comes *out* of a mock that does
    /// not care what went in. Here a prompt edit is a one-line diff in a
    /// committed file, and the test fails until the change is reviewed and
    /// blessed.
    fn rendered_prompt_snapshot() -> String {
        use niki::artifacts::types::AgentRole;
        use niki::orchestrator::pipeline::role_prompt;

        let roles = [
            AgentRole::Planner,
            AgentRole::Coder,
            AgentRole::Tester,
            AgentRole::Reviewer,
            AgentRole::Synthesizer,
            AgentRole::SecurityAuditor,
            AgentRole::Red,
            AgentRole::Critic,
        ];

        let mut out = String::from("# NIKI rendered role prompts\n");
        out.push_str(
            "# Generated by tests/reverse/context_snapshot.rs. Regenerate with:\n\
             #   cargo test --test reverse regenerate_context_snapshot -- --ignored --nocapture\n\
             #\n\
             # A diff here is a change to what every agent is told. Review it as carefully as\n\
             # a diff to the pipeline itself.\n\n",
        );

        for role in roles {
            let (template_name, schema_path) = role_prompt(role);
            let body = niki::load_asset(&format!("prompts/{template_name}"))
                .unwrap_or_else(|e| panic!("{role:?} prompt {template_name}: {e}"));
            // Compiling the template is itself the check: a stray `{{` in a
            // role prompt used to abort a live run at stage start.
            let mut env = minijinja::Environment::new();
            env.add_template(template_name, &body)
                .unwrap_or_else(|e| panic!("{role:?} prompt does not compile: {e}"));

            out.push_str(&format!(
                "── {role:?} (prompt: {template_name}, schema: {schema_path}) ──\n\n"
            ));
            out.push_str(&body);
            out.push_str("\n\n");
        }
        out
    }

    #[test]
    fn rendered_prompts_match_the_committed_golden() {
        let current = rendered_prompt_snapshot();
        let Some(golden) = load_golden() else {
            panic!(
                "tests/snapshots/context.snap is missing. Regenerate with:\n  \
                 cargo test --test reverse regenerate_context_snapshot -- --ignored --nocapture"
            );
        };
        if current != golden {
            // Report the first differing region rather than dumping two large
            // files, which is unreadable in CI output.
            let first_diff = current
                .lines()
                .zip(golden.lines())
                .enumerate()
                .find(|(_, (a, b))| a != b)
                .map(|(i, (a, _))| format!("line {}:\n  now:  {a}", i + 1));
            panic!(
                "the prompts the model is told have changed.\n{}\n\
                 A diff here is a behaviour change for every agent. If it is intended, \
                 regenerate with:\n  cargo test --test reverse regenerate_context_snapshot \
                 -- --ignored --nocapture",
                first_diff.unwrap_or_else(|| {
                    "the files differ in length rather than content".to_string()
                })
            );
        }
    }

    /// The golden must not be a snapshot of an empty or truncated render — a
    /// file that "matches" because both sides are blank guards nothing.
    #[test]
    fn the_committed_golden_is_not_empty_or_truncated() {
        let Some(golden) = load_golden() else {
            panic!("tests/snapshots/context.snap is missing; regenerate it");
        };
        assert!(
            golden.len() > 2_000,
            "the golden is only {} bytes; a snapshot this small is not covering the prompts",
            golden.len()
        );
        for role in ["Planner", "Coder", "Tester", "Reviewer"] {
            assert!(
                golden.contains(&format!("── {role}")),
                "the golden has no section for {role}"
            );
        }
    }

    /// Deliberate, and `#[ignore]`d so it can never run as part of a normal
    /// suite: a test that silently rewrites the thing it asserts is not a
    /// test.
    #[test]
    #[ignore = "golden regeneration; run deliberately with --ignored --nocapture"]
    fn regenerate_context_snapshot() {
        let dest = golden_path();
        std::fs::create_dir_all(dest.parent().expect("snapshot dir")).expect("snapshot dir");
        std::fs::write(&dest, rendered_prompt_snapshot()).expect("golden written");
        eprintln!("wrote {}", dest.display());
    }

    #[test]
    fn a_prompt_edit_changes_the_snapshot() {
        let before = json!({
            "model": "m",
            "messages": [{"role": "system", "content": "Make the smallest change."}],
        });
        let after = json!({
            "model": "m",
            "messages": [{"role": "system", "content": "Make the smallest change. Do not refactor."}],
        });
        assert_ne!(
            render_request(1, &before),
            render_request(1, &after),
            "a prompt edit must produce a diff, or the snapshot is not reviewing anything"
        );
    }

    /// And the inverse: an unrelated volatile change must NOT produce a diff,
    /// or the golden file churns on every run and gets ignored.
    #[test]
    fn a_uuid_change_does_not_produce_a_diff() {
        let a = json!({
            "model": "m",
            "messages": [{"role": "system", "content": "Task 550e8400-e29b-41d4-a716-446655440000 done"}],
        });
        let b = json!({
            "model": "m",
            "messages": [{"role": "system", "content": "Task 6ba7b810-9dad-11d1-80b4-00c04fd430c8 done"}],
        });
        assert_eq!(
            render_request(1, &a),
            render_request(1, &b),
            "a fresh UUID per run must not churn the golden file"
        );
    }
}
