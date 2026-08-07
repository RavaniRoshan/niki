use niki::agents::errors::{OutputFailure, classify_failure, validate_detailed};
use niki::llm::repair::repair_json;
use serde_json::json;

#[test]
fn test_repair_json_code_fences() {
    let input = r#"```json
{"name": "test", "value": 42}
```"#;
    let result = repair_json(input).unwrap();
    let v: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(v["name"], "test");
    assert_eq!(v["value"], 42);
}

#[test]
fn test_repair_json_trailing_commas() {
    let input = r#"{"name": "test", "value": 42,}"#;
    let result = repair_json(input).unwrap();
    let v: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(v["name"], "test");
}

#[test]
fn test_repair_json_single_quotes() {
    let input = r#"{'name': 'test'}"#;
    let result = repair_json(input).unwrap();
    let v: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(v["name"], "test");
}

#[test]
fn test_repair_json_python_literals() {
    let input = r#"{"active": True, "deleted": False, "name": None}"#;
    let result = repair_json(input).unwrap();
    let v: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(v["active"], true);
    assert_eq!(v["deleted"], false);
    assert_eq!(v["name"], serde_json::Value::Null);
}

#[test]
fn test_repair_json_unclosed_brackets() {
    let input = r#"{"name": "test", "items": [1, 2"#;
    let result = repair_json(input).unwrap();
    let v: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(v["name"], "test");
}

#[test]
fn test_repair_json_from_prose() {
    let input = r#"Here is the output:
{"name": "test", "value": 42}
End of output."#;
    let result = repair_json(input).unwrap();
    let v: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(v["name"], "test");
}

#[test]
fn test_repair_json_combined() {
    let input = r#"```json
{
  'name': 'test',
  'active': True,
}
```"#;
    let result = repair_json(input).unwrap();
    let v: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(v["name"], "test");
    assert_eq!(v["active"], true);
}

#[test]
fn test_repair_json_unrepairable() {
    let input = "This is not JSON at all, just plain text";
    let result = repair_json(input);
    assert!(result.is_err());
}

#[test]
fn test_classify_failure_empty() {
    let f = classify_failure("", None, None, None);
    assert_eq!(f, OutputFailure::EmptyResponse);
    assert!(!f.is_retryable());
}

#[test]
fn test_classify_failure_refusal() {
    let f = classify_failure("I cannot help", Some("refusal"), None, None);
    assert_eq!(f, OutputFailure::Refusal);
    assert!(!f.is_retryable());
}

#[test]
fn test_classify_failure_truncated() {
    let f = classify_failure("{", Some("max_tokens"), None, None);
    assert_eq!(f, OutputFailure::Truncated);
    assert!(f.is_retryable());
}

#[test]
fn test_classify_failure_no_json() {
    let f = classify_failure("done", None, None, None);
    assert_eq!(f, OutputFailure::NoJson);
    assert!(f.is_retryable());
}

#[test]
fn test_classify_failure_parse_error() {
    let f = classify_failure("{bad json}", None, Some("expected `:`"), None);
    assert_eq!(
        f,
        OutputFailure::ParseError {
            detail: "expected `:`".to_string()
        }
    );
    assert!(f.is_retryable());
}

#[test]
fn test_classify_failure_validation_error() {
    let f = classify_failure(
        r#"{"name": "test"}"#,
        None,
        None,
        Some(vec!["missing field 'count'".to_string()]),
    );
    assert_eq!(
        f,
        OutputFailure::ValidationError {
            fields: vec!["missing field 'count'".to_string()]
        }
    );
    assert!(f.is_retryable());
}

#[test]
fn test_validate_detailed_valid() {
    let schema = json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"}
        },
        "required": ["name"]
    });
    let result = validate_detailed(r#"{"name": "test"}"#, &schema);
    assert!(result.is_ok());
}

#[test]
fn test_validate_detailed_missing_required() {
    let schema = json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"},
            "count": {"type": "number"}
        },
        "required": ["name", "count"]
    });
    let result = validate_detailed(r#"{"name": "test"}"#, &schema);
    assert!(result.is_err());
    let errs = result.unwrap_err();
    assert!(errs.iter().any(|e| e.contains("count")));
}

#[test]
fn test_validate_detailed_type_mismatch() {
    let schema = json!({
        "type": "object",
        "properties": {
            "count": {"type": "number"}
        }
    });
    let result = validate_detailed(r#"{"count": "not a number"}"#, &schema);
    assert!(result.is_err());
    let errs = result.unwrap_err();
    assert!(errs.iter().any(|e| e.contains("type")));
}

#[test]
fn test_validate_detailed_invalid_json() {
    let schema = json!({"type": "object"});
    let result = validate_detailed("{bad json}", &schema);
    assert!(result.is_err());
}
