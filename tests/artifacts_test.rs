use niki::artifacts::types::{Complexity, TaskSpec};
use niki::artifacts::validate::validate_artifact;
use serde_json::json;

fn schema_path(name: &str) -> String {
    format!("{}/schemas/{}", env!("CARGO_MANIFEST_DIR"), name)
}

#[test]
fn test_task_spec_serialization_roundtrip() {
    let spec = TaskSpec {
        summary: "Add a health check endpoint".to_string(),
        approach: "Add a GET /health route returning status ok".to_string(),
        files_to_modify: vec![],
        acceptance_criteria: vec!["Returns 200 with status ok".to_string()],
        constraints: vec!["Do not change existing routes".to_string()],
        estimated_complexity: Complexity::Low,
    };

    let serialized = serde_json::to_string(&spec).expect("serialize");
    let deserialized: TaskSpec = serde_json::from_str(&serialized).expect("deserialize");

    assert_eq!(spec.summary, deserialized.summary);
    assert_eq!(spec.approach, deserialized.approach);
    assert_eq!(spec.acceptance_criteria, deserialized.acceptance_criteria);
    assert_eq!(spec.constraints, deserialized.constraints);
}

#[test]
fn test_artifact_schema_validation_accepts_valid() {
    let valid = json!({
        "summary": "Add health endpoint",
        "approach": "Add a route",
        "files_to_modify": [],
        "acceptance_criteria": ["Returns ok"],
        "constraints": [],
        "estimated_complexity": "low"
    });

    let result = validate_artifact(&valid.to_string(), &schema_path("task_spec.schema.json"));
    assert!(
        result.is_ok(),
        "expected valid TaskSpec to pass validation, got: {:?}",
        result.err()
    );
}

#[test]
fn test_artifact_schema_validation_rejects_invalid() {
    // Missing required fields: summary, acceptance_criteria, estimated_complexity.
    let invalid = json!({
        "approach": "Add a route",
        "files_to_modify": []
    });

    let result = validate_artifact(&invalid.to_string(), &schema_path("task_spec.schema.json"));
    assert!(
        result.is_err(),
        "expected invalid TaskSpec (missing required fields) to fail validation"
    );
}
