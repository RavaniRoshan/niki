#![allow(dead_code)]

use niki::orchestrator::state::StageMetric;

pub fn total_input_tokens(metrics: &[StageMetric]) -> u32 {
    metrics.iter().map(|m| m.input_tokens).sum()
}

pub fn total_output_tokens(metrics: &[StageMetric]) -> u32 {
    metrics.iter().map(|m| m.output_tokens).sum()
}

pub fn total_cost(metrics: &[StageMetric]) -> f64 {
    metrics.iter().map(|m| m.cost_usd).sum()
}

pub fn assert_has_metric_for(metrics: &[StageMetric], role_name: &str) {
    let found = metrics
        .iter()
        .any(|m| format!("{:?}", m.role).to_lowercase() == role_name.to_lowercase());
    assert!(
        found,
        "Expected a metric for role '{}', got roles: {:?}",
        role_name,
        metrics
            .iter()
            .map(|m| format!("{:?}", m.role))
            .collect::<Vec<_>>()
    );
}

pub fn find_metric(
    metrics: &[StageMetric],
    role: niki::artifacts::types::AgentRole,
) -> Option<&StageMetric> {
    metrics.iter().find(|m| m.role == role)
}

pub fn assert_cost_non_negative(metrics: &[StageMetric]) {
    for m in metrics {
        assert!(
            m.cost_usd >= 0.0,
            "Cost should be non-negative, got {} for {:?}",
            m.cost_usd,
            m.role
        );
    }
}

pub fn assert_latency_positive(metrics: &[StageMetric]) {
    for m in metrics {
        assert!(
            m.latency_ms > 0 || m.role == niki::artifacts::types::AgentRole::Planner,
            "Latency should be positive for {:?}",
            m.role
        );
    }
}
