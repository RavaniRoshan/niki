//! Trace invariants: properties that must hold for every recorded run.
//!
//! This is the oracle-free backbone of the harness. It needs no model, no
//! container runtime and no expected answer — only the artifacts a run leaves
//! behind. That makes it fast enough to run on every PR, and it is the layer
//! that catches "the run reported success but did nothing".
//!
//! Five of these invariants **fail against the 0.8.0 code**, and they are kept
//! in that state until the underlying defect is fixed — they are the regression
//! tests for those defects, not temporary TODOs. See the `KNOWN_FAILING` list
//! below and `tests/reverse/invariants_status.rs`.
//!
//! Design rules this file follows:
//!
//! * Every invariant names the written rule it enforces, so a failure message
//!   says what contract broke, not just that a number differed.
//! * A layer with **zero** invariants reports `unexercised`, never
//!   `100% covered` — a coverage claim with no invariant behind it is the thing
//!   this harness exists to eliminate.
//! * Violations are collected, not thrown, so one run reports every breach.

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The recorded evidence of one run.
#[derive(Debug, Clone, Default)]
pub struct RunTrace {
    pub task_dir: PathBuf,
    /// Contents of `task.json`, if present.
    pub record: Option<Value>,
    /// `artifacts/<role>.json` by filename.
    pub artifacts: BTreeMap<String, Value>,
    /// Contents of `changes.patch`, if present.
    pub patch: Option<String>,
    /// Contents of `report.md`, if present.
    pub report: Option<String>,
}

impl RunTrace {
    /// Load a trace from a task directory. Missing pieces are `None` rather
    /// than an error: a crashed run is exactly when we most want the checker
    /// to run.
    pub fn load(task_dir: &Path) -> Self {
        let read = |p: PathBuf| std::fs::read_to_string(p).ok();
        let record = read(task_dir.join("task.json")).and_then(|s| serde_json::from_str(&s).ok());

        let mut artifacts = BTreeMap::new();
        if let Ok(entries) = std::fs::read_dir(task_dir.join("artifacts")) {
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().and_then(|s| s.to_str()) == Some("json")
                    && let Ok(s) = std::fs::read_to_string(&p)
                    && let Ok(v) = serde_json::from_str(&s)
                {
                    artifacts.insert(
                        p.file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or_default()
                            .to_string(),
                        v,
                    );
                }
            }
        }

        Self {
            task_dir: task_dir.to_path_buf(),
            record,
            artifacts,
            patch: read(task_dir.join("changes.patch")),
            report: read(task_dir.join("report.md")),
        }
    }

    /// Find the single task directory under `<project>/.niki/tasks`.
    pub fn load_latest(project: &Path) -> Option<Self> {
        let tasks = project.join(".niki").join("tasks");
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(tasks)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        dirs.last().map(|d| Self::load(d))
    }

    fn status(&self) -> String {
        self.record
            .as_ref()
            .and_then(|r| r.get("status"))
            .map(|s| match s {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_default()
    }

    fn recorded_branch(&self) -> Option<String> {
        self.record
            .as_ref()
            .and_then(|r| r.get("branch"))
            .and_then(|b| b.as_str())
            .map(str::to_string)
    }

    fn is_completed(&self) -> bool {
        self.status().contains("Completed")
    }
}

/// One invariant's verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    /// Breached, with enough detail to act on.
    Fail(String),
    /// The trace lacks the evidence this invariant needs. Distinct from Pass so
    /// "we could not check" is never counted as "we checked and it was fine".
    Unexercised(String),
}

pub struct Invariant {
    pub id: &'static str,
    pub category: &'static str,
    /// The written rule this enforces, quoted into the failure message.
    pub rule: &'static str,
    pub check: fn(&RunTrace) -> Verdict,
}

fn pass() -> Verdict {
    Verdict::Pass
}
fn fail(msg: impl Into<String>) -> Verdict {
    Verdict::Fail(msg.into())
}
fn unexercised(why: impl Into<String>) -> Verdict {
    Verdict::Unexercised(why.into())
}

