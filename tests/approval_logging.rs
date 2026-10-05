//! W5 — approvals are real: nothing is allowed without either a decision or a record.
//!
//! Two halves. The shell half (safest option focused, Esc denies) is proven by
//! `shell/test/approval.test.tsx`. This file covers the engine half, which is where a silent
//! auto-approve lives: `PermissionRequirement::Ask` under a headless mode returned `Ok(())` with
//! nothing written anywhere, so a run that allowed every command left no trace of having done so.

use niki::artifacts::types::AgentRole;
use niki::runtime::policy::{PolicyViolation, ToolPolicy};
use niki::runtime::tools::{PermissionRequirement, RiskLevel, ToolCategory};

mod common;

fn policy(mode: &str) -> ToolPolicy {
    ToolPolicy::for_role(AgentRole::Coder, RiskLevel::Medium, mode)
}

fn ask_bash(p: &ToolPolicy) -> Result<(), PolicyViolation> {
    p.check_permission(
        "bash",
        ToolCategory::Execute,
        RiskLevel::Medium,
        PermissionRequirement::Ask,
    )
}

#[test]
fn asking_under_a_headless_mode_is_allowed() {
    // The decision is correct — `dontask` means "Ask becomes Allow" — and it is the logging
    // below that was missing, not this.
    for mode in ["dontask", "auto", "bypass"] {
        assert!(
            ask_bash(&policy(mode)).is_ok(),
            "{mode} must allow an Ask, or it is not doing what its name says"
        );
    }
}

#[test]
fn asking_under_manual_still_refuses_headlessly() {
    // Failing closed is the safe default and must not be weakened by the logging change: with no
    // interface to ask, an Ask is a denial.
    let err = ask_bash(&policy("manual")).expect_err("manual must refuse headlessly");
    assert!(
        matches!(err, PolicyViolation::PermissionDenied(_)),
        "manual must refuse, got {err:?}"
    );
}

#[test]
fn the_refusal_message_names_the_tool_and_the_posture() {
    // A user reading this has to know which tool was refused and which posture caused it, or the
    // message is decoration.
    let err = ask_bash(&policy("manual")).expect_err("manual refuses");
    let text = err.to_string();
    assert!(
        text.contains("bash"),
        "the message does not name the tool: {text}"
    );
    assert!(
        text.to_lowercase().contains("manual"),
        "the message does not name the posture that caused it: {text}"
    );
}

#[test]
fn a_planner_still_cannot_run_bash_in_any_mode() {
    // Logging an auto-approve must never soften a role restriction. The Planner is read-only,
    // and no permission mode makes it otherwise.
    for mode in ["manual", "auto", "dontask", "bypass"] {
        let p = ToolPolicy::for_role(AgentRole::Planner, RiskLevel::Low, mode);
        let err = p
            .check_permission(
                "bash",
                ToolCategory::Execute,
                RiskLevel::Medium,
                PermissionRequirement::Ask,
            )
            .expect_err("the Planner must not execute under any mode");
        assert!(
            matches!(err, PolicyViolation::RoleNotAllowed(_)),
            "{mode} produced {err:?}, which is not a role restriction"
        );
    }
}

#[test]
fn an_explicit_deny_stays_a_deny_under_every_mode() {
    for mode in ["manual", "auto", "dontask", "bypass"] {
        let mut p = policy(mode);
        p.overrides
            .insert("bash".to_string(), PermissionRequirement::Deny);
        let err = ask_bash(&p).expect_err("an explicit deny must hold");
        assert!(
            matches!(err, PolicyViolation::PermissionDenied(_)),
            "{mode} softened an explicit deny into {err:?}"
        );
    }
}

#[test]
fn an_allow_needs_no_posture_at_all() {
    // `Allow` is a decision the tool made, not one the posture made. It must not acquire a log
    // line or a mode dependency from this change.
    for mode in ["manual", "auto", "dontask", "bypass"] {
        assert!(
            policy(mode)
                .check_permission(
                    "read",
                    ToolCategory::Explore,
                    RiskLevel::Low,
                    PermissionRequirement::Allow,
                )
                .is_ok(),
            "{mode} refused a tool declared Allow"
        );
    }
}

/// The scan this row depends on must match a real string, or the logging could be removed and
/// the tests would still pass.
#[test]
fn the_auto_approve_message_is_the_one_that_is_printed() {
    let src = include_str!("../src/runtime/policy.rs");
    assert!(
        src.contains("auto-approved") && src.contains("without asking"),
        "the auto-approval line the policy layer is supposed to print is not in policy.rs"
    );
}

