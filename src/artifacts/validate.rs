//! Artifact validation — the negative path this module never had.
//!
//! `tests/artifact_contracts.rs` fed eight *valid* artifacts through
//! `validate_artifact` and asserted they passed. Nothing ever asserted that a
//! malformed, wrongly-typed, or semantically empty artifact was **rejected**,
//! so the entire failure branch of the schema gate was unverified.
//!
//! Two classes of defect are covered here:
//!
//! * **Shape.** Type errors, missing required fields, unexpected properties.
//! * **Semantics.** A structurally valid artifact that says nothing. JSON
//!   Schema cannot express these, so they are checked here: a code diff with
//!   no edits, a verdict with neither issues nor an assessment, a Red agent
//!   that raised challenges and reconciled none of them.

use anyhow::Result;
use serde_json::Value;

/// Validate an artifact against its JSON schema, then apply the cross-field
/// and semantic rules a schema cannot express.
pub fn validate_artifact(json_str: &str, schema_path: &str) -> Result<()> {
    let schema_content = crate::load_asset(schema_path)?;
    let schema_json: Value = serde_json::from_str(&schema_content)
        .map_err(|e| anyhow::anyhow!("Failed to parse schema JSON: {}", e))?;
    let artifact_json: Value = serde_json::from_str(json_str)
        .map_err(|e| anyhow::anyhow!("Failed to parse artifact JSON: {}", e))?;

    // Use the explicit validator rather than `jsonschema::is_valid`. The
    // boolean helper disagreed with `validator_for(..).iter_errors(..)` on
    // documents the latter accepts, and its failure message ("fields present /
    // schema requires") named neither the offending field nor the reason — so a
    // schema mistake was near-impossible to diagnose from the output.
    let compiled = jsonschema::validator_for(&schema_json)
        .map_err(|e| anyhow::anyhow!("{schema_path} is not a usable JSON Schema: {e}"))?;
    let errors: Vec<String> = compiled
        .iter_errors(&artifact_json)
        .map(|e| e.to_string())
        .collect();
    if !errors.is_empty() {
        let (artifact_keys, schema_required) = extract_field_info(&artifact_json, &schema_json);
        return Err(anyhow::anyhow!(
            "Artifact does not match {schema_path}. {}\n  (fields present: [{}]; required: [{}])",
            errors.join("; "),
            artifact_keys,
            schema_required
        ));
    }

    check_semantics(&artifact_json, schema_path)
}

