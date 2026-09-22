//! Optional Convex control-plane mirror (P1 scaffold, unwired).
//!
//! Local `.niki/tasks/<uuid>/` files remain the source of truth. When the
//! mirror is enabled (`NIKI_CONTROL_PLANE=mirror` plus a deployment URL), the
//! CLI best-effort mirrors run headers and stage transitions to Convex via
//! HTTP one-shot `POST /api/mutation` calls. Any failure is warn-and-spool:
//! the op returns to the [`Spool`] outbox and local execution continues.
//!
//! This module is intentionally not wired into the pipeline yet. P1 wiring
//! must only call [`Mirror::enqueue`] after local state is already durable.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// Mirror mode selected by `NIKI_CONTROL_PLANE`. Defaults to [`Mode::Local`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// No Convex traffic at all. Full local functionality.
    #[default]
    Local,
    /// Best-effort async mirror alongside full local execution.
    Mirror,
}

/// Mirror configuration. The token is never logged (see [`MirrorConfig`]).
pub struct MirrorConfig {
    /// Deployment base URL, e.g. `https://<deployment>.convex.cloud`.
    pub url: String,
    /// Bearer credential for the Functions HTTP API. Never logged.
    token: String,
    /// Mirror on/off switch.
    pub mode: Mode,
}

impl std::fmt::Debug for MirrorConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MirrorConfig")
            .field("url", &self.url)
            .field("token", &"<redacted>")
            .field("mode", &self.mode)
            .finish()
    }
}

impl MirrorConfig {
    /// Read `NIKI_CONTROL_PLANE` (`mirror` enables), `NIKI_CONVEX_URL` and
    /// `NIKI_CONVEX_TOKEN`. Returns `None` when the mirror is disabled or
    /// incompletely configured — callers treat that as local-only mode.
    pub fn from_env() -> Option<Self> {
        let mode = match std::env::var("NIKI_CONTROL_PLANE")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "mirror" | "cloud" | "connected" => Mode::Mirror,
            _ => Mode::Local,
        };
        if mode == Mode::Local {
            return None;
        }
        let url = std::env::var("NIKI_CONVEX_URL")
            .ok()
            .filter(|s| !s.is_empty())?;
        let token = std::env::var("NIKI_CONVEX_TOKEN")
            .ok()
            .filter(|s| !s.is_empty())?;
        Some(Self { url, token, mode })
    }

    /// HTTP endpoint for a Functions-API mutation call.
    pub fn mutation_endpoint(&self) -> String {
        format!("{}/api/mutation", self.url.trim_end_matches('/'))
    }

    pub fn bearer_token(&self) -> &str {
        &self.token
    }
}

/// One idempotent mirror write. The `key` is stable across retries so a
/// reconnect flush replays the original instead of duplicating it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MirrorOp {
    /// Stable idempotency key: `<taskUuid>:<kind>[:<seq>]`.
    pub key: String,
    /// Convex function path, e.g. `runs:createRun`.
    pub path: String,
    /// Arguments object for the Functions HTTP API.
    pub args: serde_json::Value,
}

/// Build a stable idempotency key for a mirror op.
pub fn idempotency_key(task_uuid: &str, kind: &str, seq: Option<u64>) -> String {
    match seq {
        Some(n) => format!("{task_uuid}:{kind}:{n}"),
        None => format!("{task_uuid}:{kind}"),
    }
}

impl MirrorOp {
    pub fn create_run(task_uuid: &str, status: &str, now_ms: i64) -> Self {
        Self {
            key: idempotency_key(task_uuid, "createRun", None),
            path: "runs:createRun".to_string(),
            args: serde_json::json!({
                "taskUuid": task_uuid,
                "status": status,
                "now": now_ms,
            }),
        }
    }

