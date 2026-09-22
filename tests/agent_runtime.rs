//! Integration tests for NIKI's AgentRuntime, Session, Turn, Step, ContextStore,
//! ToolRouter, Policy, Events, and Checkpointing.

use niki::artifacts::types::AgentRole;
use niki::config::NikiConfig;
use niki::runtime::{
    AgentEventKind, AgentRuntime, ContextCompactor, ContextFragment, ContextStore, FragmentKind,
    JournalEventSink, PermissionRequirement, RiskLevel, ToolCategory, ToolContext, ToolInput,
    ToolPolicy, ToolStatus, build_baseline_registry,
};

use std::sync::Arc;
use tempfile::tempdir;
use uuid::Uuid;

#[tokio::test]
async fn test_runtime_session_lifecycle() {
    let tmp = tempdir().unwrap();
    let config = NikiConfig::default();
    let runtime = AgentRuntime::new(config);

    let task_id = Uuid::new_v4();
    let mut session = runtime
        .start_session(
            tmp.path().to_path_buf(),
            task_id,
            "Add health check endpoint".to_string(),
            None,
            None,
        )
        .await
        .expect("start session should succeed");

    assert_eq!(session.task_id, task_id);
    assert!(session.session_id.starts_with("sess-"));

    // Prewarm resources
    let prewarm = session.prewarm().await;
    assert!(prewarm.tools_ready);
    assert!(prewarm.model_session_ready);

    // Start a turn
    let mut turn = runtime
        .start_turn(&mut session, AgentRole::Planner, "Plan the implementation")
        .await
        .expect("start turn should succeed");

    assert_eq!(turn.role, AgentRole::Planner);
    assert_eq!(turn.turn_number, 1);

    // Record an event
    session
        .emit_event(
            &turn.turn_id,
            "step-1",
            AgentEventKind::ContextBuilt,
            serde_json::json!({"tokens": 150}),
        )
        .await
        .expect("emit event should succeed");

    turn.complete();
    session.turns.push(turn);
    assert_eq!(session.turns.len(), 1);
}

#[tokio::test]
async fn test_context_store_incremental_and_priorities() {
    let mut store = ContextStore::new(200);

    // Static fragment: System instructions (stable prefix, cacheable)
    let sys = ContextFragment::new(
        "sys",
        FragmentKind::SystemInstructions,
        "You are an expert Rust engineer.",
        50,
    );
    assert!(store.upsert(sys.clone()));
    // Duplicate insertion is suppressed
    assert!(!store.upsert(sys));

    // Dynamic high-priority fragment
    let plan = ContextFragment::new(
        "plan",
        FragmentKind::PlanContext,
        "Step 1: Update Cargo.toml\nStep 2: Add route",
        50,
    )
    .with_cacheable(false)
    .with_priority(90);
    store.upsert(plan);

    // Dynamic lower-priority fragment
    let history = ContextFragment::new(
        "hist",
        FragmentKind::DynamicHistory,
        "Verbose old turn history...".repeat(5),
        200,
    )
    .with_cacheable(false)
    .with_priority(20);
    store.upsert(history);

    let assembled = store.assemble();
    assert!(assembled.system_prompt.contains("expert Rust engineer"));
    assert!(assembled.user_prompt.contains("Step 1: Update Cargo.toml"));
    assert!(assembled.cacheable_tokens > 0);
    assert!(assembled.total_tokens <= 200);
}

#[tokio::test]
async fn test_compaction_preserves_high_value_state() {
    let mut store = ContextStore::new(1000);

    // High value fragments
    store.upsert(
        ContextFragment::new("plan", FragmentKind::PlanContext, "Important Plan", 50)
            .with_cacheable(false)
            .with_priority(90),
    );
    store.upsert(
        ContextFragment::new(
            "failure",
            FragmentKind::TestFailure,
            "Test failed: assertion mismatch",
            50,
        )
        .with_cacheable(false)
        .with_priority(80),
    );

    // Low value / verbose tool output
    let verbose_log = (0..40)
        .map(|i| format!("Compiling crate line {i}..."))
        .collect::<Vec<_>>()
        .join("\n");
    store.upsert(
        ContextFragment::new("build_log", FragmentKind::ToolContext, verbose_log, 500)
            .with_cacheable(false)
            .with_priority(25),
    );

    let before = store.total_estimated_tokens();
    let result = ContextCompactor::compact(&mut store, 100);

    assert!(result.tokens_after < before);
    // Plan and test failures must be preserved
    assert!(store.get("plan").is_some());
    assert!(store.get("failure").is_some());
}

