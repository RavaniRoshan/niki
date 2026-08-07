mod common;

use niki::artifacts::types::AgentRole;
use niki::orchestrator::state::{StageMetric, TaskRecord, TaskStatus};
use uuid::Uuid;

#[test]
fn stage_metric_has_retry_count_field() {
    let metric = StageMetric {
        role: AgentRole::Coder,
        provider: "anthropic".into(),
        model: "claude-3-sonnet".into(),
        input_tokens: 100,
        output_tokens: 50,
        latency_ms: 2000,
        cost_usd: 0.001,
        retry_count: 2,
        ttft_ms: 150,
    };
    assert_eq!(metric.retry_count, 2);
}

#[test]
fn stage_metric_has_ttft_ms_field() {
    let metric = StageMetric {
        role: AgentRole::Planner,
        provider: "anthropic".into(),
        model: "claude-3-opus".into(),
        input_tokens: 200,
        output_tokens: 100,
        latency_ms: 3000,
        cost_usd: 0.002,
        retry_count: 1,
        ttft_ms: 450,
    };
    assert_eq!(metric.ttft_ms, 450);
}

#[test]
fn stage_metric_retry_count_defaults_to_zero_with_serde_default() {
    // The retry_count field has #[serde(default)], so deserializing from
    // JSON without the field should produce 0.
    let json = r#"{
        "role": "coder",
        "provider": "anthropic",
        "model": "claude-3-sonnet",
        "input_tokens": 100,
        "output_tokens": 50,
        "latency_ms": 2000,
        "cost_usd": 0.001
    }"#;
    let metric: StageMetric = serde_json::from_str(json).unwrap();
    assert_eq!(metric.retry_count, 0);
}

#[test]
fn stage_metric_ttft_defaults_to_zero_with_serde_default() {
    let json = r#"{
        "role": "tester",
        "provider": "openai",
        "model": "gpt-4",
        "input_tokens": 100,
        "output_tokens": 50,
        "latency_ms": 2000,
        "cost_usd": 0.001,
        "retry_count": 1
    }"#;
    let metric: StageMetric = serde_json::from_str(json).unwrap();
    assert_eq!(metric.ttft_ms, 0);
}

#[test]
fn stage_metric_serializes_all_fields() {
    let metric = StageMetric {
        role: AgentRole::Reviewer,
        provider: "anthropic".into(),
        model: "claude-3-sonnet".into(),
        input_tokens: 100,
        output_tokens: 50,
        latency_ms: 2000,
        cost_usd: 0.001,
        retry_count: 3,
        ttft_ms: 120,
    };
    let json = serde_json::to_string(&metric).unwrap();
    assert!(
        json.contains("\"retry_count\":3"),
        "JSON should include retry_count: {}",
        json
    );
    assert!(
        json.contains("\"ttft_ms\":120"),
        "JSON should include ttft_ms: {}",
        json
    );
}

#[test]
fn task_record_has_total_retry_count() {
    let record = TaskRecord::new(Uuid::new_v4(), "test task");
    assert_eq!(record.total_retry_count, 0);
}

#[test]
fn task_record_has_max_ttft_ms() {
    let record = TaskRecord::new(Uuid::new_v4(), "test task");
    assert_eq!(record.max_ttft_ms, 0);
}

#[test]
fn task_record_add_metrics_accumulates_retry_count() {
    let mut record = TaskRecord::new(Uuid::new_v4(), "test task");
    let metrics = vec![
        StageMetric {
            role: AgentRole::Planner,
            provider: "mock".into(),
            model: "mock-planner".into(),
            input_tokens: 80,
            output_tokens: 120,
            latency_ms: 1000,
            cost_usd: 0.0001,
            retry_count: 1,
            ttft_ms: 10,
        },
        StageMetric {
            role: AgentRole::Coder,
            provider: "mock".into(),
            model: "mock-coder".into(),
            input_tokens: 200,
            output_tokens: 80,
            latency_ms: 2000,
            cost_usd: 0.0002,
            retry_count: 0,
            ttft_ms: 25,
        },
    ];
    record.add_metrics(&metrics);
    assert_eq!(record.total_retry_count, 1);
    assert_eq!(record.max_ttft_ms, 25);
}

#[test]
fn task_record_add_metrics_tracks_max_ttft() {
    let mut record = TaskRecord::new(Uuid::new_v4(), "test task");
    let metrics = vec![
        StageMetric {
            role: AgentRole::Planner,
            provider: "mock".into(),
            model: "mock-planner".into(),
            input_tokens: 80,
            output_tokens: 120,
            latency_ms: 1000,
            cost_usd: 0.0001,
            retry_count: 2,
            ttft_ms: 450,
        },
        StageMetric {
            role: AgentRole::Coder,
            provider: "mock".into(),
            model: "mock-coder".into(),
            input_tokens: 200,
            output_tokens: 80,
            latency_ms: 2000,
            cost_usd: 0.0002,
            retry_count: 1,
            ttft_ms: 120,
        },
    ];
    record.add_metrics(&metrics);
    assert_eq!(record.max_ttft_ms, 450);
    assert_eq!(record.total_retry_count, 3);
}

#[test]
fn task_record_serializes_with_new_fields() {
    let mut record = TaskRecord::new(Uuid::new_v4(), "test task");
    let metric = StageMetric {
        role: AgentRole::Planner,
        provider: "mock".into(),
        model: "mock-planner".into(),
        input_tokens: 100,
        output_tokens: 50,
        latency_ms: 2000,
        cost_usd: 0.001,
        retry_count: 2,
        ttft_ms: 300,
    };
    record.add_metrics(&[metric]);

    let json = serde_json::to_string(&record).unwrap();
    assert!(
        json.contains("\"total_retry_count\":2"),
        "JSON should include total_retry_count"
    );
    assert!(
        json.contains("\"max_ttft_ms\":300"),
        "JSON should include max_ttft_ms"
    );
}

#[test]
fn task_record_deserializes_with_new_fields() {
    let record = TaskRecord::new(Uuid::new_v4(), "test task");
    let json = serde_json::to_string(&record).unwrap();
    let back: TaskRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(back.total_retry_count, 0);
    assert_eq!(back.max_ttft_ms, 0);
}

#[test]
fn task_record_status_is_running_initially() {
    let record = TaskRecord::new(Uuid::new_v4(), "test task");
    assert_eq!(record.status, TaskStatus::Running);
}
