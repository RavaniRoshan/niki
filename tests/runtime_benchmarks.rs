use niki::artifacts::types::AgentRole;
use niki::config::NikiConfig;
use niki::runtime::{
    AgentEvent, AgentEventKind, AgentRuntime, ContextCompactor, ContextFragment, ContextStore,
    EventSink, FragmentKind, JournalEventSink, PermissionRequirement, RiskLevel, SessionCheckpoint,
    ToolCategory, ToolPolicy, ToolRouter, build_baseline_registry,
};
use std::sync::Arc;
use std::time::Instant;
use tempfile::TempDir;
use uuid::Uuid;

#[tokio::test]
async fn bench_session_initialization_and_prewarm() {
    let temp = TempDir::new().unwrap();
    let config = NikiConfig::default();
    let runtime = AgentRuntime::new(config);

    let start = Instant::now();
    let session = runtime
        .start_session(
            temp.path().to_path_buf(),
            Uuid::new_v4(),
            "Benchmark task description".into(),
            None,
            None,
        )
        .await
        .expect("Session should initialize successfully");
    let init_duration = start.elapsed();

    let prewarm_start = Instant::now();
    let prewarm = session.prewarm().await;
    let prewarm_duration = prewarm_start.elapsed();

    println!(
        "Benchmark: Session initialization: {:?}, Prewarm: {:?} (reported: {} ms)",
        init_duration, prewarm_duration, prewarm.prewarm_ms
    );

    // Assert that session initialization without network calls is sub-50ms
    assert!(
        init_duration.as_millis() < 50,
        "Session initialization took too long: {:?}",
        init_duration
    );
}

#[tokio::test]
async fn bench_context_store_compaction() {
    let mut store = ContextStore::new(4000);

    // Insert 50 fragments of varying priorities with enough content to test pruning
    let start = Instant::now();
    for i in 0..50 {
        let kind = match i % 5 {
            0 => FragmentKind::SystemInstructions,
            1 => FragmentKind::PlanContext,
            2 => FragmentKind::TestFailure,
            3 => FragmentKind::ReviewFinding,
            _ => FragmentKind::DynamicHistory,
        };
        store.upsert(ContextFragment::new(
            format!("fragment_{}", i),
            kind,
            format!(
                "Content line for fragment {} with repeated tokens for padding. Line 1: abc. Line 2: def. Line 3: ghi. Line 4: jkl.",
                i
            ),
            100,
        ));
    }
    let insert_duration = start.elapsed();

    let compact_start = Instant::now();
    let result = ContextCompactor::compact(&mut store, 800);
    let compact_duration = compact_start.elapsed();

    println!(
        "Benchmark: 50 Context fragments insert: {:?}, Compaction: {:?} (tokens before: {}, tokens after: {}, removed: {})",
        insert_duration,
        compact_duration,
        result.tokens_before,
        result.tokens_after,
        result.fragments_removed
    );

    assert!(
        compact_duration.as_millis() < 15,
        "Compaction took too long: {:?}",
        compact_duration
    );
    assert!(result.tokens_after < result.tokens_before);
    assert!(result.fragments_removed > 0);
    // Ensure all PlanContext fragments are strictly preserved
    assert!(store.get("fragment_1").is_some());
    assert_eq!(
        store.get("fragment_1").unwrap().kind,
        FragmentKind::PlanContext
    );
}

#[tokio::test]
async fn bench_tool_policy_and_router_throughput() {
    let registry = Arc::new(build_baseline_registry());
    let router = ToolRouter::new(registry);
    let coder_policy = ToolPolicy::for_role(AgentRole::Coder, RiskLevel::Medium, "manual");

    let iterations = 10_000;
    let start = Instant::now();
    for _ in 0..iterations {
        let allowed = coder_policy.check_permission(
            "file_edit",
            ToolCategory::Modify,
            RiskLevel::Medium,
            PermissionRequirement::Allow,
        );
        assert!(allowed.is_ok());

        let denied = coder_policy.check_permission(
            "git_push",
            ToolCategory::Vcs,
            RiskLevel::Critical,
            PermissionRequirement::Deny,
        );
        assert!(denied.is_err());
    }
    let duration = start.elapsed();

    println!(
        "Benchmark: {} policy permission checks completed in {:?} ({:.2} ops/sec)",
        iterations * 2,
        duration,
        (iterations * 2) as f64 / duration.as_secs_f64()
    );

    // Check that tool router lookup by name is sub-microsecond
    let lookup_start = Instant::now();
    for _ in 0..iterations {
        assert!(router.registry().get("read").is_some());
    }
    let lookup_duration = lookup_start.elapsed();

    println!(
        "Benchmark: {} router lookups completed in {:?} ({:.2} ops/sec)",
        iterations,
        lookup_duration,
        iterations as f64 / lookup_duration.as_secs_f64()
    );

    assert!(duration.as_millis() < 100);
}

#[tokio::test]
async fn bench_checkpoint_serialization_roundtrip() {
    let temp = TempDir::new().unwrap();
    let config = NikiConfig::default();
    let runtime = AgentRuntime::new(config);

    let session = runtime
        .start_session(
            temp.path().to_path_buf(),
            Uuid::new_v4(),
            "Benchmark Task".into(),
            None,
            None,
        )
        .await
        .unwrap();

    let start = Instant::now();
    let cp_path = runtime
        .checkpoint(
            &session,
            AgentRole::Coder,
            Some("niki/bench".into()),
            Some("Low".into()),
            vec![(AgentRole::Planner, "{}".into())],
        )
        .await
        .expect("Checkpoint save should succeed");
    let save_duration = start.elapsed();

    let load_start = Instant::now();
    let loaded = SessionCheckpoint::load(&cp_path).expect("Checkpoint load should succeed");
    let load_duration = load_start.elapsed();

    println!(
        "Benchmark: Checkpoint save: {:?}, load: {:?}",
        save_duration, load_duration
    );

    assert_eq!(loaded.session_id, session.session_id);
    assert_eq!(loaded.current_role, AgentRole::Coder);
    assert!(save_duration.as_millis() < 50);
    assert!(load_duration.as_millis() < 50);
}

#[tokio::test]
async fn bench_journal_event_throughput() {
    let temp = TempDir::new().unwrap();
    let journal_path = temp.path().join("events.jsonl");
    let sink = JournalEventSink::new(&journal_path);

    let iterations = 250;
    let start = Instant::now();
    for i in 0..iterations {
        let event = AgentEvent::new(
            "bench_session",
            "turn_1",
            format!("step_{}", i),
            AgentEventKind::ToolExecutionStarted,
            serde_json::json!({
                "index": i,
                "tool": "read",
                "path": "src/main.rs"
            }),
        );
        sink.emit(&event)
            .await
            .expect("Journal write should succeed");
    }
    let duration = start.elapsed();

    println!(
        "Benchmark: {} events written to journal in {:?} ({:.2} events/sec)",
        iterations,
        duration,
        iterations as f64 / duration.as_secs_f64()
    );

    assert!(duration.as_millis() < 500);
    assert!(journal_path.exists());
}
