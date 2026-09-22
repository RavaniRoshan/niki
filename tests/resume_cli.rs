use assert_cmd::Command;
use niki::artifacts::types::AgentRole;
use niki::config::NikiConfig;
use niki::runtime::{AgentRuntime, ContextFragment, FragmentKind};
use tempfile::TempDir;
use uuid::Uuid;

#[tokio::test]
async fn test_cli_resume_command() {
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path().to_path_buf();

    // Create minimal git repo so niki config loading / project detection works
    let _repo = git2::Repository::init(&project_dir).expect("Git init should succeed");

    let config = NikiConfig::default();
    let runtime = AgentRuntime::new(config);

    let task_id = Uuid::new_v4();
    let session = runtime
        .start_session(
            project_dir.clone(),
            task_id,
            "Add telemetry subsystem".into(),
            None,
            None,
        )
        .await
        .expect("Session creation should succeed");

    // Add a context fragment
    {
        let mut store = session.context_store.write().await;
        store.upsert(ContextFragment::new(
            "telemetry_spec",
            FragmentKind::PlanContext,
            "Detailed telemetry specification",
            100,
        ));
    }

    let cp_path = runtime
        .checkpoint(
            &session,
            AgentRole::Coder,
            Some("niki/telemetry-feature".into()),
            Some("Medium".into()),
            vec![(AgentRole::Planner, "{\"spec\":\"data\"}".into())],
        )
        .await
        .expect("Checkpoint save should succeed");

    assert!(cp_path.exists());

    // Execute `niki resume <session-id> --project <project_dir>`
    let mut cmd = Command::cargo_bin("niki").expect("binary niki exists");
    let assert = cmd
        .args([
            "resume",
            &session.session_id,
            "--project",
            project_dir.to_str().unwrap(),
        ])
        .assert();

    let output = assert.success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    assert!(stdout.contains("Resumed session:"));
    assert!(stdout.contains(&session.session_id));
    assert!(stdout.contains(&task_id.to_string()));
    assert!(stdout.contains("Add telemetry subsystem"));
    assert!(stdout.contains("Coder"));
    assert!(stdout.contains("niki/telemetry-feature"));
    assert!(stdout.contains("Session state restored successfully"));
}

#[test]
fn test_cli_resume_invalid_id() {
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path().to_path_buf();

    let mut cmd = Command::cargo_bin("niki").expect("binary niki exists");
    let assert = cmd
        .args([
            "resume",
            "non-existent-session-id",
            "--project",
            project_dir.to_str().unwrap(),
        ])
        .assert();

    assert.failure();
}
