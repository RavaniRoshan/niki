#![allow(dead_code, clippy::wrong_self_convention)]

use serde_json::json;
use std::fs::File;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct MockEntry {
    pub text: Option<String>,
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub error: Option<MockError>,
}

#[derive(Debug, Clone)]
pub struct MockError {
    pub kind: String,
    pub message: String,
}

#[derive(Default)]
pub struct MockScriptBuilder {
    scripts: serde_json::Map<String, serde_json::Value>,
}

impl MockScriptBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_response(
        mut self,
        model: &str,
        text: &str,
        tokens_in: u32,
        tokens_out: u32,
    ) -> Self {
        let responses = self
            .scripts
            .entry(model.to_string())
            .or_insert_with(|| json!({"responses": []}));
        if let Some(arr) = responses
            .get_mut("responses")
            .and_then(|v| v.as_array_mut())
        {
            arr.push(json!({
                "text": text,
                "input_tokens": tokens_in,
                "output_tokens": tokens_out
            }));
        }
        self
    }

    pub fn add_error(mut self, model: &str, kind: &str, message: &str) -> Self {
        let responses = self
            .scripts
            .entry(model.to_string())
            .or_insert_with(|| json!({"responses": []}));
        if let Some(arr) = responses
            .get_mut("responses")
            .and_then(|v| v.as_array_mut())
        {
            arr.push(json!({
                "error": {"kind": kind, "message": message}
            }));
        }
        self
    }

    pub fn add_raw_response(mut self, model: &str, entry: serde_json::Value) -> Self {
        let responses = self
            .scripts
            .entry(model.to_string())
            .or_insert_with(|| json!({"responses": []}));
        if let Some(arr) = responses
            .get_mut("responses")
            .and_then(|v| v.as_array_mut())
        {
            arr.push(entry);
        }
        self
    }

    pub fn write(self, path: &PathBuf) -> PathBuf {
        let script = json!({ "models": self.scripts });
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = File::create(path).unwrap();
        serde_json::to_writer_pretty(&mut f, &script).unwrap();
        path.clone()
    }

    pub fn to_json_string(self) -> String {
        serde_json::to_string_pretty(&json!({ "models": self.scripts })).unwrap()
    }
}

pub fn task_spec_json() -> String {
    json!({
        "summary": "Fix off-by-one in paginate",
        "approach": "Change the slice upper bound from start + size - 1 to start + size to include the final element.",
        "files_to_modify": [
            {
                "path": "src/list.rs",
                "action": "modify",
                "description": "Fix the slice upper bound"
            }
        ],
        "acceptance_criteria": [
            "The last page item is returned correctly",
            "No panic on edge cases"
        ],
        "constraints": [
            "Only modify src/list.rs",
            "Preserve existing function signature"
        ],
        "estimated_complexity": "low"
    })
    .to_string()
}

pub fn code_diff_json(search: &str, replace: &str, path: &str) -> String {
    json!({
        "edits": [
            {"search": search, "replace": replace}
        ],
        "files_changed": [
            {"path": path, "action": "modify", "language": "rust"}
        ],
        "implementation_notes": "Fixed the off-by-one by adjusting the slice bound.",
        "spec_adherence": "Matches the spec: the slice now includes the last element."
    })
    .to_string()
}

pub fn test_report_json() -> String {
    json!({
        "tests_written": [
            {
                "name": "test_last_page_item",
                "file_path": "tests/list_test.rs",
                "description": "Verify the last item of a page is returned",
                "status": "passed",
                "error_message": null
            }
        ],
        "test_results": {
            "total": 1,
            "passed": 1,
            "failed": 0,
            "skipped": 0,
            "errors": 0
        },
        "coverage_summary": null,
        "edge_cases_found": [
            "Empty slice boundary"
        ],
        "tester_notes": "Tests pass; the fix correctly includes the last item."
    })
    .to_string()
}

pub fn review_verdict_approved_json() -> String {
    json!({
        "verdict": "approved",
        "overall_assessment": "Clean fix. The off-by-one is resolved.",
        "quality_scores": {
            "correctness": 9,
            "code_quality": 9,
            "test_coverage": 8,
            "spec_adherence": 10
        },
        "issues": [],
        "strengths": [
            "Minimal change",
            "Addresses root cause"
        ],
        "feedback": null,
        "red_reconciliation": null
    })
    .to_string()
}

pub fn review_verdict_revision_json(issue: &str) -> String {
    json!({
        "verdict": "revision_needed",
        "overall_assessment": "An issue needs fixing before approval.",
        "quality_scores": {
            "correctness": 6,
            "code_quality": 7,
            "test_coverage": 6,
            "spec_adherence": 8
        },
        "issues": [
            {
                "severity": "major",
                "category": "logic",
                "file_path": "src/list.rs",
                "line_range": "28-32",
                "description": issue,
                "suggested_fix": null
            }
        ],
        "strengths": [],
        "feedback": {
            "critical_issues": [
                {
                    "severity": "major",
                    "category": "logic",
                    "file_path": "src/list.rs",
                    "line_range": "28-32",
                    "description": issue,
                    "suggested_fix": null
                }
            ],
            "guidance": "Fix the issue before resubmitting.",
            "keep_unchanged": [],
            "revision_round": 0
        },
        "red_reconciliation": null
    })
    .to_string()
}