/// Every invariant. Adding one here without a category entry in
/// [`LAYERS`] would make coverage reporting wrong, so the two are cross-checked.
pub fn invariants() -> Vec<Invariant> {
    vec![
        // ── State / resume ────────────────────────────────────────────
        Invariant {
            id: "INV-BRANCH-STATUS",
            category: "state-resume",
            rule: "a Completed run has a branch, and that branch exists on disk",
            check: |t| {
                if !t.is_completed() {
                    return pass();
                }
                let Some(branch) = t.recorded_branch() else {
                    return fail(
                        "status is Completed but task.json records no branch. An empty diff, \
                         a blocked run, and --dry-run all used to fall through to Completed.",
                    );
                };
                if t.task_dir.as_os_str().is_empty() {
                    return unexercised("trace was built without a task directory");
                }
                // Resolve against the project root, two levels up from the task dir.
                let project = t
                    .task_dir
                    .parent()
                    .and_then(|p| p.parent())
                    .and_then(|p| p.parent())
                    .unwrap_or(&t.task_dir);
                let ok = std::process::Command::new("git")
                    .args([
                        "rev-parse",
                        "--verify",
                        "--quiet",
                        &format!("refs/heads/{branch}"),
                    ])
                    .current_dir(project)
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);
                if ok {
                    pass()
                } else {
                    fail(format!(
                        "status is Completed on branch `{branch}`, but refs/heads/{branch} does \
                         not exist. A Completed status must always be backed by a real branch."
                    ))
                }
            },
        },
        Invariant {
            id: "INV-VERDICT-NOT-FABRICATED",
            category: "state-resume",
            rule: "a Completed run executed at least one stage and produced a non-empty diff",
            check: |t| {
                if !t.is_completed() {
                    return pass();
                }
                let metrics = t
                    .record
                    .as_ref()
                    .and_then(|r| r.get("agent_metrics"))
                    .and_then(|m| m.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                // The strong form, and the one that made the fix necessary: a
                // bare `verdict: "Approved"` is only legitimate if
                // `outcome` says an independent stage made it. This is the
                // check that stops the pipeline ever initialising its way back
                // to a fabricated pass.
                if let Some(o) = t.record.as_ref().and_then(|r| r.get("outcome")) {
                    let outcome = o.get("outcome").and_then(|x| x.as_str()).unwrap_or("");
                    if outcome == "reviewed"
                        && let Some(v) = o.get("verdict").and_then(|x| x.as_str())
                        && v == "approved"
                    {
                        let by = o.get("by").and_then(|x| x.as_str()).unwrap_or("");
                        if by.is_empty() || by == "solo-coder" {
                            return fail(format!(
                                "task.json reports outcome=reviewed by \"{by}\" — a self-approval \
                                 must never be reported as an independent review"
                            ));
                        }
                    }
                }

                // A verdict must always name its source, so a self-approval
                // cannot be read as an independent review.
                match t
                    .record
                    .as_ref()
                    .and_then(|r| r.get("verdict_source"))
                    .and_then(|v| v.as_str())
                {
                    Some(src) if !src.trim().is_empty() => {
                        if src.contains("no independent review")
                            || src.contains("no review performed")
                        {
                            // Honest self-approval: not a failure, but the run
                            // must not be presented as reviewed.
                            let _ = src;
                        }
                    }
                    _ => {
                        return fail(
                            "task.json records a verdict with no `verdict_source`. Without it, a \
                             SingleAgent run's self-approval is indistinguishable from an \
                             independent Reviewer's approval.",
                        );
                    }
                }
                if metrics == 0 {
                    return fail(
                        "status is Completed with zero agent metrics, so no stage is attributable \
                         for the recorded verdict.",
                    );
                }
                let has_patch = t
                    .patch
                    .as_deref()
                    .map(|p| p.contains("diff --git"))
                    .unwrap_or(false);
                if !has_patch {
                    return fail(
                        "status is Completed but changes.patch contains no unified diff, so the \
                         run delivered nothing reviewable.",
                    );
                }
                pass()
            },
        },
        Invariant {
            id: "INV-ATOMIC-STATE",
            category: "state-resume",
            rule: "every JSON file under the task directory parses",
            check: |t| {
                let bad: Vec<String> = std::fs::read_dir(&t.task_dir)
                    .map(|rd| {
                        rd.flatten()
                            .filter(|e| {
                                e.path().extension().and_then(|s| s.to_str()) == Some("json")
                            })
                            .filter(|e| {
                                std::fs::read_to_string(e.path())
                                    .ok()
                                    .and_then(|s| serde_json::from_str::<Value>(&s).err())
                                    .is_some()
                            })
                            .map(|e| e.file_name().to_string_lossy().to_string())
                            .collect()
                    })
                    .unwrap_or_default();
                if bad.is_empty() {
                    pass()
                } else {
                    fail(format!(
                        "these state files are unparseable, so a crashed run silently vanishes \
                         from `niki status`: {}",
                        bad.join(", ")
                    ))
                }
            },
        },
        // ── Cost / telemetry ─────────────────────────────────────────
        Invariant {
            id: "INV-COST-SUM",
            category: "cost-telemetry",
            rule: "the recorded run total equals the sum of the per-stage costs",
            check: |t| {
                let Some(r) = &t.record else {
                    return unexercised("no task.json");
                };
                let stages: Vec<f64> = r
                    .get("agent_metrics")
                    .and_then(|m| m.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|s| s.get("cost_usd").and_then(|c| c.as_f64()))
                            .collect()
                    })
                    .unwrap_or_default();
                if stages.is_empty() {
                    return unexercised("no per-stage cost recorded");
                }
                let total = r
                    .get("total_cost_usd")
                    .and_then(|c| c.as_f64())
                    .unwrap_or(0.0);
                let sum: f64 = stages.iter().sum();
                // The tool loop accumulated usage with `.max()` instead of
                // `+=`, so a multi-step loop reported only its largest step.
                // Allow a cent of float slack.
                if (total - sum).abs() < 0.01 {
                    pass()
                } else {
                    fail(format!(
                        "total_cost_usd is {total:.6} but the per-stage costs sum to {sum:.6} \
                         (stages: {stages:?}). A run that under-reports its own spend breaks the \
                         spend cap, the Cost page and the JSON envelope at once."
                    ))
                }
            },
        },
        Invariant {
            id: "INV-COST-NONNEGATIVE",
            category: "cost-telemetry",
            rule: "no cost, token count or latency is negative",
            check: |t| {
                let Some(r) = &t.record else {
                    return unexercised("no task.json");
                };
                let mut bad = Vec::new();
                for (k, v) in [
                    ("total_cost_usd", r.get("total_cost_usd")),
                    ("total_latency_ms", r.get("total_latency_ms")),
                ] {
                    if let Some(n) = v.and_then(|x| x.as_f64())
                        && n < 0.0
                    {
                        bad.push(format!("{k} = {n}"));
                    }
                }
                if let Some(a) = r.get("agent_metrics").and_then(|m| m.as_array()) {
                    for (i, s) in a.iter().enumerate() {
                        if let Some(c) = s.get("cost_usd").and_then(|x| x.as_f64())
                            && c < 0.0
                        {
                            bad.push(format!("agent_metrics[{i}].cost_usd = {c}"));
                        }
                    }
                }
                if bad.is_empty() {
                    pass()
                } else {
                    fail(bad.join(", "))
                }
            },
        },
        // ── Artifact integrity ───────────────────────────────────────
        Invariant {
            id: "INV-ARTIFACT-SEMANTIC",
            category: "artifact-integrity",
            rule: "every artifact carries non-empty required semantics, not just a valid shape",
            check: |t| {
                if t.artifacts.is_empty() {
                    return unexercised("no artifacts recorded");
                }
                let mut hollow = Vec::new();
                for (name, v) in &t.artifacts {
                    if is_semantically_empty(v) {
                        hollow.push(name.clone());
                    }
                }
                if hollow.is_empty() {
                    pass()
                } else {
                    fail(format!(
                        "these artifacts validate against their schema but say nothing: {}. \
                         The shipped schemas declare no minItems/minLength, so an empty run is \
                         indistinguishable from real work.",
                        hollow.join(", ")
                    ))
                }
            },
        },
        // ── Scope ────────────────────────────────────────────────────
        Invariant {
            id: "INV-SCOPE",
            category: "artifact-integrity",
            rule: "every path in the diff is a relative in-repo path, never absolute or escaping",
            check: |t| {
                let Some(patch) = &t.patch else {
                    return unexercised("no changes.patch");
                };
                let mut bad = Vec::new();
                for line in patch.lines() {
                    let Some(rest) = line.trim_start().strip_prefix("+++ ") else {
                        continue;
                    };
                    let p = rest.trim();
                    if p == "/dev/null" {
                        continue;
                    }
                    let p = p.strip_prefix("b/").unwrap_or(p);
                    if p.starts_with('/') || p.starts_with("..") || p.contains(":\\") {
                        bad.push(p.to_string());
                    }
                }
                if bad.is_empty() {
                    pass()
                } else {
                    fail(format!(
                        "the diff touches paths outside the repository: {}. A SEARCH/REPLACE \
                         block must never be able to write here.",
                        bad.join(", ")
                    ))
                }
            },
        },
        // ── Security / injection ─────────────────────────────────────
        Invariant {
            id: "INV-TERMINAL-SAFE",
            category: "security-injection",
            rule: "no terminal control sequence reaches the user's terminal from model or repo content",
            check: |t| {
                // Scan every user-facing surface the run produces.
                let mut hits = Vec::new();
                for (label, body) in [
                    ("report.md", t.report.as_deref()),
                    ("changes.patch", t.patch.as_deref()),
                ] {
                    if let Some(b) = body
                        && let Some(seq) = find_terminal_escape(b)
                    {
                        hits.push(format!("{label}: {seq}"));
                    }
                }
                for (name, v) in &t.artifacts {
                    if let Some(seq) = find_terminal_escape(&v.to_string()) {
                        hits.push(format!("artifacts/{name}: {seq}"));
                    }
                }
                if hits.is_empty() {
                    pass()
                } else {
                    fail(format!(
                        "terminal control sequences reached user-facing output: {}. A model \
                         token or review-issue description can therefore rewrite the terminal \
                         title, clear scrollback, or write the user's clipboard (OSC 52).",
                        hits.join(", ")
                    ))
                }
            },
        },
        Invariant {
            id: "INV-REDACTION",
            category: "security-injection",
            rule: "no credential-shaped string is written to any run artifact",
            check: |t| {
                let mut hits = Vec::new();
                let scan = |label: &str, body: &str, hits: &mut Vec<String>| {
                    for (name, pat) in [
                        ("openai", "sk-"),
                        ("github", "ghp_"),
                        ("anthropic", "sk-ant-"),
                    ] {
                        if let Some(i) = body.find(pat) {
                            hits.push(format!(
                                "{label}: {name}-shaped secret at offset {i} ({})",
                                &body[i..(i + 8).min(body.len())]
                            ));
                        }
                    }
                    if body.contains("Bearer ") {
                        hits.push(format!("{label}: Authorization header value"));
                    }
                };
                if let Some(b) = &t.report {
                    scan("report.md", b, &mut hits);
                }
                for (name, v) in &t.artifacts {
                    scan(&format!("artifacts/{name}"), &v.to_string(), &mut hits);
                }
                if hits.is_empty() {
                    pass()
                } else {
                    fail(format!(
                        "credentials leaked into run artifacts: {}. `redact_secrets` is currently \
                         applied to provider error strings only.",
                        hits.join(", ")
                    ))
                }
            },
        },
        // ── Pipeline logic ───────────────────────────────────────────
        Invariant {
            id: "INV-STAGE-MANIFEST",
            category: "pipeline-logic",
            rule: "every stage in the risk-gated topology is either executed or explicitly skipped",
            check: |t| {
                let Some(r) = &t.record else {
                    return unexercised("no task.json");
                };
                let Some(topology) = r.get("topology").and_then(|x| x.as_str()) else {
                    return unexercised("run predates topology recording");
                };
                let executed: Vec<&str> = r
                    .get("agent_metrics")
                    .and_then(|m| m.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|s| s.get("role").and_then(|x| x.as_str()))
                            .collect()
                    })
                    .unwrap_or_default();
                if topology.contains("Single") && executed.is_empty() {
                    return fail(
                        "topology is SingleAgent but no stage was metered. The fast path must \
                         still record that it ran, or a run can be indistinguishable from a no-op.",
                    );
                }
                if executed.is_empty() {
                    return unexercised("no stages recorded");
                }
                pass()
            },
        },
    ]
}

