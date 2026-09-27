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
        cached_input_tokens: 0,
        reasoning_tokens: 0,
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
        cached_input_tokens: 0,
        reasoning_tokens: 0,
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
        cached_input_tokens: 0,
        reasoning_tokens: 0,
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
            cached_input_tokens: 0,
            reasoning_tokens: 0,
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
            cached_input_tokens: 0,
            reasoning_tokens: 0,
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
            cached_input_tokens: 0,
            reasoning_tokens: 0,
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
            cached_input_tokens: 0,
            reasoning_tokens: 0,
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
        cached_input_tokens: 0,
        reasoning_tokens: 0,
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

/// A repair retry is a second real request, and it must be billed.
///
/// `run_agent` used to assemble its usage total *before* the repair loop, so
/// the second request's tokens — which the loop itself accumulates into
/// `usage` — were never reported. A stage that needed two rounds to produce a
/// valid artifact silently under-reported its own cost to the user and to the
/// spend cap, and it did so precisely on the runs that were most expensive.
///
/// The test drives the real function against a mock whose first response is
/// unparseable, so the repair path is genuinely taken.
#[tokio::test]
async fn a_repair_retry_is_included_in_the_reported_usage() {
    use niki::agents::run_agent;
    use niki::artifacts::types::AgentRole;
    use niki::llm::mock::MockProvider;

    const FIRST_IN: u32 = 1_000;
    const FIRST_OUT: u32 = 500;
    const REPAIR_IN: u32 = 2_000;
    const REPAIR_OUT: u32 = 700;

    let dir = tempfile::TempDir::new().unwrap();
    let script_path = dir.path().join("script.json");
    let script = serde_json::json!({
        "models": {
            "m": {
                "responses": [
                    // Attempt 1: not JSON at all, so repair re-prompts.
                    {"text": "I could not produce an artifact, sorry.", "input_tokens": FIRST_IN, "output_tokens": FIRST_OUT},
                    // Attempt 2: a conformant artifact.
                    {"text": format!("```json\n{}\n```", common::mock_llm::task_spec_json()), "input_tokens": REPAIR_IN, "output_tokens": REPAIR_OUT},
                ]
            }
        }
    });
    std::fs::write(&script_path, serde_json::to_string_pretty(&script).unwrap()).unwrap();

    let provider = MockProvider::new(Some(&script_path.to_string_lossy())).unwrap();
    let mut display = niki::display::agent_stream::AgenticDisplay::new();
    let ctx = minijinja::context! {
        task_description => "Fix the off-by-one in paginate",
        project_knowledge => "",
        project_memory => "",
        current_files => "",
        mcp_tools => "",
    };

    let (_json, usage, _retries, _ttft) = run_agent(
        AgentRole::Planner,
        &provider,
        "m",
        "planner.md",
        ctx,
        "schemas/task_spec.schema.json",
        &mut display,
        4096,
        0.2,
        None,
    )
    .await
    .expect("the second attempt is conformant, so the stage completes");

    assert_eq!(
        usage.input_tokens,
        FIRST_IN + REPAIR_IN,
        "both attempts must be billed: the repair request is a real request"
    );
    assert_eq!(
        usage.output_tokens,
        FIRST_OUT + REPAIR_OUT,
        "both attempts' output must be billed"
    );
}