/// Reject a structurally valid artifact that carries no information.
///
/// JSON Schema validates shape, not meaning. Before this existed, a model
/// returning `{"edits": [], "files_changed": [], "notes": ""}` passed cleanly
/// and the run was recorded as a successful implementation.
fn check_semantics(artifact: &Value, schema_path: &str) -> Result<()> {
    let Some(obj) = artifact.as_object() else {
        return Ok(());
    };

    // A code diff with no edits is not a diff.
    if let Some(edits) = obj.get("edits").and_then(|e| e.as_array()) {
        if edits.is_empty() {
            return Err(anyhow::anyhow!(
                "{schema_path}: `edits` is empty. An artifact that changes nothing is not an \
                 implementation, and recording it as one makes a no-op run indistinguishable \
                 from real work."
            ));
        }
        if let Some(files) = obj.get("files_changed").and_then(|f| f.as_array())
            && files.is_empty()
        {
            return Err(anyhow::anyhow!(
                "{schema_path}: `files_changed` is empty while `edits` is not. The artifact \
                 claims changes but names no file."
            ));
        }
        // An edit whose search and replace are identical changes nothing.
        for (i, e) in edits.iter().enumerate() {
            let search = e.get("search").and_then(|s| s.as_str()).unwrap_or_default();
            let replace = e
                .get("replace")
                .and_then(|r| r.as_str())
                .unwrap_or_default();
            if search == replace {
                return Err(anyhow::anyhow!(
                    "{schema_path}: edits[{i}] has identical `search` and `replace`. A \
                     replacement equal to what it replaces is a no-op."
                ));
            }
            if search.trim().is_empty() {
                return Err(anyhow::anyhow!(
                    "{schema_path}: edits[{i}] has an empty `search`, which cannot anchor to \
                     anything in the file."
                ));
            }
            // A replacement that BEGINS with its own search can never converge:
            // applying it leaves the search text in place, so the next round
            // matches again and inserts the same thing. Observed live, where a
            // revision round emitted a truncated `replace` that re-inserted the
            // prefix it had matched, and three rounds later the file held
            //
            //     numbers.iter().sum()    numbers.iter().sum()    numbers.iter().sum()
            //
            // The `search == replace` rule above is the degenerate case of this
            // one and is kept as its own message because it is the clearer
            // thing to say. This catches the general form, which is what a
            // truncated response actually looks like.
            if replace.starts_with(search) {
                return Err(anyhow::anyhow!(
                    "{schema_path}: edits[{i}] replaces {search:?} with text that still begins \
                     with {search:?}. Applying this edit leaves the search text in place, so it \
                     would match again on the next round and keep inserting — it cannot \
                     converge. This usually means the `replace` was truncated."
                ));
            }
        }
    }

    // A verdict that approves with nothing to say is not a review.
    if let Some(verdict) = obj.get("verdict").and_then(|v| v.as_str()) {
        let issues = obj
            .get("issues")
            .and_then(|i| i.as_array())
            .map(|a| a.len());
        let strengths = obj
            .get("strengths")
            .and_then(|s| s.as_array())
            .map(|a| a.len());
        let assessment = obj
            .get("overall_assessment")
            .or_else(|| obj.get("summary"))
            .and_then(|a| a.as_str())
            .unwrap_or_default();
        if issues == Some(0) && strengths == Some(0) && assessment.trim().is_empty() {
            return Err(anyhow::anyhow!(
                "{schema_path}: verdict `{verdict}` carries no issues, no strengths and no \
                 assessment. A verdict with no content is indistinguishable from a default."
            ));
        }
        // An "approved" verdict alongside blocking issues is self-contradictory.
        if verdict.eq_ignore_ascii_case("approved") {
            let blocking = obj
                .get("issues")
                .and_then(|i| i.as_array())
                .map(|a| {
                    a.iter().any(|i| {
                        i.get("severity")
                            .and_then(|s| s.as_str())
                            .is_some_and(|s| s.eq_ignore_ascii_case("critical"))
                    })
                })
                .unwrap_or(false);
            if blocking {
                return Err(anyhow::anyhow!(
                    "{schema_path}: verdict `approved` but a critical issue is present. The \
                     verdict must be derived from the issue list, not asserted alongside it."
                ));
            }
        }
    }

    // Red/Blue reconciliations must name a challenge, and must not name the
    // same one twice — a duplicate is a challenge "answered" by copying the
    // same rationale.
    //
    // Note the limit of this check: `challenges` lives in the Red artifact and
    // `red_reconciliation` in the Reviewer's, so "every challenge was
    // reconciled" is a *cross-artifact* rule and cannot be decided here. It is
    // covered by INV-STAGE-MANIFEST in tests/reverse/invariants.rs.
    if let Some(reconciled) = obj.get("red_reconciliation").and_then(|r| r.as_array()) {
        let mut seen: Vec<&str> = Vec::new();
        for (i, r) in reconciled.iter().enumerate() {
            let id = r
                .get("challenge_id")
                .and_then(|c| c.as_str())
                .unwrap_or_default();
            if id.trim().is_empty() {
                return Err(anyhow::anyhow!(
                    "{schema_path}: red_reconciliation[{i}] names no `challenge_id`, so it \
                     cannot be tied back to the challenge it answers."
                ));
            }
            if seen.contains(&id) {
                return Err(anyhow::anyhow!(
                    "{schema_path}: red_reconciliation answers challenge `{id}` more than once. \
                     A duplicated answer is not two answers."
                ));
            }
            seen.push(id);
        }
    }

    Ok(())
}

