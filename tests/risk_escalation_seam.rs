//! C3 — the risk classifier escalates a minimal topology into a security audit, and
//! leaves a low-risk task alone.
//!
//! Two directions, because a classifier test that only checks the positive case proves
//! nothing: if `apply_risk_stages` injected a `SecurityAuditor` unconditionally, every
//! positive assertion here would still pass. The low-risk case is the one that says the
//! escalation is a decision rather than a default.
//!
//! `tests/risk_enumeration.rs` already calls `apply_risk_stages` directly for a High
//! tier and a Low tier. What it does not do is drive the **real engine** and read the
//! stage sequence the run actually emitted, which is the only place the injection can be
//! checked against what ran rather than against what a function returned. So the second
//! test here is two `niki serve` turns — one security-sensitive task, one not — and it
//! reads the `stage.start` notifications off the wire.
//!
//! ## What "minimal topology" means here, precisely
//!
//! `[security] enabled = false`, no explicit `[pipeline].stages`, and the default `auto`
//! topology. Under `auto`, `force_multiagent_for_high_risk` upgrades a fast-path
//! selection to the full chain precisely so the risk-added auditor survives; without
//! that upgrade the `singleagent` arm collapses every stage but the Coder and the
//! injected auditor would be computed and then never run. Both halves are asserted
//! directly.
//!
//! A *pinned* `[pipeline].topology = "singleagent"` is a different case and is
//! deliberately not claimed here: `force_multiagent_for_high_risk` returns `false`
//! unless the configured topology is `auto`, because the module's own contract is that
//! an explicit topology is never rewritten. `pinned_singleagent_does_not_override_an_explicit_topology`
//! pins that behaviour so it is documented rather than accidental.

mod common;

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use common::fixture_repo::create_fixture_repo;
use common::mock_llm::{
    MockScriptBuilder, code_diff_json, review_verdict_approved_json, security_verdict_json,
    test_report_json,
};
use niki::artifacts::types::{AgentRole, Complexity, FileAction, FileChange, TaskSpec};
use niki::config::NikiConfig;
use niki::config::types::{PipelineStageConfig, TopologyMode};
use niki::orchestrator::pipeline::{
    apply_risk_stages, force_multiagent_for_high_risk, resolve_stages,
};
use niki::risk::{RiskLevel, classify};
use serde_json::json;

const BEFORE: &str = "let end = start + size - 1;";
const AFTER: &str = "let end = start + size;";
const TARGET: &str = "src/list.rs";

fn spec(summary: &str, approach: &str, files: &[&str]) -> TaskSpec {
    TaskSpec {
        summary: summary.to_string(),
        approach: approach.to_string(),
        files_to_modify: files
            .iter()
            .map(|p| FileChange {
                path: p.to_string(),
                action: FileAction::Modify,
                description: "x".to_string(),
            })
            .collect(),
        acceptance_criteria: vec![],
        constraints: vec![],
        estimated_complexity: Complexity::Low,
        uncertainties: None,
    }
}

fn roles_of(stages: &[PipelineStageConfig]) -> Vec<AgentRole> {
    stages.iter().map(|s| s.role).collect()
}

/// A task that touches authentication. The denylist entry is matched against the
/// planned paths *and* the spec text, so a path is enough.
fn auth_spec() -> TaskSpec {
    spec(
        "Tighten session token expiry in the login path",
        "Rotate the refresh credential on every request and reject an expired one.",
        &["src/auth/session.rs"],
    )
}

/// An ordinary arithmetic fix: one file, no sensitive path, no severity keyword.
fn low_risk_spec() -> TaskSpec {
    spec(
        "Fix off-by-one in paginate",
        "Change the slice upper bound from start + size - 1 to start + size.",
        &["src/list.rs"],
    )
}

/// A config with the audit turned off — the whole point being that the tier alone has
/// to bring the auditor back.
fn audit_off_config() -> NikiConfig {
    let mut config = NikiConfig::default();
    config.security.enabled = false;
    config.critic.enabled = false;
    config.pipeline.stages.clear();
    config
}

