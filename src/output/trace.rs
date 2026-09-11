//! Run-trace spans: one JSON object per line (`trace.jsonl` next to `report.md`).
//!
//! The trace is a faithful transform of recorded data — stage order, per-stage
//! latency, tokens, and cost from [`StageMetric`]s, plus task/test/verdict
//! markers. Per-stage absolute timestamps are NOT recorded anywhere upstream,
//! so the timeline is derived by stacking latencies in execution order and
//! labeled `"derived_timeline": true`. Consumers must not treat offsets as
//! measured wall-clock spans.

use crate::orchestrator::pipeline::PipelineResult;
use serde_json::json;

/// Build the trace lines for a finished run. Pure function of the result —
/// no I/O, no clock reads — so unit tests pin the exact contract.
pub fn trace_lines(
    task_id: &str,
    description: &str,
    branch: Option<&str>,
    result: &PipelineResult,
) -> Vec<serde_json::Value> {
    let mut lines = Vec::new();
    let mut offset_ms: u64 = 0;
    lines.push(json!({
        "trace_id": task_id,
        "span": "task",
        "description": description,
        "derived_timeline": true,
    }));
    for m in &result.metrics {
        lines.push(json!({
            "trace_id": task_id,
            "span": format!("{:?}", m.role).to_lowercase(),
            "parent": "task",
            "provider": m.provider,
            "model": m.model,
            "start_offset_ms": offset_ms,
            "duration_ms": m.latency_ms,
            "input_tokens": m.input_tokens,
            "output_tokens": m.output_tokens,
            "cached_input_tokens": m.cached_input_tokens,
            "reasoning_tokens": m.reasoning_tokens,
            "cost_usd": m.cost_usd,
            "retries": m.retry_count,
            "derived_timeline": true,
        }));
        offset_ms += m.latency_ms;
    }
    if let Some(te) = &result.test_execution {
        lines.push(json!({
            "trace_id": task_id,
            "span": "test_execution",
            "parent": "task",
            "command": te.command,
            "passed": te.passed,
            "exit_code": te.exit_code,
            "mutation_passed": te.mutation.as_ref().map(|m| m.passed),
            "derived_timeline": true,
        }));
    }
    lines.push(json!({
        "trace_id": task_id,
        "span": "verdict",
        "parent": "task",
        "verdict": format!("{:?}", result.verdict),
        "revision_rounds": result.revision_rounds,
        "branch": branch,
        "topology": format!("{:?}", result.topology).to_lowercase(),
        "total_offset_ms": offset_ms,
        "derived_timeline": true,
    }));
    lines
}

/// Render the trace as newline-delimited JSON.
pub fn render_trace(
    task_id: &str,
    description: &str,
    branch: Option<&str>,
    result: &PipelineResult,
) -> String {
    trace_lines(task_id, description, branch, result)
        .iter()
        .map(|v| serde_json::to_string(v).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifacts::types::AgentRole;
    use crate::orchestrator::state::StageMetric;

    fn metric(role: AgentRole, latency_ms: u64, cost: f64) -> StageMetric {
        StageMetric {
            role,
            provider: "mock".into(),
            model: "mock-model".into(),
            input_tokens: 10,
            output_tokens: 5,
            cached_input_tokens: 0,
            reasoning_tokens: 0,
            latency_ms,
            cost_usd: cost,
            retry_count: 0,
            ttft_ms: 0,
        }
    }

    fn result_with(metrics: Vec<StageMetric>) -> PipelineResult {
        PipelineResult {
            task_id: uuid::Uuid::nil(),
            context_budget: crate::orchestrator::state::PipelineState::new(uuid::Uuid::nil())
                .context_budget,
            state: crate::orchestrator::state::PipelineState::new(uuid::Uuid::nil()),
            final_diff: String::new(),
            diff_guardwarn: None,
            verdict: crate::artifacts::types::Verdict::Approved,
            revision_rounds: 1,
            artifacts: vec![],
            metrics,
            safety_proof: None,
            isolation: vec![],
            topology: crate::config::types::TopologyMode::MultiAgent,
            topology_reason: String::new(),
            risk_level: String::new(),
            risk_rationale: String::new(),
            test_execution: None,
        }
    }

    #[test]
    fn trace_stacks_latencies_in_order() {
        let result = result_with(vec![
            metric(AgentRole::Planner, 100, 0.01),
            metric(AgentRole::Coder, 250, 0.02),
        ]);
        let lines = trace_lines("t1", "do things", Some("niki/abc"), &result);
        assert_eq!(lines.len(), 4); // task + 2 stages + verdict
        assert_eq!(lines[1]["span"], "planner");
        assert_eq!(lines[1]["start_offset_ms"], 0);
        assert_eq!(lines[2]["span"], "coder");
        assert_eq!(lines[2]["start_offset_ms"], 100);
        assert_eq!(lines[3]["span"], "verdict");
        assert_eq!(lines[3]["total_offset_ms"], 350);
        assert_eq!(lines[3]["branch"], "niki/abc");
        for line in &lines {
            assert_eq!(line["derived_timeline"], true);
            assert_eq!(line["trace_id"], "t1");
        }
    }

    #[test]
    fn trace_renders_valid_jsonl() {
        let result = result_with(vec![metric(AgentRole::Planner, 10, 0.0)]);
        let text = render_trace("t2", "x", None, &result);
        for line in text.lines() {
            serde_json::from_str::<serde_json::Value>(line).expect("valid JSON per line");
        }
        assert_eq!(text.lines().count(), 3);
    }
}