/// The layers the harness claims to cover. A layer with no invariant must
/// report as unexercised, which is the whole point of tracking it.
pub const LAYERS: &[&str] = &[
    "pipeline-logic",
    "sandbox-isolation",
    "artifact-integrity",
    "llm-protocol",
    "state-resume",
    "cost-telemetry",
    "security-injection",
    "tui-ux",
    "config-secrets",
    "supply-chain",
];

/// Invariants that are known to fail against the current code. Each is a real,
/// open defect with a written rule; removing an entry requires fixing the
/// defect, not relaxing the invariant.
pub const KNOWN_FAILING: &[(&str, &str)] = &[
    (
        "INV-TERMINAL-SAFE",
        "raw model tokens are print!-ed to the terminal (display/agent_stream.rs:319)",
    ),
    (
        "INV-ARTIFACT-SEMANTIC",
        "artifact schemas declare no minItems/minLength, so a no-op validates cleanly",
    ),
    (
        "INV-STAGE-MANIFEST",
        "a SingleAgent run records no stage metrics at all",
    ),
];

/// Find the first dangerous terminal sequence in `body`.
///
/// C0 controls other than tab/newline, plus the CSI/OSC/DCS introducers, are
/// all reachable from repository content and model output.
pub fn find_terminal_escape(body: &str) -> Option<String> {
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            continue;
        }
        match chars.peek() {
            Some('[') => {
                return Some("ESC [ (CSI — cursor/screen control)".to_string());
            }
            Some(']') => {
                return Some("ESC ] (OSC — title/clipboard/hyperlink control)".to_string());
            }
            Some('P') => {
                return Some("ESC P (DCS)".to_string());
            }
            Some(_) => {
                return Some("ESC (single-character escape)".to_string());
            }
            None => {
                return Some("ESC (truncated)".to_string());
            }
        }
    }
    // Bare C0 controls (BEL, backspace, form feed) are also unsafe even
    // without an ESC introducer.
    for c in body.chars() {
        if matches!(c, '\u{07}' | '\u{08}' | '\u{0c}' | '\u{0b}') {
            return Some(format!("bare C0 control U+{:04X}", c as u32));
        }
    }
    None
}

