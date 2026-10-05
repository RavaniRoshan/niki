//! ATIF — Agent Trajectory Interchange Format, as Terminal-Bench's leaderboard consumes it.
//!
//! **This is not a second protocol.** `niki-protocol` remains the only message format NIKI and
//! the shell speak. ATIF is an external, read-only interop format: NIKI writes one
//! `trajectory.json` at the end of a run so an external harness can read what happened, and
//! nothing in the engine ever reads one back.
//!
//! Why it exists at all: the leaderboard requires an ATIF trajectory for every rewarded trial,
//! and a harness that validates it is the only thing standing between "the agent said it
//! finished" and "there is a record of what it actually did". A run that cannot produce one is
//! not claimable.
//!
//! What it is built from: `events.jsonl`, which every run already writes and which carries a
//! timestamp, a turn, a step and a payload for each runtime event, plus the `StageMetric`
//! records the pipeline already keeps. Nothing here is invented or back-filled — a field the
//! run did not produce is absent rather than zero-filled, because a zero in a trajectory reads
//! as a measurement.

use crate::llm::provider::redact_secrets;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The schema this writer emits.
pub const ATIF_VERSION: &str = "ATIF-v1.7";

fn schema_version() -> &'static str {
    ATIF_VERSION
}

/// Who produced the trajectory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtifAgent {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
}

/// Token and money accounting for one step or for the whole run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AtifMetrics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

/// One recorded event in the trajectory.
///
/// `step_id` is 1-based and strictly sequential: the validator rejects a gap, and a gap would
/// mean a step was dropped on the way out.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtifStep {
    pub step_id: u32,
    /// `system`, `user` or `agent`. Agent-only fields are rejected on the other two, so the
    /// variant is not decoration.
    pub source: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<AtifMetrics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_call_count: Option<u32>,
}

/// Whole-run totals.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AtifFinalMetrics {
    #[serde(default)]
    pub total_prompt_tokens: u32,
    #[serde(default)]
    pub total_completion_tokens: u32,
    #[serde(default)]
    pub total_cached_tokens: u32,
    #[serde(default)]
    pub total_cost_usd: f64,
    #[serde(default)]
    pub total_steps: u32,
}

/// A complete trajectory document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtifTrajectory {
    pub atif_version: String,
    pub agent: AtifAgent,
    pub steps: Vec<AtifStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_metrics: Option<AtifFinalMetrics>,
}

impl AtifTrajectory {
    /// Start a trajectory for a run.
    pub fn new(agent_name: &str, agent_version: &str, model_name: Option<String>) -> Self {
        Self {
            atif_version: schema_version().to_string(),
            agent: AtifAgent {
                name: agent_name.to_string(),
                version: agent_version.to_string(),
                model_name,
            },
            steps: Vec::new(),
            final_metrics: None,
        }
    }

    /// Append a step, assigning the next sequential id.
    ///
    /// The id is assigned here rather than by the caller so a caller cannot produce a gap by
    /// accident, and so a caller that forgets cannot produce a duplicate.
    pub fn push(
        &mut self,
        source: &str,
        message: impl Into<String>,
        timestamp: Option<String>,
    ) -> &mut AtifStep {
        let step_id = self.steps.len() as u32 + 1;
        self.steps.push(AtifStep {
            step_id,
            source: source.to_string(),
            message: message.into(),
            timestamp,
            model_name: None,
            tool_calls: None,
            metrics: None,
            llm_call_count: None,
        });
        self.steps.last_mut().expect("just pushed")
    }

    /// The most recently pushed step, if any.
    pub fn last_step_mut(&mut self) -> Option<&mut AtifStep> {
        self.steps.last_mut()
    }

    /// Compute `final_metrics` from the per-step metrics actually recorded.
    ///
    /// Sums what is present. A run that reported nothing reports zero totals and the caller
    /// should then not claim the trajectory is priced — `priced` says which it was.
    pub fn finalize(&mut self, priced: bool) {
        let mut totals = AtifFinalMetrics {
            total_steps: self.steps.len() as u32,
            ..Default::default()
        };
        let mut saw_usage = false;
        for step in &self.steps {
            let Some(m) = &step.metrics else { continue };
            if m.prompt_tokens.is_some() {
                saw_usage = true;
            }
            totals.total_prompt_tokens += m.prompt_tokens.unwrap_or(0);
            totals.total_completion_tokens += m.completion_tokens.unwrap_or(0);
            totals.total_cached_tokens += m.cached_tokens.unwrap_or(0);
            totals.total_cost_usd += m.cost_usd.unwrap_or(0.0);
        }
        if !priced && !saw_usage {
            totals.total_cost_usd = 0.0;
        }
        self.final_metrics = Some(totals);
    }