fn position(stages: &[PipelineStageConfig], role: AgentRole) -> usize {
    stages
        .iter()
        .position(|s| s.role == role)
        .unwrap_or_else(|| panic!("{role:?} must be in the stage list: {stages:?}"))
}

/// A security-sensitive task brings in a `SecurityAuditor` even though `[security]`
/// says the audit is off, and the auditor is placed ahead of the Reviewer so its
/// findings are something the Reviewer can act on rather than a footnote it cannot.
#[test]
fn a_security_sensitive_task_forces_an_auditor_ahead_of_the_reviewer() {
    let config = audit_off_config();
    let risk = classify(&auth_spec(), &config);
    assert_eq!(
        risk.level,
        RiskLevel::High,
        "a path under src/auth must clear the High tier; got {:?} ({})",
        risk.level,
        risk.rationale
    );

    let before = roles_of(&resolve_stages(&config));
    assert!(
        !before.contains(&AgentRole::SecurityAuditor),
        "precondition: with `[security] enabled = false` the resolved topology has no auditor: {before:?}"
    );

    let after = apply_risk_stages(resolve_stages(&config), &risk, &config);
    let roles = roles_of(&after);
    assert!(
        roles.contains(&AgentRole::SecurityAuditor),
        "a High tier must force the auditor the config asked to skip: {roles:?}"
    );
    assert!(
        position(&after, AgentRole::SecurityAuditor) < position(&after, AgentRole::Reviewer),
        "the auditor must run before the Reviewer, not after it: {roles:?}"
    );

    // The escalation survives the fast path. Without this upgrade a run whose topology
    // resolved to `singleagent` would compute the auditor above and then collapse it
    // away in the SingleAgent arm, which runs the Coder alone.
    assert!(
        force_multiagent_for_high_risk(TopologyMode::SingleAgent, TopologyMode::Auto, risk.level),
        "a High tier must upgrade an auto-selected fast path, or the injected stage is dead code"
    );
}

/// The other direction. A low-risk task changes nothing: no auditor, and no Critic
/// either — `apply_risk_stages` returns before the tier is even looked at.
#[test]
fn a_low_risk_task_escalates_nothing() {
    let config = audit_off_config();
    let risk = classify(&low_risk_spec(), &config);
    assert_eq!(
        risk.level,
        RiskLevel::Low,
        "an arithmetic fix in one ordinary file must stay Low; got {:?} ({})",
        risk.level,
        risk.rationale
    );

    let before = resolve_stages(&config);
    let after = apply_risk_stages(before.clone(), &risk, &config);
    assert_eq!(
        roles_of(&after),
        roles_of(&before),
        "a Low tier must leave the topology exactly as it was"
    );
    assert!(
        !roles_of(&after).contains(&AgentRole::SecurityAuditor),
        "a Low task must not acquire a security auditor: {:?}",
        roles_of(&after)
    );
    assert!(
        !roles_of(&after).contains(&AgentRole::Critic),
        "a Low task must not acquire a Critic either: {:?}",
        roles_of(&after)
    );
    assert!(
        !force_multiagent_for_high_risk(TopologyMode::SingleAgent, TopologyMode::Auto, risk.level),
        "a Low tier must leave an auto-selected fast path alone"
    );
}

/// An explicit `[pipeline].topology = "singleagent"` is user intent and is never
/// rewritten, so the auditor is *not* forced. Documented here because the previous test
/// proves the escalation and this one is the boundary it stops at — leaving it
/// unwritten would let a reader assume the opposite.
#[test]
fn pinned_singleagent_does_not_override_an_explicit_topology() {
    let config = audit_off_config();
    let risk = classify(&auth_spec(), &config);
    assert!(
        !force_multiagent_for_high_risk(
            TopologyMode::SingleAgent,
            TopologyMode::SingleAgent,
            risk.level
        ),
        "an explicitly pinned topology is user intent; the risk override must not rewrite it"
    );
}