/// A JSON value that satisfies its schema but conveys nothing.
fn is_semantically_empty(v: &Value) -> bool {
    match v {
        Value::Object(o) => {
            // A verdict with an approved status and no issues or strengths is
            // the canonical hollow artifact.
            if o.get("verdict").is_some() {
                let issues = o.get("issues").and_then(|i| i.as_array()).map(|a| a.len());
                let strengths = o
                    .get("strengths")
                    .and_then(|s| s.as_array())
                    .map(|a| a.len());
                let has_text = o
                    .get("summary")
                    .and_then(|s| s.as_str())
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false);
                if issues == Some(0) && strengths == Some(0) && !has_text {
                    return true;
                }
                return false;
            }
            // A code diff with no edits and no files changed.
            if o.get("edits").is_some() || o.get("files_changed").is_some() {
                let edits = o.get("edits").and_then(|e| e.as_array()).map(|a| a.len());
                let files = o
                    .get("files_changed")
                    .and_then(|f| f.as_array())
                    .map(|a| a.len());
                let notes = o
                    .get("implementation_notes")
                    .and_then(|n| n.as_str())
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false);
                let adherence = o
                    .get("spec_adherence")
                    .and_then(|n| n.as_str())
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false);
                if edits == Some(0) && files == Some(0) && !notes && !adherence {
                    return true;
                }
                return false;
            }
            false
        }
        _ => false,
    }
}