    /// Serialize to pretty JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Write the trajectory to `path`, creating parent directories.
    ///
    /// Written through a temporary file and renamed, so a harness that is watching for the file
    /// never sees a half-written document — and a crashed run leaves no file that parses as
    /// valid but is not.
    pub fn write_to(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.partial");
        std::fs::write(&tmp, self.to_json()?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

/// Read a journal written by `JournalEventSink` and turn it into ATIF steps.
///
/// Every journal line becomes one agent step, in the order it was written. Nothing is filtered:
/// a harness reading this must see the failures, not a tidy summary.
pub fn steps_from_journal(journal: &str, trajectory: &mut AtifTrajectory) -> usize {
    let mut added = 0usize;
    for line in journal.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            // A journal that cannot be parsed is itself information, but inventing a step for it
            // would put text in a trajectory that no run produced. Skip it; the count below is
            // what the caller reports.
            continue;
        };
        let kind = event
            .get("event_kind")
            .and_then(|k| k.as_str())
            .unwrap_or("Unknown");
        let timestamp = event
            .get("timestamp")
            .and_then(|t| t.as_str())
            .map(str::to_string);
        let payload = event
            .get("payload")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        // A trajectory is a file that leaves the machine, so the same redaction every log line
        // gets applies here. Tool arguments routinely carry whatever the model was looking at,
        // and a model that was shown a key will put it in a command line.
        let mut payload = payload;
        redact_in_place(&mut payload);
        let message = redact_secrets(&render_event(kind, &payload));

        let step = trajectory.push("agent", message, timestamp);

        // Tool calls are declared on agent steps only, and only when the journal actually
        // reported one.
        if kind == "ToolCallRequested" || kind == "ToolExecutionCompleted" {
            if let Some(name) = payload.get("tool_name").and_then(|n| n.as_str()) {
                let mut call = serde_json::json!({
                    "id": event.get("step_id").and_then(|s| s.as_str()).unwrap_or_default(),
                    "name": name,
                });
                if let Some(args) = payload.get("arguments") {
                    call["arguments"] = args.clone();
                }
                if let Some(ok) = payload.get("success").and_then(|s| s.as_bool()) {
                    call["success"] = serde_json::Value::Bool(ok);
                }
                step.tool_calls = Some(vec![call]);
            }
        }

        // Per-step usage, when the journal carried any.
        let prompt = payload
            .get("input_tokens")
            .and_then(serde_json::Value::as_u64)
            .map(|v| v as u32);
        let completion = payload
            .get("output_tokens")
            .and_then(serde_json::Value::as_u64)
            .map(|v| v as u32);
        if prompt.is_some() || completion.is_some() {
            step.metrics = Some(AtifMetrics {
                prompt_tokens: prompt,
                completion_tokens: completion,
                cached_tokens: payload
                    .get("cached_input_tokens")
                    .and_then(serde_json::Value::as_u64)
                    .map(|v| v as u32),
                cost_usd: payload.get("cost_usd").and_then(serde_json::Value::as_f64),
            });
        }
        if kind == "ModelRequestStarted" {
            step.llm_call_count = Some(1);
        }
        added += 1;
    }
    added
}

/// Render one journal event as a line of readable text.
///
/// The message has to say what happened without the reader needing the journal's schema, and it
/// has to say it for failures too — a trajectory where every line reads "ok" is not a record.
fn render_event(kind: &str, payload: &serde_json::Value) -> String {
    let get = |k: &str| payload.get(k).and_then(serde_json::Value::as_str);
    match kind {
        "SessionStarted" => format!("session started ({})", field(payload, "session_id")),
        "SessionResumed" => "session resumed".to_string(),
        "TurnStarted" => format!("turn started ({})", field(payload, "turn_id")),
        "ContextBuilt" => format!("context built: {}", truncate(&payload.to_string(), 400)),
        "ModelRequestStarted" => format!(
            "model request: model={} input_tokens={}",
            get("model").unwrap_or("unknown"),
            payload
                .get("input_tokens")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
        ),
        "ModelResponseStarted" => "model response started".to_string(),
        "ModelResponseCompleted" => format!(
            "model response complete: output_tokens={} cost_usd={}",
            payload
                .get("output_tokens")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
            payload
                .get("cost_usd")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0)
        ),
        "ToolCallRequested" => format!(
            "tool call requested: {}({})",
            get("tool_name").unwrap_or("unknown"),
            truncate(
                &payload
                    .get("arguments")
                    .cloned()
                    .unwrap_or_default()
                    .to_string(),
                400
            )
        ),
        "ToolExecutionStarted" => {
            format!("tool running: {}", get("tool_name").unwrap_or("unknown"))
        }
        "ToolExecutionCompleted" => format!("tool ok: {}", get("tool_name").unwrap_or("unknown")),
        "ToolExecutionFailed" => format!(
            "tool FAILED: {} — {}",
            get("tool_name").unwrap_or("unknown"),
            get("error").unwrap_or("no reason recorded")
        ),
        "ArtifactProduced" => format!(
            "artifact produced: {} ({} bytes)",
            get("role").unwrap_or("unknown"),
            payload
                .get("bytes")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
        ),
        "TestStarted" => "tests started".to_string(),
        "TestCompleted" => format!(
            "tests complete: {} passed",
            payload
                .get("passed")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        ),
        "CompactionStarted" => "context compaction started".to_string(),
        "CompactionCompleted" => "context compaction complete".to_string(),
        "CheckpointCreated" => "checkpoint created".to_string(),
        "TurnCompleted" => "turn complete".to_string(),
        "SessionCompleted" => "session complete".to_string(),
        "SessionFailed" => format!(
            "session FAILED: {}",
            get("error").unwrap_or("no reason recorded")
        ),
        other => format!("{other}: {}", truncate(&payload.to_string(), 400)),
    }
}

