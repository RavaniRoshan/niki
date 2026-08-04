use niki::llm::repair::repair_json;
use niki::agents::errors::validate_detailed;
use serde_json::json;

/// Test that repair_json handles the most common LLM malformations.
/// These are real-world examples from free NVIDIA/NIM endpoints.
#[test]
fn test_nvidia_nim_malformed_json() {
    // Common: LLM wraps JSON in thinking/explanation text
    let input = r#"Let me analyze the task.

Based on the requirements, here is my plan:

```json
{
  "steps": [
    {"action": "read", "path": "src/main.rs"},
    {"action": "edit", "path": "src/main.rs", "search": "old", "replace": "new"}
  ],
  "summary": "Simple refactor"
}
```

This should fix the issue."#;

    let result = repair_json(input).unwrap();
    let v: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(v["steps"].is_array());
    assert_eq!(v["summary"], "Simple refactor");
}

#[test]
fn test_nvidia_nim_trailing_comma_in_array() {
    let input = r#"{"steps": [{"action": "read"}, {"action": "edit"},]}"#;
    let result = repair_json(input).unwrap();
    let v: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(v["steps"].as_array().unwrap().len(), 2);
}

#[test]
fn test_nvidia_nim_python_style_json() {
    let input = r#"{'steps': [{'action': 'read', 'path': 'main.rs'}], 'done': True}"#;
    let result = repair_json(input).unwrap();
    let v: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(v["done"], true);
    assert_eq!(v["steps"][0]["action"], "read");
}

#[test]
fn test_validate_detailed_array_type() {
    let schema = json!({
        "type": "object",
        "properties": {
            "steps": {"type": "array"}
        }
    });
    let result = validate_detailed(r#"{"steps": "not an array"}"#, &schema);
    assert!(result.is_err());
    let errs = result.unwrap_err();
    assert!(errs.iter().any(|e| e.contains("array")));
}

#[test]
fn test_validate_detailed_nested_object() {
    let schema = json!({
        "type": "object",
        "properties": {
            "config": {"type": "object"}
        }
    });
    let result = validate_detailed(r#"{"config": "not an object"}"#, &schema);
    assert!(result.is_err());
    let errs = result.unwrap_err();
    assert!(errs.iter().any(|e| e.contains("object")));
}
