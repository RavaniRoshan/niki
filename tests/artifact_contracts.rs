//! Phase 1.5 — all-roles contract roundtrip harness (CI gate).
//!
//! Every role's JSON contract must BOTH validate against its JSON Schema
//! (via the `jsonschema` crate, same path as production `validate_artifact`)
//! AND parse via `parse_role` (the same function the pipeline uses).
//! A Synthesizer fixture covers the parallel-coder path (Phase 1.1) and a
//! Critic fixture covers the post-1.2 grounded prompt.
//!
//! The template-variable lint renders every prompt with its real context shape
//! and fails on unknown/missing variables, plus pins the Phase 1.2/1.3 fixes:
//! Critic shows spec/verdict text, `task_relevant_context` is gone,
//! `mcp_tools` is visible to the core chain.

use niki::artifacts::types::AgentRole;
use niki::artifacts::validate::validate_artifact;
use niki::orchestrator::pipeline::parse_role;
use serde_json::json;

fn planner_fixture() -> String {
    json!({
        "summary": "Fix off-by-one in paginate",
        "approach": "Change slice bound from start + size - 1 to start + size.",
        "files_to_modify": [
            {"path": "src/list.rs", "action": "modify", "description": "Fix slice bound"}
        ],
        "acceptance_criteria": ["Last page item returned", "No panic on edge cases"],
        "constraints": ["Only modify src/list.rs"],
        "estimated_complexity": "low"
    })
    .to_string()
}

fn coder_fixture() -> String {
    json!({
        "edits": [
            {"search": "let end = start + size - 1;", "replace": "let end = start + size;"}
        ],
        "files_changed": [
            {"path": "src/list.rs", "action": "modify", "language": "rust"}
        ],
        "implementation_notes": "Adjusted slice bound.",
        "spec_adherence": "Matches spec."
    })
    .to_string()
}

fn tester_fixture() -> String {
    json!({
        "tests_written": [
            {
                "name": "test_last_page_item",
                "file_path": "tests/list_test.rs",
                "description": "Verify last item returned",
                "status": "passed",
                "error_message": null,
                "oracle_source": "spec",
                "spec_reference": "Last page item is returned correctly"
            }
        ],
        "test_results": {"total": 1, "passed": 1, "failed": 0, "skipped": 0, "errors": 0},
        "coverage_summary": null,
        "edge_cases_found": ["Empty slice boundary"],
        "tester_notes": "Tests pass."
    })
    .to_string()
}

fn reviewer_fixture() -> String {
    json!({
        "verdict": "approved",
        "overall_assessment": "Clean fix.",
        "quality_scores": {"correctness": 9, "code_quality": 9, "test_coverage": 8, "spec_adherence": 10},
        "issues": [],
        "strengths": ["Minimal change"],
        "feedback": null,
        "red_reconciliation": null
    })
    .to_string()
}

fn reviewer_correctness_boundary_fixture() -> String {
    // Phase 1.4 regression: `correctness`/`boundary` categories must validate
    // (they exist on the Rust `IssueCategory` side).
    json!({
        "verdict": "revision_needed",
        "overall_assessment": "Needs correctness check.",
        "quality_scores": {"correctness": 5, "code_quality": 7, "test_coverage": 6, "spec_adherence": 7},
        "issues": [
            {
                "severity": "major",
                "category": "correctness",
                "file_path": "src/list.rs",
                "line_range": "1-4",
                "description": "Off-by-one on boundary",
                "suggested_fix": null
            },
            {
                "severity": "minor",
                "category": "boundary",
                "file_path": "src/list.rs",
                "line_range": "1-4",
                "description": "Empty input edge",
                "suggested_fix": null
            }
        ],
        "strengths": [],
        "feedback": null,
        "red_reconciliation": null
    })
    .to_string()
}