// ── the seam ─────────────────────────────────────────────────────────

fn wrap_json(text: &str) -> String {
    format!("```json\n{text}\n```")
}

fn niki_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_niki"))
}

/// The chain a run of `spec` needs, with the auditor and the Critic available but not
/// requested: `[security] enabled = false` and `[critic] enabled = false`, so the only
/// way an auditor can appear in the emitted sequence is the risk tier.
fn escalation_toml(script_path: &Path) -> String {
    format!(
        r#"[docker]
backend = "worktree"
extra_packages = []

[security]
enabled = false

[critic]
enabled = false

[red_blue]
enabled = false

[providers.mock]
base_url = "{}"
default_model = "mock-planner"

[agents.planner]
provider = "mock"
model = "mock-planner"

[agents.coder]
provider = "mock"
model = "mock-coder"

[agents.tester]
provider = "mock"
model = "mock-tester"

[agents.reviewer]
provider = "mock"
model = "mock-reviewer"

[agents.security_auditor]
provider = "mock"
model = "mock-security_auditor"
"#,
        script_path.display()
    )
}

fn script_for(spec_json: &str, path: &Path) -> PathBuf {
    MockScriptBuilder::new()
        .add_response("mock-planner", &wrap_json(spec_json), 80, 120)
        .add_response(
            "mock-coder",
            &wrap_json(&code_diff_json(BEFORE, AFTER, TARGET)),
            200,
            80,
        )
        .add_response("mock-tester", &wrap_json(&test_report_json()), 100, 60)
        .add_response(
            "mock-reviewer",
            &wrap_json(&review_verdict_approved_json()),
            150,
            50,
        )
        .add_response(
            "mock-security_auditor",
            &wrap_json(&security_verdict_json()),
            100,
            40,
        )
        .write(&path.to_path_buf())
}

fn auth_spec_json() -> String {
    json!({
        "summary": "Tighten session token expiry in the login path",
        "approach": "Rotate the refresh credential on every request and reject an expired one.",
        "files_to_modify": [
            {"path": "src/auth/session.rs", "action": "modify", "description": "rotate the credential"}
        ],
        "acceptance_criteria": ["An expired credential is rejected"],
        "constraints": [],
        "estimated_complexity": "low"
    })
    .to_string()
}

fn low_risk_spec_json() -> String {
    json!({
        "summary": "Fix off-by-one in paginate",
        "approach": "Change the slice upper bound from start + size - 1 to start + size.",
        "files_to_modify": [
            {"path": "src/list.rs", "action": "modify", "description": "boundary fix"}
        ],
        "acceptance_criteria": ["The last page item is returned correctly"],
        "constraints": [],
        "estimated_complexity": "low"
    })
    .to_string()
}