fn extract_field_info(artifact: &Value, schema: &Value) -> (String, String) {
    let artifact_keys = match artifact {
        Value::Object(map) => map.keys().cloned().collect::<Vec<_>>().join(", "),
        _ => "(not an object)".to_string(),
    };
    let schema_required = match schema.get("required") {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        _ => {
            // Try properties
            match schema.get("properties") {
                Some(Value::Object(map)) => map.keys().cloned().collect::<Vec<_>>().join(", "),
                _ => "(unknown)".to_string(),
            }
        }
    };
    (artifact_keys, schema_required)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = "schemas/code_diff.schema.json";
    const VERDICT: &str = "schemas/review_verdict.schema.json";
    const RED: &str = "schemas/red_challenge.schema.json";

    fn ok(json: &str, schema: &str) {
        if let Err(e) = validate_artifact(json, schema) {
            panic!("expected this artifact to be accepted, got: {e}");
        }
    }

    fn rejected(json: &str, schema: &str) -> String {
        match validate_artifact(json, schema) {
            Ok(()) => panic!("expected this artifact to be REJECTED, but it validated"),
            Err(e) => e.to_string(),
        }
    }

    // ── Positive cases ───────────────────────────────────────────
    #[test]
    fn a_real_code_diff_is_accepted() {
        ok(
            r#"{
                "edits": [{"search": "let end = start + size - 1;", "replace": "let end = start + size;"}],
                "files_changed": [{"path": "src/list.rs", "action": "modify", "language": "rust"}],
                "implementation_notes": "removed the off-by-one",
                "spec_adherence": "fixes the reported bug"
            }"#,
            DIFF,
        );
    }

    #[test]
    fn a_rejected_verdict_with_content_is_accepted() {
        ok(
            r#"{
                "verdict": "rejected",
                "overall_assessment": "The diff introduces an off-by-one in the empty case.",
                "quality_scores": {"correctness": 3, "code_quality": 7, "test_coverage": 5, "spec_adherence": 4},
                "issues": [{"severity": "critical", "category": "correctness", "description": "off-by-one", "file": "src/list.rs", "line": 4}],
                "strengths": []
            }"#,
            VERDICT,
        );
    }

    #[test]
    fn an_approved_verdict_with_an_assessment_is_accepted() {
        ok(
            r#"{
                "verdict": "approved",
                "overall_assessment": "The fix is correct and covered by the new test.",
                "quality_scores": {"correctness": 9, "code_quality": 8, "test_coverage": 9, "spec_adherence": 9},
                "issues": [],
                "strengths": ["clear diff", "test added"]
            }"#,
            VERDICT,
        );
    }

    // ── Shape: the negative path was never asserted ──────────────
    #[test]
    fn a_missing_required_field_is_rejected() {
        let e = rejected(
            r#"{"edits": [{"search": "a", "replace": "b"}], "files_changed": [{"path": "x", "action": "modify"}]}"#,
            DIFF,
        );
        assert!(e.contains("schema"), "{e}");
    }

    #[test]
    fn a_wrongly_typed_field_is_rejected() {
        rejected(
            r#"{"edits": "not an array", "files_changed": [], "implementation_notes": "x", "spec_adherence": "y"}"#,
            DIFF,
        );
    }

    #[test]
    fn an_unexpected_property_is_rejected() {
        rejected(
            r#"{"edits": [{"search": "a", "replace": "b"}], "files_changed": [{"path": "x", "action": "modify"}], "implementation_notes": "x", "spec_adherence": "y", "surprise": 1}"#,
            DIFF,
        );
    }

    #[test]
    fn an_invalid_enum_value_is_rejected() {
        rejected(
            r#"{"edits": [{"search": "a", "replace": "b"}], "files_changed": [{"path": "x", "action": "teleport"}], "implementation_notes": "x", "spec_adherence": "y"}"#,
            DIFF,
        );
    }

    #[test]
    fn malformed_json_is_rejected() {
        rejected("{not json at all", DIFF);
    }

    // ── Semantics: valid shape, no meaning ───────────────────────
    #[test]
    fn an_empty_diff_is_rejected() {
        // This exact payload is the `valid_schema_empty_semantics` fault, and
        // it validated cleanly before the schema gained minItems. It is now
        // rejected by the schema itself, before the semantic check runs.
        let e = rejected(
            r#"{"edits": [], "files_changed": [], "implementation_notes": "", "spec_adherence": ""}"#,
            DIFF,
        );
        assert!(e.contains("less than 1 item"), "{e}");
    }

    #[test]
    fn a_diff_claiming_changes_but_naming_no_file_is_rejected() {
        let e = rejected(
            r#"{"edits": [{"search": "a", "replace": "b"}], "files_changed": [], "implementation_notes": "x", "spec_adherence": "y"}"#,
            DIFF,
        );
        assert!(e.contains("less than 1 item"), "{e}");
    }

    #[test]
    fn a_self_cancelling_edit_is_rejected() {
        let e = rejected(
            r#"{"edits": [{"search": "same", "replace": "same"}], "files_changed": [{"path": "x", "action": "modify"}], "implementation_notes": "x", "spec_adherence": "y"}"#,
            DIFF,
        );
        assert!(e.contains("no-op"), "{e}");
    }

    #[test]
    fn an_edit_with_an_empty_anchor_is_rejected() {
        let e = rejected(
            r#"{"edits": [{"search": "   ", "replace": "b"}], "files_changed": [{"path": "x", "action": "modify"}], "implementation_notes": "x", "spec_adherence": "y"}"#,
            DIFF,
        );
        assert!(e.contains("cannot anchor"), "{e}");
    }

    #[test]
    fn a_contentless_approved_verdict_is_rejected() {
        let e = rejected(
            r#"{
                "verdict": "approved",
                "overall_assessment": "",
                "quality_scores": {"correctness": 10, "code_quality": 10, "test_coverage": 10, "spec_adherence": 10},
                "issues": [],
                "strengths": []
            }"#,
            VERDICT,
        );
        assert!(e.contains("shorter than 1 character"), "{e}");
    }

    #[test]
    fn approved_with_a_critical_issue_is_rejected() {
        // The `verdict_contradicts_issues` fault. The verdict must be derived
        // from the issue list rather than asserted alongside it.
        let e = rejected(
            r#"{
                "verdict": "approved",
                "overall_assessment": "Looks good to me.",
                "quality_scores": {"correctness": 9, "code_quality": 9, "test_coverage": 9, "spec_adherence": 9},
                "issues": [{"severity": "critical", "category": "security", "description": "command injection", "file": "src/x.rs", "line": 1}],
                "strengths": []
            }"#,
            VERDICT,
        );
        assert!(e.contains("must be derived from the issue list"), "{e}");
    }

    #[test]
    fn a_reconciliation_naming_no_challenge_is_rejected() {
        let e = rejected(
            r#"{
                "verdict": "approved",
                "overall_assessment": "Fine.",
                "quality_scores": {"correctness": 9, "code_quality": 9, "test_coverage": 9, "spec_adherence": 9},
                "issues": [],
                "strengths": ["ok"],
                "red_reconciliation": [{"disposition": "refuted", "rationale": "not real"}]
            }"#,
            VERDICT,
        );
        assert!(e.contains("challenge_id"), "{e}");
    }

    #[test]
    fn a_duplicated_reconciliation_is_rejected() {
        let e = rejected(
            r#"{
                "verdict": "approved",
                "overall_assessment": "Fine.",
                "quality_scores": {"correctness": 9, "code_quality": 9, "test_coverage": 9, "spec_adherence": 9},
                "issues": [],
                "strengths": ["ok"],
                "red_reconciliation": [
                    {"challenge_id": "c1", "disposition": "refuted", "rationale": "a"},
                    {"challenge_id": "c1", "disposition": "upheld", "rationale": "b"}
                ]
            }"#,
            VERDICT,
        );
        assert!(e.contains("more than once"), "{e}");
    }

    #[test]
    fn a_red_challenge_artifact_is_accepted() {
        ok(
            r#"{
                "overall_red_assessment": "Probed the exec path and the diff scope.",
                "challenges": [
                    {"id": "c1", "severity": "critical", "category": "security",
                     "claim": "Command injection in the exec path", "confidence": 8}
                ]
            }"#,
            RED,
        );
    }

    /// The semantic rules must not reject an artifact that carries real
    /// content. A validator that refuses good work gets switched off, which is
    /// worse than the bug it was added for.
    #[test]
    fn semantically_rich_artifacts_still_pass() {
        ok(
            r#"{
                "verdict": "approved",
                "overall_assessment": "Reviewed the pagination fix and the accompanying test.",
                "quality_scores": {"correctness": 9, "code_quality": 8, "test_coverage": 9, "spec_adherence": 8},
                "issues": [{"severity": "nit", "category": "style", "description": "naming", "file": "src/list.rs", "line": 2}],
                "strengths": ["test covers the empty case"]
            }"#,
            VERDICT,
        );
    }
}