fn synthesizer_fixture() -> String {
    // Parallel-coder path: `merged` mirrors `CodeDiff` exactly (Phase 1.1).
    json!({
        "merged": {
            "edits": [
                {"search": "let end = start + size - 1;", "replace": "let end = start + size;"}
            ],
            "files_changed": [
                {"path": "src/list.rs", "action": "modify", "language": "rust"}
            ],
            "implementation_notes": "Synthesized from parallel coders.",
            "spec_adherence": "Matches the spec."
        },
        "reconciliation_notes": "Both coders agreed.",
        "sources_merged": 2
    })
    .to_string()
}

fn security_fixture() -> String {
    // `strengths` is required on both sides after Phase 1.4.
    json!({
        "verdict": "approved",
        "overall_assessment": "No security issues.",
        "findings": [],
        "strengths": ["No secrets in diff"]
    })
    .to_string()
}

fn red_fixture() -> String {
    json!({
        "overall_red_assessment": "Change looks safe.",
        "challenges": []
    })
    .to_string()
}

fn critic_fixture() -> String {
    json!({
        "disposition": "approve",
        "summary": "All cited files exist and issues trace to diff evidence.",
        "unsupported_claims": [],
        "confirmed_findings": ["off-by-one fix verified in diff"]
    })
    .to_string()
}

fn check_roundtrip(role: AgentRole, schema: &str, fixture: &str) {
    validate_artifact(fixture, schema).unwrap_or_else(|e| {
        panic!("{role:?} fixture failed schema validation ({schema}): {e}\n{fixture}")
    });
    parse_role(role, fixture)
        .unwrap_or_else(|e| panic!("{role:?} fixture failed parse_role: {e}\n{fixture}"));
}

#[test]
fn artifact_contracts() {
    check_roundtrip(
        AgentRole::Planner,
        "schemas/task_spec.schema.json",
        &planner_fixture(),
    );
    check_roundtrip(
        AgentRole::Coder,
        "schemas/code_diff.schema.json",
        &coder_fixture(),
    );
    check_roundtrip(
        AgentRole::Tester,
        "schemas/test_report.schema.json",
        &tester_fixture(),
    );
    check_roundtrip(
        AgentRole::Reviewer,
        "schemas/review_verdict.schema.json",
        &reviewer_fixture(),
    );
    check_roundtrip(
        AgentRole::Synthesizer,
        "schemas/synthesis.schema.json",
        &synthesizer_fixture(),
    );
    check_roundtrip(
        AgentRole::SecurityAuditor,
        "schemas/security_audit.schema.json",
        &security_fixture(),
    );
    check_roundtrip(
        AgentRole::Red,
        "schemas/red_challenge.schema.json",
        &red_fixture(),
    );
    check_roundtrip(
        AgentRole::Critic,
        "schemas/critique.schema.json",
        &critic_fixture(),
    );
}

#[test]
fn synthesis_contract() {
    // Narrow gate for Phase 1.1: the parallel-coder Synthesizer fixture must
    // validate AND parse (the old `unified_diff` shape failed one side).
    check_roundtrip(
        AgentRole::Synthesizer,
        "schemas/synthesis.schema.json",
        &synthesizer_fixture(),
    );
}

#[test]
fn issue_category_correctness_boundary_validates() {
    // Phase 1.4: struct-valid `correctness`/`boundary` categories must not abort
    // schema validation.
    check_roundtrip(
        AgentRole::Reviewer,
        "schemas/review_verdict.schema.json",
        &reviewer_correctness_boundary_fixture(),
    );
}