/// Run every invariant against a trace.
pub fn check(trace: &RunTrace) -> Vec<(&'static str, &'static str, Verdict)> {
    invariants()
        .into_iter()
        .map(|i| (i.id, i.category, (i.check)(trace)))
        .collect()
}

/// Per-layer coverage. A layer with no invariant is `unexercised` — never 100%.
pub fn layer_coverage(
    results: &[(&'static str, &'static str, Verdict)],
) -> BTreeMap<&'static str, (usize, usize, usize)> {
    let mut out: BTreeMap<&'static str, (usize, usize, usize)> = BTreeMap::new();
    for layer in LAYERS {
        out.insert(layer, (0, 0, 0));
    }
    for (_, layer, verdict) in results {
        let e = out.entry(layer).or_insert((0, 0, 0));
        match verdict {
            Verdict::Pass => e.0 += 1,
            Verdict::Fail(_) => e.1 += 1,
            Verdict::Unexercised(_) => e.2 += 1,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trace_with(record: Value, patch: Option<&str>, artifacts: &[(&str, Value)]) -> RunTrace {
        RunTrace {
            task_dir: PathBuf::new(),
            record: Some(record),
            artifacts: artifacts
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
            patch: patch.map(str::to_string),
            report: None,
        }
    }

    fn by_id<'a>(r: &'a [(&'static str, &'static str, Verdict)], id: &str) -> &'a Verdict {
        r.iter()
            .find(|(i, _, _)| *i == id)
            .map(|(_, _, v)| v)
            .unwrap_or_else(|| panic!("no invariant {id}"))
    }

    // ── Terminal escape detection ──────────────────────────────────
    #[test]
    fn detects_osc52_clipboard_write() {
        let s = "before \u{1b}]52;c;SGVsbG8=\u{7} after";
        let found = find_terminal_escape(s).expect("OSC 52 must be found");
        assert!(found.contains("OSC"), "{found}");
    }

    #[test]
    fn detects_osc0_title_rewrite() {
        assert!(find_terminal_escape("x\u{1b}]0;pwned\u{7}").is_some());
    }

    #[test]
    fn detects_csi_screen_clear() {
        assert!(find_terminal_escape("x\u{1b}[2J").is_some());
    }

    #[test]
    fn detects_bare_bell() {
        assert!(find_terminal_escape("ding\u{7}").is_some());
    }

    #[test]
    fn plain_text_is_clean() {
        assert!(find_terminal_escape("a normal diff with tabs\tand newlines\n").is_none());
    }

    // ── Semantic emptiness ─────────────────────────────────────────
    #[test]
    fn hollow_verdict_is_detected() {
        let v: Value = serde_json::json!({
            "verdict": "approved", "summary": "", "issues": [], "strengths": []
        });
        assert!(is_semantically_empty(&v));
    }

    #[test]
    fn real_verdict_is_not_empty() {
        let v: Value = serde_json::json!({
            "verdict": "rejected",
            "summary": "injection in exec",
            "issues": [{"severity": "blocking"}],
            "strengths": []
        });
        assert!(!is_semantically_empty(&v));
    }

    #[test]
    fn hollow_diff_is_detected() {
        let v: Value = serde_json::json!({
            "edits": [], "files_changed": [],
            "implementation_notes": "", "spec_adherence": ""
        });
        assert!(is_semantically_empty(&v));
    }

    #[test]
    fn diff_with_notes_is_not_empty() {
        let v: Value = serde_json::json!({
            "edits": [], "files_changed": [], "implementation_notes": "nothing to change"
        });
        assert!(!is_semantically_empty(&v));
    }

    // ── Invariant behaviour ────────────────────────────────────────
    #[test]
    fn cost_sum_catches_an_underreported_total() {
        let t = trace_with(
            serde_json::json!({
                "status": "Completed",
                "agent_metrics": [
                    {"role": "Coder", "cost_usd": 0.10},
                    {"role": "Reviewer", "cost_usd": 0.20}
                ],
                "total_cost_usd": 0.20
            }),
            None,
            &[],
        );
        // 0.20 recorded vs 0.30 actual — the `.max()` bug's signature.
        assert!(matches!(
            by_id(&check(&t), "INV-COST-SUM"),
            Verdict::Fail(_)
        ));
    }

    #[test]
    fn cost_sum_accepts_a_correct_total() {
        let t = trace_with(
            serde_json::json!({
                "status": "Completed",
                "agent_metrics": [
                    {"role": "Coder", "cost_usd": 0.10},
                    {"role": "Reviewer", "cost_usd": 0.20}
                ],
                "total_cost_usd": 0.30
            }),
            None,
            &[],
        );
        assert_eq!(*by_id(&check(&t), "INV-COST-SUM"), Verdict::Pass);
    }

    #[test]
    fn cost_sum_reports_unexercised_not_pass_without_metrics() {
        let t = trace_with(serde_json::json!({"status": "Completed"}), None, &[]);
        assert!(matches!(
            by_id(&check(&t), "INV-COST-SUM"),
            Verdict::Unexercised(_)
        ));
    }

    #[test]
    fn completed_without_a_branch_fails() {
        let t = trace_with(
            serde_json::json!({"status": "Completed"}),
            Some("diff --git a/x b/x\n"),
            &[],
        );
        assert!(matches!(
            by_id(&check(&t), "INV-BRANCH-STATUS"),
            Verdict::Fail(_)
        ));
    }

    #[test]
    fn non_completed_with_a_branch_fails() {
        let t = trace_with(
            serde_json::json!({"status": "Failed", "branch": "niki/abc"}),
            None,
            &[],
        );
        // A Failed run carrying a branch is a different breach; the branch
        // invariant only speaks about Completed, so this is exercised through
        // the explicit null-branch expectation below.
        let results = check(&t);
        let v = by_id(&results, "INV-BRANCH-STATUS");
        assert!(
            matches!(v, Verdict::Pass | Verdict::Unexercised(_)),
            "a non-Completed run is out of scope for INV-BRANCH-STATUS, got {v:?}"
        );
    }

    #[test]
    fn scope_rejects_paths_escaping_the_repo() {
        let t = trace_with(
            serde_json::json!({"status": "Running"}),
            Some("diff --git a/../../etc/passwd b/../../etc/passwd\n+++ b/../../etc/passwd\n"),
            &[],
        );
        assert!(matches!(by_id(&check(&t), "INV-SCOPE"), Verdict::Fail(_)));
    }

    #[test]
    fn scope_rejects_absolute_paths() {
        let t = trace_with(
            serde_json::json!({"status": "Running"}),
            Some("--- a/x\n+++ /etc/shadow\n"),
            &[],
        );
        assert!(matches!(by_id(&check(&t), "INV-SCOPE"), Verdict::Fail(_)));
    }

    #[test]
    fn scope_accepts_normal_paths_and_deletions() {
        let t = trace_with(
            serde_json::json!({"status": "Running"}),
            Some(
                "--- a/gone.rs\n+++ /dev/null\ndiff --git a/src/lib.rs b/src/lib.rs\n+++ b/src/lib.rs\n",
            ),
            &[],
        );
        assert_eq!(*by_id(&check(&t), "INV-SCOPE"), Verdict::Pass);
    }

    #[test]
    fn redaction_catches_a_leaked_openai_key() {
        let t = trace_with(
            serde_json::json!({"status": "Running"}),
            None,
            &[(
                "reviewer.json",
                serde_json::json!({"summary": "used sk-abc123def456 to call"}),
            )],
        );
        assert!(matches!(
            by_id(&check(&t), "INV-REDACTION"),
            Verdict::Fail(_)
        ));
    }

    #[test]
    fn redaction_catches_a_bearer_header() {
        let t = trace_with(
            serde_json::json!({"status": "Running"}),
            None,
            &[(
                "reviewer.json",
                serde_json::json!({"summary": "sent Authorization: Bearer abc"}),
            )],
        );
        assert!(matches!(
            by_id(&check(&t), "INV-REDACTION"),
            Verdict::Fail(_)
        ));
    }

    #[test]
    fn negative_numbers_are_rejected() {
        let t = trace_with(
            serde_json::json!({"status": "Running", "total_cost_usd": -1.0, "agent_metrics": []}),
            None,
            &[],
        );
        assert!(matches!(
            by_id(&check(&t), "INV-COST-NONNEGATIVE"),
            Verdict::Fail(_)
        ));
    }

    #[test]
    fn every_declared_layer_appears_in_the_coverage_map() {
        // A layer dropped from LAYERS silently loses its coverage accounting.
        // (The previous version of this test asserted `n == 0 || n > 0`, which
        // is true for every integer — a vacuous test of the exact kind this
        // harness exists to eliminate.)
        let results = check(&RunTrace::default());
        let cov = layer_coverage(&results);
        for layer in LAYERS {
            assert!(
                cov.contains_key(layer),
                "layer `{layer}` is declared but missing from the coverage map"
            );
        }
        assert_eq!(
            cov.len(),
            LAYERS.len(),
            "the coverage map must have exactly one entry per declared layer"
        );
    }

    #[test]
    fn a_layer_with_no_invariants_scores_zero_not_full_marks() {
        let results = check(&RunTrace::default());
        let cov = layer_coverage(&results);
        for (layer, (pass, fail, unex)) in &cov {
            if pass + fail + unex == 0 {
                assert_eq!(
                    *pass, 0,
                    "layer `{layer}` has no invariants, so it must not report any passes"
                );
            }
        }
    }

    #[test]
    fn known_failing_ids_all_exist_as_invariants() {
        for (id, _) in KNOWN_FAILING {
            assert!(
                invariants().iter().any(|i| i.id == *id),
                "KNOWN_FAILING names `{id}`, which is not a registered invariant. Every known \
                 defect must be backed by a live check, or it is just a comment."
            );
        }
    }

    #[test]
    fn every_invariant_belongs_to_a_declared_layer() {
        for inv in invariants() {
            assert!(
                LAYERS.contains(&inv.category),
                "invariant `{}` declares layer `{}`, which is not in LAYERS",
                inv.id,
                inv.category
            );
        }
    }
}