    pub fn transition_run(
        task_uuid: &str,
        expected_status: &str,
        next_status: &str,
        now_ms: i64,
    ) -> Self {
        Self {
            key: idempotency_key(task_uuid, "transitionRun", None),
            path: "runs:transitionRun".to_string(),
            args: serde_json::json!({
                "taskUuid": task_uuid,
                "expectedStatus": expected_status,
                "nextStatus": next_status,
                "now": now_ms,
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_stage(
        task_uuid: &str,
        seq: u64,
        role: &str,
        attempt: u64,
        status: &str,
        input_tokens: u64,
        output_tokens: u64,
        cost_usd: f64,
        latency_ms: u64,
    ) -> Self {
        Self {
            key: idempotency_key(task_uuid, "recordStage", Some(seq)),
            path: "runs:recordStage".to_string(),
            args: serde_json::json!({
                "taskUuid": task_uuid,
                "seq": seq,
                "role": role,
                "attempt": attempt,
                "status": status,
                "inputTokens": input_tokens,
                "outputTokens": output_tokens,
                "costUsd": cost_usd,
                "latencyMs": latency_ms,
            }),
        }
    }

    /// Body for `POST {url}/api/mutation`.
    pub fn http_body(&self) -> serde_json::Value {
        serde_json::json!({
            "path": self.path,
            "args": self.args,
            "format": "json",
        })
    }
}

/// Durable-outbox ordering helper. Ops are flushed FIFO; the high-water mark
/// (persisted by the caller only after a successful flush batch) is the count
/// of ops known delivered, so a crash replays from the mark.
#[derive(Debug, Default)]
pub struct Spool {
    ops: VecDeque<MirrorOp>,
    /// Number of leading ops confirmed delivered (for persistence).
    pub high_water: usize,
}

impl Spool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue, de-duplicating on the idempotency key (latest wins in place).
    pub fn push(&mut self, op: MirrorOp) {
        if let Some(pos) = self.ops.iter().position(|o| o.key == op.key) {
            self.ops[pos] = op;
        } else {
            self.ops.push_back(op);
        }
    }

    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Drain all ops in FIFO order for a flush attempt. On failure the caller
    /// re-queues the unconfirmed tail via [`Spool::requeue`].
    pub fn drain(&mut self) -> Vec<MirrorOp> {
        self.ops.drain(..).collect()
    }

    /// Re-queue ops that were not confirmed, preserving order.
    pub fn requeue(&mut self, ops: Vec<MirrorOp>) {
        for op in ops {
            self.ops.push_back(op);
        }
    }

    /// Advance the high-water mark after `confirmed` leading ops delivered.
    pub fn confirm(&mut self, confirmed: usize) {
        self.high_water = self.high_water.saturating_add(confirmed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idempotency_keys_are_stable() {
        assert_eq!(idempotency_key("t", "createRun", None), "t:createRun");
        assert_eq!(
            idempotency_key("t", "recordStage", Some(2)),
            "t:recordStage:2"
        );
        assert_eq!(
            idempotency_key("t", "recordStage", Some(2)),
            idempotency_key("t", "recordStage", Some(2))
        );
    }

    #[test]
    fn spool_dedupes_by_key_keeping_latest() {
        let mut spool = Spool::new();
        spool.push(MirrorOp::transition_run("t", "coding", "testing", 1));
        spool.push(MirrorOp::transition_run("t", "coding", "testing", 2));
        assert_eq!(spool.len(), 1);
        assert_eq!(spool.ops[0].args["now"], 2);
    }

    #[test]
    fn spool_drains_fifo_and_requeues_tail() {
        let mut spool = Spool::new();
        spool.push(MirrorOp::create_run("t", "queued", 1));
        spool.push(MirrorOp::transition_run("t", "queued", "planning", 2));
        let batch = spool.drain();
        assert_eq!(batch.len(), 2);
        assert_eq!(batch[0].path, "runs:createRun");
        assert!(spool.is_empty());
        spool.requeue(batch[1..].to_vec());
        assert_eq!(spool.len(), 1);
        spool.confirm(1);
        assert_eq!(spool.high_water, 1);
    }

    #[test]
    fn http_body_matches_functions_api_shape() {
        let op = MirrorOp::create_run("task-1", "queued", 7);
        let body = op.http_body();
        assert_eq!(body["path"], "runs:createRun");
        assert_eq!(body["format"], "json");
        assert_eq!(body["args"]["taskUuid"], "task-1");
    }

    #[test]
    fn mirror_disabled_without_env() {
        unsafe {
            std::env::remove_var("NIKI_CONTROL_PLANE");
            std::env::remove_var("NIKI_CONVEX_URL");
            std::env::remove_var("NIKI_CONVEX_TOKEN");
        }
        assert!(MirrorConfig::from_env().is_none());
    }

    #[test]
    fn mirror_requires_url_and_token() {
        unsafe {
            std::env::set_var("NIKI_CONTROL_PLANE", "mirror");
            std::env::remove_var("NIKI_CONVEX_URL");
            std::env::remove_var("NIKI_CONVEX_TOKEN");
        }
        assert!(MirrorConfig::from_env().is_none());
        unsafe {
            std::env::remove_var("NIKI_CONTROL_PLANE");
        }
    }
}