#[test]
fn prompt_vars() {
    use minijinja::{Environment, context};
    use std::path::PathBuf;

    // No referenced-never-supplied variable may remain.
    let prompt_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("prompts");
    let mut all_text = String::new();
    for entry in std::fs::read_dir(&prompt_dir).expect("prompts dir readable") {
        let entry = entry.unwrap();
        let text = std::fs::read_to_string(entry.path()).unwrap();
        all_text.push_str(&text);
        assert!(
            !text.contains("task_relevant_context"),
            "{} still references removed task_relevant_context",
            entry.path().display()
        );
    }
    // `mcp_tools` must be supplied somewhere AND used somewhere (Phase 1.3).
    assert!(
        all_text.contains("mcp_tools"),
        "no prompt interpolates mcp_tools after Phase 1.3"
    );
    // Critic must see evidence (Phase 1.2): spec + coder + tester + reviewer.
    let critic = std::fs::read_to_string(prompt_dir.join("critic.md")).unwrap();
    for needle in [
        "input_artifacts[0]",
        "input_artifacts[1]",
        "input_artifacts[2]",
        "input_artifacts[3]",
    ] {
        assert!(critic.contains(needle), "critic.md missing {needle}");
    }

    // Render every prompt with a full mock context; unknown/missing vars fail.
    let templates = [
        "planner.md",
        "coder.md",
        "tester.md",
        "reviewer.md",
        "synthesizer.md",
        "security_auditor.md",
        "red.md",
        "critic.md",
        "solo.md",
    ];
    let spec_marker = "SPEC-MARKER-12345";
    let coder_marker = "CODER-EVIDENCE-67890";
    let tester_marker = "TESTER-REPORT-abcde";
    let reviewer_marker = "REVIEWER-VERDICT-fghij";
    for name in templates {
        let src = std::fs::read_to_string(prompt_dir.join(name)).unwrap();
        let mut env = Environment::new();
        env.add_template("t", &src).unwrap();
        let tmpl = env.get_template("t").unwrap();
        let rendered = tmpl
            .render(context! {
                task_description => "do the thing",
                task_spec_json => "{\"summary\":\"x\"}",
                input_artifacts => vec![
                    format!("{{\"spec\":\"{spec_marker}\"}}"),
                    format!("{{\"coder\":\"{coder_marker}\"}}"),
                    format!("{{\"tester\":\"{tester_marker}\"}}"),
                    format!("{{\"reviewer\":\"{reviewer_marker}\"}}"),
                    "{\"red\":\"R1\"}".to_string(),
                ],
                revision_context => Option::<String>::None,
                revision_round => 0,
                project_knowledge => "knowledge",
                project_memory => "memory",
                current_files => "files",
                mcp_tools => "tool: read",
                artifact_schema => "{\"type\":\"object\"}",
                diff_guardrail_hint => Option::<String>::None,
            })
            .unwrap_or_else(|e| panic!("{name} failed to render with full ctx: {e}"));
        assert!(
            !rendered.contains("{{") && !rendered.contains("}}"),
            "{name} has unrendered variables"
        );
    }
}

#[test]
fn critic_render() {
    // Phase 1.2 acceptance: the rendered critic prompt contains spec/verdict text.
    use minijinja::{Environment, context};

    let prompt_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("prompts");
    let src = std::fs::read_to_string(prompt_dir.join("critic.md")).unwrap();
    let mut env = Environment::new();
    env.add_template("critic", &src).unwrap();
    let tmpl = env.get_template("critic").unwrap();
    let spec_text = "SPEC-GROUNDING-CHECK";
    let verdict_text = "VERDICT-UNDER-TEST";
    let rendered = tmpl
        .render(context! {
            input_artifacts => vec![
                format!("{{\"spec\":\"{spec_text}\"}}"),
                "{\"coder\":\"evidence\"}".to_string(),
                "{\"tester\":\"report\"}".to_string(),
                format!("{{\"reviewer\":\"{verdict_text}\"}}"),
            ],
            project_knowledge => "knowledge",
            project_memory => "memory",
            mcp_tools => "",
            artifact_schema => "{\"type\":\"object\"}",
        })
        .expect("critic renders");
    assert!(
        rendered.contains(spec_text),
        "critic render missing spec text"
    );
    assert!(
        rendered.contains(verdict_text),
        "critic render missing verdict text"
    );
}
