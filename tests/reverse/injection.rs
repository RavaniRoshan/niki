//! Untrusted content must never act on the user's machine.
//!
//! Three attack surfaces, in increasing order of how much they matter:
//!
//! 1. **The terminal.** Model tokens, report output, slash-command bodies and
//!    skill diffs all reach the user's terminal. Before
//!    `display::sanitize` existed, an `ESC ] 52` in any of them overwrote the
//!    system clipboard and `ESC [ 2 J` cleared the scrollback. All of it is
//!    reachable from repository content — a file, an `AGENTS.md`, a slash
//!    command — none of which the user wrote.
//! 2. **Artifacts.** `report.md` and `artifacts/*.json` carry model text. They
//!    are files, so a user who `cat`s one, pipes one into another tool, or
//!    opens one in an editor is exposed to the same sequences.
//! 3. **Privileged roles.** Red/SecurityAuditor/Critic read repository
//!    excerpts. Content there must not be able to change tool permissions or
//!    the pipeline topology.

use niki::display::sanitize::{sanitize_for_terminal, sanitize_line};

/// The payloads an attacker would plant in a repository or coax a model into
/// emitting. Each is a real terminal attack, not a synthetic marker.
pub const ATTACK_PAYLOADS: &[(&str, &str)] = &[
    (
        "osc52-clipboard",
        "\u{1b}]52;c;Y2xpcGVhcmRfZ3JvYmJlGQ==\u{7}",
    ),
    ("osc52-clipboard-st", "\u{1b}]52;c;Y2xpcGJvYXJk\u{1b}\\"),
    ("osc0-title", "\u{1b}]0;pwned-title\u{7}"),
    ("osc2-title", "\u{1b}]2;pwned-title\u{7}"),
    ("csi-clear-scrollback", "\u{1b}[3J"),
    ("csi-clear-screen", "\u{1b}[2J"),
    ("csi-alternate-screen", "\u{1b}[?1049h"),
    ("csi-hide-cursor", "\u{1b}[?25l"),
    ("csi-sgr-recolour", "\u{1b}[38;5;196mALERT\u{1b}[0m"),
    ("charset-shift", "\u{1b}(0"),
    ("carriage-return-overwrite", "safe\r\u{1b}[Koverwritten"),
    ("backspace-overwrite", "safe\u{8}\u{8}\u{8}\u{8}pwned"),
    ("bell", "ding\u{7}"),
    ("nul-truncation", "before\u{0}after"),
    ("nested-osc-in-csi", "\u{1b}[\u{1b}]52;c;eA==\u{7}"),
    ("truncated-osc", "\u{1b}]52;c;Y2xpcGVhcmRfZ3JvYmJl"),
];

fn contains_escape(s: &str) -> bool {
    s.chars().any(|c| {
        c == '\u{1b}'
            || c == '\r'
            || c == '\u{0}'
            || c == '\u{7f}'
            || (c != '\n' && c != '\t' && (c as u32) < 0x20)
    })
}

#[test]
fn every_attack_payload_is_defused_by_the_sanitizer() {
    let mut survivors = Vec::new();
    for (name, payload) in ATTACK_PAYLOADS {
        let out = sanitize_line(payload);
        if contains_escape(&out) {
            survivors.push(format!(
                "{name}: still contains a control character ({out:?})"
            ));
        }
    }
    assert!(
        survivors.is_empty(),
        "these payloads still carry terminal control after sanitising:\n{}",
        survivors.join("\n")
    );
}

#[test]
fn the_sanitizer_never_leaks_the_clipboard_payload() {
    let out = sanitize_for_terminal(ATTACK_PAYLOADS[0].1);
    assert!(!out.contains("52;c"), "{out:?}");
    assert!(!out.contains("Y2xpcGVhcmRfZ3JvYmJl"), "{out:?}");
}

#[test]
fn benign_content_is_not_mangled() {
    // A sanitizer that eats real output is its own kind of data loss, so the
    // pass-through cases are asserted as loudly as the attack cases.
    for text in [
        "diff --git a/src/lib.rs b/src/lib.rs",
        "let end = start + size;",
        "héllo ✅ 世界 🌍",
        "```rust\nfn main() {}\n```",
        "Tests: 8/8 passed",
        "error: expected `;`, found `}`",
    ] {
        assert_eq!(
            sanitize_for_terminal(text),
            text,
            "benign content was modified: {text:?}"
        );
    }
}