/// Redact every string in a payload, in place.
///
/// Applied to the whole event rather than to a few named keys, because the keys a secret can
/// appear in are exactly the ones nobody enumerated.
fn redact_in_place(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(s) => *s = redact_secrets(s),
        serde_json::Value::Array(items) => {
            for item in items.iter_mut() {
                redact_in_place(item);
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values_mut() {
                redact_in_place(v);
            }
        }
        _ => {}
    }
}

fn field(payload: &serde_json::Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown")
        .to_string()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max).collect();
    format!("{head}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_ids_are_sequential_from_one() {
        let mut t = AtifTrajectory::new("niki", "0.10.0", None);
        for i in 0..5 {
            t.push("agent", format!("event {i}"), None);
        }
        let ids: Vec<u32> = t.steps.iter().map(|s| s.step_id).collect();
        assert_eq!(ids, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn final_metrics_sum_only_what_was_reported() {
        let mut t = AtifTrajectory::new("niki", "0.10.0", None);
        t.push("agent", "a", None).metrics = Some(AtifMetrics {
            prompt_tokens: Some(100),
            completion_tokens: Some(20),
            cached_tokens: None,
            cost_usd: Some(0.01),
        });
        t.push("agent", "b", None).metrics = Some(AtifMetrics {
            prompt_tokens: Some(50),
            completion_tokens: None,
            cached_tokens: None,
            cost_usd: None,
        });
        t.push("agent", "c", None); // no metrics at all
        t.finalize(true);
        let m = t.final_metrics.clone().expect("finalized");
        assert_eq!(m.total_prompt_tokens, 150);
        assert_eq!(m.total_completion_tokens, 20);
        assert_eq!(m.total_steps, 3);
        assert!((m.total_cost_usd - 0.01).abs() < 1e-9);
    }

    #[test]
    fn a_run_with_no_reported_usage_reports_zero_and_says_so() {
        let mut t = AtifTrajectory::new("niki", "0.10.0", None);
        t.push("agent", "a", None);
        t.finalize(false);
        let m = t.final_metrics.clone().expect("finalized");
        assert_eq!(m.total_prompt_tokens, 0);
        assert_eq!(m.total_cost_usd, 0.0);
    }

    #[test]
    fn the_serialized_document_declares_its_version_and_carries_the_steps() {
        let mut t = AtifTrajectory::new("niki", "0.10.0", Some("mock-model".into()));
        t.push("user", "do the thing", Some("2026-10-05T00:00:00Z".into()));
        t.push("agent", "did it", None);
        let v: serde_json::Value =
            serde_json::from_str(&t.to_json().expect("json")).expect("parse");
        assert_eq!(v["atif_version"], ATIF_VERSION);
        assert_eq!(v["agent"]["name"], "niki");
        assert_eq!(v["agent"]["model_name"], "mock-model");
        assert_eq!(v["steps"].as_array().expect("array").len(), 2);
        assert_eq!(v["steps"][0]["step_id"], 1);
        assert_eq!(v["steps"][0]["source"], "user");
    }

    #[test]
    fn an_absent_field_is_absent_rather_than_zero() {
        let mut t = AtifTrajectory::new("niki", "0.10.0", None);
        t.push("agent", "x", None).metrics = Some(AtifMetrics {
            prompt_tokens: None,
            completion_tokens: None,
            cached_tokens: None,
            cost_usd: None,
        });
        let v: serde_json::Value =
            serde_json::from_str(&t.to_json().expect("json")).expect("parse");
        let m = &v["steps"][0]["metrics"];
        assert!(
            m.get("prompt_tokens").is_none(),
            "an unreported count must not read as 0"
        );
        assert!(
            m.get("cost_usd").is_none(),
            "an unpriced step must not read as free"
        );
    }

    #[test]
    fn a_journal_becomes_steps_in_order_with_the_failures_intact() {
        let journal = r#"{"session_id":"s1","turn_id":"t1","step_id":"1","timestamp":"2026-10-05T00:00:00Z","event_kind":"ToolCallRequested","payload":{"tool_name":"bash","arguments":{"cmd":"ls"}}}
{"session_id":"s1","turn_id":"t1","step_id":"2","timestamp":"2026-10-05T00:00:01Z","event_kind":"ToolExecutionFailed","payload":{"tool_name":"bash","error":"exit 1"}}
{"session_id":"s1","turn_id":"t1","step_id":"3","timestamp":"2026-10-05T00:00:02Z","event_kind":"ModelResponseCompleted","payload":{"output_tokens":42,"cost_usd":0.002}}
"#;
        let mut t = AtifTrajectory::new("niki", "0.10.0", None);
        let n = steps_from_journal(journal, &mut t);
        assert_eq!(n, 3);
        assert_eq!(t.steps.len(), 3);
        assert!(
            t.steps[1].message.contains("FAILED"),
            "the failure must survive: {}",
            t.steps[1].message
        );
        assert!(t.steps[1].message.contains("exit 1"));
        let calls = t.steps[0].tool_calls.as_ref().expect("tool call recorded");
        assert_eq!(calls[0]["name"], "bash");
        assert_eq!(
            t.steps[2]
                .metrics
                .as_ref()
                .expect("usage")
                .completion_tokens,
            Some(42)
        );
    }

    #[test]
    fn a_secret_in_the_journal_does_not_reach_the_trajectory() {
        // A trajectory is a file that leaves the machine, and a tool argument routinely carries
        // whatever the model was looking at. This is the case that makes an unredacted export a
        // credential on disk.
        let journal = r#"{"event_kind":"ToolCallRequested","payload":{"tool_name":"bash","arguments":{"cmd":"curl -H 'Authorization: Bearer sk-ant-api03-SECRETVALUE' https://api.example.com"}}}
{"event_kind":"ToolCallRequested","payload":{"tool_name":"bash","arguments":{"cmd":"OPENAI_API_KEY=sk-proj-ABCDEFGHIJKLMNOPQRST export OPENAI_API_KEY"}}}
"#;
        let mut t = AtifTrajectory::new("niki", "0.10.0", None);
        steps_from_journal(journal, &mut t);
        let json = t.to_json().expect("json");
        assert!(
            !json.contains("SECRETVALUE"),
            "a bearer token reached the trajectory: {json}"
        );
        assert!(
            !json.contains("ABCDEFGHIJKLMNOPQRST"),
            "an API key reached the trajectory: {json}"
        );
        // The tool call itself must survive; a redacted trajectory with no tool calls is a
        // trajectory that cannot be read.
        assert!(
            t.steps[0]
                .tool_calls
                .as_ref()
                .is_some_and(|c| c[0]["name"] == "bash"),
            "redaction destroyed the tool call record"
        );
    }

    #[test]
    fn a_journal_line_that_is_not_json_is_skipped_rather_than_invented() {
        let mut t = AtifTrajectory::new("niki", "0.10.0", None);
        let n = steps_from_journal("not json\n\n{\"event_kind\":\"TurnStarted\"}\n", &mut t);
        assert_eq!(n, 1);
        assert_eq!(t.steps.len(), 1);
    }

    #[test]
    fn write_to_leaves_no_partial_file_behind() {
        let mut t = AtifTrajectory::new("niki", "0.10.0", None);
        t.push("agent", "x", None);
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nested/deep/trajectory.json");
        t.write_to(&path).expect("write");
        assert!(path.exists());
        assert!(!path.with_extension("json.partial").exists());
        let read: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("parse");
        assert_eq!(read["steps"].as_array().expect("array").len(), 1);
    }
}