#[tokio::test]
async fn test_tool_router_and_policy_enforcement() {
    let registry = Arc::new(build_baseline_registry());
    let router = niki::runtime::ToolRouter::new(registry);

    let tmp = tempdir().unwrap();
    let ctx = ToolContext {
        agent_id: niki::mission::AgentId("agent-test".to_string()),
        mission_id: niki::mission::MissionId("mission-test".to_string()),
        role: "planner".to_string(),
        project_path: tmp.path().to_path_buf(),
        permissions: std::collections::HashMap::new(),
        permission_mode: "manual".to_string(),
        task_store: None,
    };

    // 1. Planner policy should DENY bash / edit / write
    let planner_policy = ToolPolicy::for_role(AgentRole::Planner, RiskLevel::Critical, "manual");
    let res = router
        .route_and_execute(
            "bash",
            ToolInput::new(serde_json::json!({"command": "ls"})),
            &ctx,
            Some(&planner_policy),
        )
        .await;

    assert_eq!(res.status, ToolStatus::PermissionDenied);
    assert!(res.summary.contains("denied"));

    // 2. Planner policy should ALLOW read / glob
    let res_read = router
        .route_and_execute(
            "glob",
            ToolInput::new(serde_json::json!({"pattern": "*.rs"})),
            &ctx,
            Some(&planner_policy),
        )
        .await;

    assert_eq!(res_read.status, ToolStatus::Success);

    // 3. Coder policy should allow bash (but block dangerous commands)
    let coder_policy = ToolPolicy::for_role(AgentRole::Coder, RiskLevel::Critical, "manual");
    assert!(
        coder_policy
            .check_permission(
                "bash",
                ToolCategory::Execute,
                RiskLevel::High,
                PermissionRequirement::Allow
            )
            .is_ok()
    );
}

#[tokio::test]
async fn test_checkpoint_and_resumability() {
    let tmp = tempdir().unwrap();
    let config = NikiConfig::default();
    let runtime = AgentRuntime::new(config);

    let task_id = Uuid::new_v4();
    let session = runtime
        .start_session(
            tmp.path().to_path_buf(),
            task_id,
            "Refactor auth service".to_string(),
            None,
            None,
        )
        .await
        .unwrap();

    // Add context fragment
    {
        let mut store = session.context_store.write().await;
        store.upsert(ContextFragment::new(
            "task_spec",
            FragmentKind::PlanContext,
            "Auth plan: move to JWT",
            100,
        ));
    }

    let artifacts = vec![(AgentRole::Planner, "{\"summary\":\"spec\"}".to_string())];
    let cp_path = runtime
        .checkpoint(
            &session,
            AgentRole::Planner,
            Some("niki/auth-refactor".to_string()),
            Some("normal".to_string()),
            artifacts.clone(),
        )
        .await
        .expect("checkpoint save should succeed");

    assert!(cp_path.exists());

    // Resume the session from checkpoint
    let (resumed_session, checkpoint) = runtime
        .resume(tmp.path(), &session.session_id)
        .await
        .expect("session resume should succeed");

    assert_eq!(resumed_session.task_id, task_id);
    assert_eq!(checkpoint.current_role, AgentRole::Planner);
    assert_eq!(checkpoint.produced_artifacts.len(), 1);
    assert_eq!(checkpoint.produced_artifacts[0].0, AgentRole::Planner);
    assert_eq!(
        checkpoint.active_branch.as_deref(),
        Some("niki/auth-refactor")
    );

    // Check that context store was restored
    let restored_store = resumed_session.context_store.read().await;
    assert!(restored_store.get("task_spec").is_some());
    assert!(
        restored_store
            .get("task_spec")
            .unwrap()
            .content
            .contains("move to JWT")
    );
}

#[tokio::test]
async fn test_journal_event_sink_persistence() {
    let tmp = tempdir().unwrap();
    let journal_file = tmp.path().join("events.jsonl");
    let sink = Arc::new(JournalEventSink::new(&journal_file));

    let config = NikiConfig::default();
    let runtime = AgentRuntime::new(config);

    let session = runtime
        .start_session(
            tmp.path().to_path_buf(),
            Uuid::new_v4(),
            "Logging test".to_string(),
            Some(sink),
            None,
        )
        .await
        .unwrap();

    session
        .emit_event(
            "t-1",
            "s-1",
            AgentEventKind::ToolExecutionStarted,
            serde_json::json!({"tool": "read"}),
        )
        .await
        .unwrap();

    let text = tokio::fs::read_to_string(&journal_file).await.unwrap();
    assert!(text.contains("session_started"));
    assert!(text.contains("tool_execution_started"));
}