#[test]
fn sanitizing_is_idempotent() {
    // Streaming sends overlapping chunks; a payload split across two tokens
    // must not survive reassembly.
    for (_, payload) in ATTACK_PAYLOADS {
        let once = sanitize_for_terminal(payload);
        let twice = sanitize_for_terminal(&once);
        assert_eq!(once, twice, "sanitising is not idempotent for {payload:?}");
    }
}

#[test]
fn a_payload_split_across_stream_chunks_is_still_defused() {
    // The streaming path emits token by token, so an escape sequence can be
    // split mid-sequence. Reassembling the sanitised pieces must still be safe.
    for (_, payload) in ATTACK_PAYLOADS {
        let bytes = payload.as_bytes();
        for split in 1..bytes.len() {
            let (a, b) = niki::display::sanitize::split_at(payload, split);
            let joined = format!("{}{}", sanitize_for_terminal(&a), sanitize_for_terminal(&b));
            assert!(
                !contains_escape(&joined),
                "payload split at {split} reassembled into an unsafe string: {joined:?}"
            );
        }
    }
}

/// Code-shape audit: every place NIKI prints model- or repository-derived text
/// must route through the sanitizer. This is the test that stops the next
/// `print!` from quietly reintroducing the vulnerability.
#[test]
fn no_untrusted_print_bypasses_the_sanitizer() {
    // Paths that print content which originates outside the program. Anything
    // listed here MUST either sanitise, or be a fixed prompt with no
    // interpolated external data.
    const GUARDED: &[&str] = &[
        "src/display/agent_stream.rs", // streamed model tokens
        "src/cli/report.rs",           // report.md: task description + diff
        "src/cli/commands.rs",         // repo-controllable slash-command bodies
        "src/cli/skills.rs",           // skill file diff
    ];

    for path in GUARDED {
        let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
        let body = std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("read {path}: {e}"));

        // Any print!/println! that interpolates a value (contains `{`) must be
        // inside a sanitize call. Multi-line macro invocations are folded into
        // one logical unit first, so a call split across lines is not skipped.
        for (i, stmt) in fold_print_calls(&body).into_iter().enumerate() {
            let line = stmt.trim();
            let is_interpolating_print = (line.starts_with("print!(\"")
                || line.starts_with("println!(\"")
                || line.contains("print!(\"{"))
                && line.contains('{');
            if !is_interpolating_print {
                continue;
            }
            // Accept either the full name or a local `as sanitize` alias.
            let guarded = line.contains("sanitize_for_terminal")
                || line.contains("sanitize_line")
                || line.contains("sanitize(");
            assert!(
                guarded,
                "{path} prints interpolated content without sanitising it (statement {i}):\n  {line}\n\
                 Anything derived from the model or the repository is untrusted input."
            );
        }
    }
}

/// Collapse multi-line `print!(...)` / `println!(...)` invocations into single
/// logical strings, so the audit sees the whole argument list rather than one
/// line of it.
fn fold_print_calls(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut depth = 0usize;
    for line in body.lines() {
        let t = line.trim();
        if depth == 0 && !(t.starts_with("print!(") || t.starts_with("println!(")) {
            out.push(t.to_string());
            continue;
        }
        if depth == 0 {
            buf.clear();
        }
        buf.push_str(t);
        buf.push(' ');
        depth += t.matches('(').count();
        depth = depth.saturating_sub(t.matches(')').count());
        if depth == 0 {
            out.push(std::mem::take(&mut buf));
        }
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    out
}

/// A secret planted anywhere in the pipeline's output must not survive into
/// anything the user reads. `redact_secrets` previously covered provider error
/// strings only.
#[test]
fn the_redactor_covers_the_shapes_agents_leak() {
    use niki::llm::provider::redact_secrets;
    for secret in [
        "sk-abcdefghijklmnopqrstuvwxyz012345",
        "ghp_abcdefghijklmnopqrstuvwxyz0123456789",
        "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9",
    ] {
        let out = redact_secrets(&format!("error calling provider: {secret}"));
        assert!(!out.contains(secret), "secret survived redaction: {out:?}");
    }
}

#[cfg(test)]
mod payload_selftest {
    use super::*;

    /// The payload list must actually contain dangerous input, or the tests
    /// above would pass on an empty corpus — the exact vacuity this harness
    /// exists to prevent.
    #[test]
    fn the_payload_corpus_is_actually_dangerous() {
        assert!(
            ATTACK_PAYLOADS.len() >= 15,
            "the corpus shrank to {} entries",
            ATTACK_PAYLOADS.len()
        );
        for (name, payload) in ATTACK_PAYLOADS {
            assert!(
                contains_escape(payload),
                "payload `{name}` is not actually dangerous — the corpus would be testing nothing"
            );
        }
    }
}