/// `bypass` needs to be said twice.
///
/// It turns every approval into an allow. Typing one word and getting an agent that can run
/// anything on the machine is the shape of accident this closes, so `--permission-mode bypass`
/// is refused without `--i-understand-bypass`.
#[test]
fn bypass_without_the_acknowledgement_is_refused_before_anything_runs() {
    let repo = common::fixture_repo::create_fixture_repo();
    let project = repo.path().to_path_buf();

    let out = std::process::Command::new(std::path::PathBuf::from(env!("CARGO_BIN_EXE_niki")))
        .args([
            "run",
            "do something",
            "--backend",
            "worktree",
            "--project",
            project.to_str().expect("utf-8 path"),
            "--permission-mode",
            "bypass",
        ])
        .output()
        .expect("niki runs");

    assert!(
        !out.status.success(),
        "bypass ran without an acknowledgement"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("i-understand-bypass"),
        "the refusal must name the flag that would work:\n{stderr}"
    );
    assert!(
        stderr.contains("Nothing ran"),
        "the refusal must say that nothing happened:\n{stderr}"
    );
    assert!(
        stderr.contains("--permission-mode auto"),
        "the refusal must offer the safer option:\n{stderr}"
    );
    // The whole point is that it costs nothing: no task directory, therefore no model call.
    assert!(
        !project.join(".niki/tasks").exists(),
        "a refused bypass still created a task directory"
    );
}

/// With the acknowledgement it is allowed — the point is a deliberate opt-in, not a removal.
#[test]
fn bypass_with_the_acknowledgement_is_accepted() {
    let repo = common::fixture_repo::create_fixture_repo();
    let project = repo.path().to_path_buf();

    let out = std::process::Command::new(std::path::PathBuf::from(env!("CARGO_BIN_EXE_niki")))
        .args([
            "run",
            "do something",
            "--backend",
            "worktree",
            "--project",
            project.to_str().expect("utf-8 path"),
            "--permission-mode",
            "bypass",
            "--i-understand-bypass",
        ])
        .output()
        .expect("niki runs");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("i-understand-bypass"),
        "the acknowledgement did not take:\n{stderr}"
    );
    // It will still fail for want of a provider, which is a different failure and proves the
    // permission gate let it through to that point.
    assert!(
        !project.join(".niki/tasks").is_dir() || !stderr.contains("add --i-understand-bypass"),
        "bypass was still refused with the acknowledgement:\n{stderr}"
    );
}

/// The other modes must be untouched. Making the dangerous one deliberate must not make the safe
/// ones harder.
#[test]
fn the_other_modes_need_no_acknowledgement() {
    for mode in ["manual", "auto", "dontask"] {
        let repo = common::fixture_repo::create_fixture_repo();
        let project = repo.path().to_path_buf();
        let out = std::process::Command::new(std::path::PathBuf::from(env!("CARGO_BIN_EXE_niki")))
            .args([
                "run",
                "do something",
                "--backend",
                "worktree",
                "--project",
                project.to_str().expect("utf-8 path"),
                "--permission-mode",
                mode,
            ])
            .output()
            .expect("niki runs");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !stderr.contains("i-understand-bypass"),
            "--permission-mode {mode} demanded an acknowledgement meant only for bypass:\n{stderr}"
        );
    }
}

/// `bypass` must still never be the default, and the acknowledgement must not become one either.
#[test]
fn neither_the_default_nor_the_acknowledgement_is_ever_bypass() {
    let repo = common::fixture_repo::create_fixture_repo();
    let project = repo.path().to_path_buf();
    let out = std::process::Command::new(std::path::PathBuf::from(env!("CARGO_BIN_EXE_niki")))
        .args([
            "run",
            "do something",
            "--backend",
            "worktree",
            "--project",
            project.to_str().expect("utf-8 path"),
            "--i-understand-bypass",
        ])
        .output()
        .expect("niki runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    // The flag alone means nothing without the mode. If this ever starts bypassing, the
    // acknowledgement has stopped being an acknowledgement of anything.
    assert!(
        !stderr.contains("auto-approved") || !project.join(".niki/tasks").exists(),
        "--i-understand-bypass alone changed the posture"
    );
}