/// Drive one `niki serve` turn and return the roles of every `stage.start` it emitted,
/// in order.
fn emitted_stage_roles(project: &Path, script: &Path) -> Vec<String> {
    std::fs::write(project.join("niki.toml"), escalation_toml(script)).expect("write niki.toml");

    let mut child = Command::new(niki_bin())
        .args([
            "serve",
            "--project",
            project.to_str().expect("utf-8 project path"),
            "--backend",
            "worktree",
            "--bare",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("niki serve starts");
    let mut stdin = child.stdin.take().expect("stdin piped");
    let stdout = child.stdout.take().expect("stdout piped");
    let mut guard = ChildGuard(Some(child));
    let stderr = guard
        .0
        .as_mut()
        .expect("child alive")
        .stderr
        .take()
        .expect("stderr piped");

    let (tx, rx) = mpsc::channel::<serde_json::Value>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(v) = serde_json::from_str(&line)
                && tx.send(v).is_err()
            {
                break;
            }
        }
    });
    std::thread::spawn(move || {
        let _ = BufReader::new(stderr).lines().map_while(Result::ok).count();
    });

    let canonical = project.canonicalize().expect("resolves");
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":1,"trace_id":"t1","method":"session.load","params":{{"session_id":null,"project_path":{:?}}}}}"#,
        canonical.to_string_lossy()
    )
    .expect("write session.load");
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":2,"trace_id":"t1","method":"turn.start","params":{{"prompt":"make the change","permission_mode":"manual"}}}}"#
    )
    .expect("write turn.start");
    stdin.flush().expect("flush");

    let mut roles: Vec<String> = Vec::new();
    let mut final_params: Option<serde_json::Value> = None;
    let deadline = std::time::Instant::now() + Duration::from_secs(240);
    while std::time::Instant::now() < deadline && final_params.is_none() {
        match rx.recv_timeout(Duration::from_secs(60)) {
            Ok(frame) => match frame.get("method").and_then(|m| m.as_str()) {
                Some("stage.start") => {
                    let role = frame["params"]["role"]
                        .as_str()
                        .unwrap_or_else(|| panic!("stage.start must carry a role: {frame}"))
                        .to_string();
                    roles.push(role);
                }
                Some("final") => final_params = Some(frame["params"].clone()),
                _ => {}
            },
            Err(e) => {
                guard.kill();
                panic!("no frame from niki serve within 60s: {e}")
            }
        }
    }
    let Some(params) = final_params else {
        guard.kill();
        panic!("the turn never sent `final`")
    };
    assert!(
        params["error"].is_null(),
        "the turn must not fail outright: {params}"
    );
    guard.kill();
    roles
}

/// The real engine, both directions, read off the wire.
///
/// The auditor has to be *emitted*, not merely resolved: `apply_risk_stages` returning
/// a stage nobody runs would satisfy the direct test above while the run did exactly
/// what it always does.
#[test]
fn the_emitted_stage_sequence_escalates_only_for_the_sensitive_task() {
    let repo = create_fixture_repo();
    let project = repo.path().to_path_buf();
    let script = project.join(".niki-mock-script.json");

    // ── sensitive task ───────────────────────────────────────────────
    script_for(&auth_spec_json(), &script);
    let escalated = emitted_stage_roles(&project, &script);
    assert!(
        escalated.contains(&"security_auditor".to_string()),
        "a task touching src/auth must run the auditor the config never asked for; emitted {escalated:?}"
    );
    let auditor_at = escalated
        .iter()
        .position(|r| r == "security_auditor")
        .unwrap();
    let reviewer_at = escalated
        .iter()
        .position(|r| r == "reviewer")
        .unwrap_or_else(|| panic!("the Reviewer must still run: {escalated:?}"));
    assert!(
        auditor_at < reviewer_at,
        "the auditor must be emitted before the Reviewer: {escalated:?}"
    );
    assert_eq!(
        escalated.first().map(String::as_str),
        Some("planner"),
        "the chain still starts at the Planner: {escalated:?}"
    );

    // ── the same engine, a task with nothing sensitive in it ─────────
    script_for(&low_risk_spec_json(), &script);
    let plain = emitted_stage_roles(&project, &script);
    assert_eq!(
        plain.first().map(String::as_str),
        Some("planner"),
        "precondition: the low-risk run still runs a chain: {plain:?}"
    );
    assert!(
        plain.iter().any(|r| r == "reviewer"),
        "precondition: the Reviewer runs on the low-risk task too: {plain:?}"
    );
    assert!(
        !plain.contains(&"security_auditor".to_string()),
        "a Low task must not acquire an auditor; emitted {plain:?}"
    );
    assert!(
        !plain.contains(&"critic".to_string()),
        "a Low task must not acquire a Critic; emitted {plain:?}"
    );

    // And the two sequences genuinely differ, so the positive half was not the same
    // run twice.
    assert_ne!(
        escalated, plain,
        "the risk tier must change what the engine emits"
    );

    std::mem::forget(repo);
}

/// Kills a child however the test leaves — including on a panic, where nothing else
/// runs and a server holding an open stdin pipe would outlive the test binary.
struct ChildGuard(Option<std::process::Child>);

impl ChildGuard {
    fn kill(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.kill();
    }
}