pub fn red_challenge_json_empty() -> String {
    json!({
        "overall_red_assessment": "Change looks safe; no adversarial concerns.",
        "challenges": []
    })
    .to_string()
}

pub fn red_challenge_json_with_issue(claim: &str) -> String {
    json!({
        "overall_red_assessment": "Potentially risky: an adversarial concern was raised.",
        "challenges": [
            {
                "id": "R1",
                "severity": "major",
                "category": "logic",
                "claim": claim,
                "confidence": 8,
                "evidence": null,
                "suggested_check": null
            }
        ]
    })
    .to_string()
}

pub fn synthesis_json() -> String {
    json!({
        "merged": {
            "edits": [],
            "unified_diff": "",
            "files_changed": [],
            "implementation_notes": "Synthesized from parallel coders.",
            "spec_adherence": "Matches the spec."
        },
        "reconciliation_notes": "Both coders agreed on the fix.",
        "sources_merged": 2
    })
    .to_string()
}

pub fn security_verdict_json() -> String {
    json!({
        "verdict": "approved",
        "overall_assessment": "No security issues found.",
        "findings": [],
        "strengths": ["No secrets in diff", "No dangerous patterns"]
    })
    .to_string()
}

pub fn mock_script_for_happy_path(script_path: &PathBuf) -> PathBuf {
    MockScriptBuilder::new()
        .add_response(
            "mock-planner",
            &format!("```json\n{}\n```", task_spec_json()),
            80,
            120,
        )
        .add_response(
            "mock-coder",
            &format!(
                "```json\n{}\n```",
                code_diff_json(
                    "let end = start + size - 1;",
                    "let end = start + size;",
                    "src/list.rs"
                )
            ),
            200,
            80,
        )
        .add_response(
            "mock-tester",
            &format!("```json\n{}\n```", test_report_json()),
            100,
            60,
        )
        .add_response(
            "mock-red",
            &format!("```json\n{}\n```", red_challenge_json_empty()),
            60,
            20,
        )
        .add_response(
            "mock-reviewer",
            &format!("```json\n{}\n```", review_verdict_approved_json()),
            150,
            50,
        )
        .add_response(
            "mock-planner2",
            &format!("```json\n{}\n```", task_spec_json()),
            80,
            120,
        )
        .add_response(
            "mock-coder2",
            &format!(
                "```json\n{}\n```",
                code_diff_json(
                    "let end = start + size - 1;",
                    "let end = start + size;",
                    "src/list.rs"
                )
            ),
            200,
            80,
        )
        .add_response(
            "mock-tester2",
            &format!("```json\n{}\n```", test_report_json()),
            100,
            60,
        )
        .add_response(
            "mock-red2",
            &format!("```json\n{}\n```", red_challenge_json_empty()),
            60,
            20,
        )
        .add_response(
            "mock-reviewer2",
            &format!("```json\n{}\n```", review_verdict_approved_json()),
            150,
            50,
        )
        .write(script_path)
}

pub fn mock_script_for_revision(script_path: &PathBuf) -> PathBuf {
    MockScriptBuilder::new()
        .add_response(
            "mock-planner",
            &format!("```json\n{}\n```", task_spec_json()),
            80,
            120,
        )
        .add_response(
            "mock-coder",
            &format!(
                "```json\n{}\n```",
                code_diff_json(
                    "let end = start + size - 1;",
                    "let end = start + size;",
                    "src/list.rs"
                )
            ),
            200,
            80,
        )
        .add_response(
            "mock-tester",
            &format!("```json\n{}\n```", test_report_json()),
            100,
            60,
        )
        .add_response(
            "mock-red",
            &format!("```json\n{}\n```", red_challenge_json_empty()),
            60,
            20,
        )
        .add_response(
            "mock-reviewer",
            &format!(
                "```json\n{}\n```",
                review_verdict_revision_json("The fix needs more tests.")
            ),
            150,
            60,
        )
        .add_response(
            "mock-coder2",
            &format!(
                "```json\n{}\n```",
                code_diff_json(
                    "let end = start + size - 1;",
                    "let end = start + size;",
                    "src/list.rs"
                )
            ),
            100,
            40,
        )
        .add_response(
            "mock-tester2",
            &format!("```json\n{}\n```", test_report_json()),
            100,
            60,
        )
        .add_response(
            "mock-red2",
            &format!("```json\n{}\n```", red_challenge_json_empty()),
            60,
            20,
        )
        .add_response(
            "mock-reviewer2",
            &format!("```json\n{}\n```", review_verdict_approved_json()),
            150,
            50,
        )
        .write(script_path)
}
