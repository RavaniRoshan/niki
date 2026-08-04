use serde_json::Value;

/// Classification of LLM output failures.
/// Used to determine whether to retry, repair, or abort.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputFailure {
    /// LLM returned empty or whitespace-only content.
    EmptyResponse,
    /// LLM refused to generate output (stop_reason = "refusal").
    Refusal,
    /// No valid JSON object found in the output at all.
    NoJson,
    /// Output was truncated (stop_reason = "max_tokens" / finish_reason = "length").
    Truncated,
    /// JSON is valid but fails schema validation.
    ValidationError { fields: Vec<String> },
    /// JSON is malformed (syntax error).
    ParseError { detail: String },
    /// Rule / business logic violation (configurable per-agent).
    RuleError { detail: String },
    /// Runtime error in the LLM response (non-retryable).
    RunError { detail: String },
}

impl OutputFailure {
    /// Whether this failure should trigger a retry (repair + re-prompt).
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            OutputFailure::NoJson
                | OutputFailure::Truncated
                | OutputFailure::ValidationError { .. }
                | OutputFailure::ParseError { .. }
        )
    }

    /// Human-readable description for repair prompts.
    pub fn description(&self) -> String {
        match self {
            OutputFailure::EmptyResponse => "The response was empty.".to_string(),
            OutputFailure::Refusal => "The model refused to generate output.".to_string(),
            OutputFailure::NoJson => "No JSON object was found in the response.".to_string(),
            OutputFailure::Truncated => {
                "The response was truncated (hit max_tokens). The JSON is incomplete.".to_string()
            }
            OutputFailure::ValidationError { fields } => {
                format!(
                    "JSON does not match the schema. Violations: [{}]",
                    fields.join(", ")
                )
            }
            OutputFailure::ParseError { detail } => {
                format!("JSON is malformed: {}", detail)
            }
            OutputFailure::RuleError { detail } => {
                format!("Rule violation: {}", detail)
            }
            OutputFailure::RunError { detail } => {
                format!("Runtime error: {}", detail)
            }
        }
    }
}

/// Classify a completion response by reading stop_reason / finish_reason BEFORE parsing.
pub fn classify_failure(
    content: &str,
    stop_reason: Option<&str>,
    parse_error: Option<&str>,
    validation_errors: Option<Vec<String>>,
) -> OutputFailure {
    // Check stop_reason first (authoritative)
    if let Some(reason) = stop_reason {
        let reason_lower = reason.to_lowercase();
        if reason_lower.contains("refusal") {
            return OutputFailure::Refusal;
        }
        if reason_lower.contains("max_tokens") || reason_lower.contains("length") {
            return OutputFailure::Truncated;
        }
    }

    // Check content
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return OutputFailure::EmptyResponse;
    }

    // Check for validation errors (provided by caller)
    if let Some(fields) = validation_errors {
        if !fields.is_empty() {
            return OutputFailure::ValidationError { fields };
        }
    }

    // Check for parse errors (provided by caller)
    if let Some(detail) = parse_error {
        return OutputFailure::ParseError {
            detail: detail.to_string(),
        };
    }

    // Check if content contains JSON at all
    if !trimmed.contains('{') && !trimmed.contains('[') {
        return OutputFailure::NoJson;
    }

    // Default: assume parse error if we got here with content
    OutputFailure::ParseError {
        detail: "Unknown parse failure".to_string(),
    }
}

/// Validate JSON against a schema and return field-level errors.
pub fn validate_detailed(json_str: &str, schema: &Value) -> Result<(), Vec<String>> {
    let artifact: Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(e) => return Err(vec![format!("JSON parse error: {}", e)]),
    };

    let mut errors = Vec::new();

    // Check required fields
    if let Some(required) = schema.get("required").and_then(|v| v.as_array()) {
        if let Some(props) = schema.get("properties").and_then(|v| v.as_object()) {
            for field in required {
                if let Some(field_name) = field.as_str() {
                    if !props.contains_key(field_name) {
                        continue;
                    }
                    // Check if field is present in artifact
                    if !artifact.get(field_name).is_some() {
                        errors.push(format!("missing required field '{}'", field_name));
                    }
                }
            }
        }
    }

    // Check type mismatches for properties
    if let Some(properties) = schema.get("properties").and_then(|v| v.as_object()) {
        if let Some(obj) = artifact.as_object() {
            for (key, prop_schema) in properties {
                if let Some(actual) = obj.get(key) {
                    if let Some(expected_type) = prop_schema.get("type").and_then(|v| v.as_str()) {
                        let actual_type = match actual {
                            Value::String(_) => "string",
                            Value::Number(_) => "number",
                            Value::Bool(_) => "boolean",
                            Value::Null => "null",
                            Value::Array(_) => "array",
                            Value::Object(_) => "object",
                        };
                        if actual_type != expected_type {
                            errors.push(format!(
                                "field '{}' expected type '{}' but got '{}'",
                                key, expected_type, actual_type
                            ));
                        }
                    }
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_classify_empty() {
        let f = classify_failure("", None, None, None);
        assert_eq!(f, OutputFailure::EmptyResponse);
    }

    #[test]
    fn test_classify_refusal() {
        let f = classify_failure("I cannot help", Some("refusal"), None, None);
        assert_eq!(f, OutputFailure::Refusal);
        assert!(!f.is_retryable());
    }

    #[test]
    fn test_classify_truncated() {
        let f = classify_failure("{", Some("max_tokens"), None, None);
        assert_eq!(f, OutputFailure::Truncated);
        assert!(f.is_retryable());
    }

    #[test]
    fn test_classify_no_json() {
        let f = classify_failure("Here is the output: done", None, None, None);
        assert_eq!(f, OutputFailure::NoJson);
        assert!(f.is_retryable());
    }

    #[test]
    fn test_classify_parse_error() {
        let f = classify_failure(
            "```json\n{bad json}\n```",
            None,
            Some("expected `:` at line 1"),
            None,
        );
        assert_eq!(
            f,
            OutputFailure::ParseError {
                detail: "expected `:` at line 1".to_string()
            }
        );
        assert!(f.is_retryable());
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
}
