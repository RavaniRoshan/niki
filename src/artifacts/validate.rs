use anyhow::{Result, anyhow};
use serde_json::Value;
use std::fs;

pub fn validate_artifact(json_str: &str, schema_path: &str) -> Result<()> {
    let schema_content = fs::read_to_string(schema_path)
        .map_err(|e| anyhow!("Failed to read schema {}: {}", schema_path, e))?;
    let schema_json: Value = serde_json::from_str(&schema_content)
        .map_err(|e| anyhow!("Failed to parse schema JSON: {}", e))?;
    let artifact_json: Value = serde_json::from_str(json_str)
        .map_err(|e| anyhow!("Failed to parse artifact JSON: {}", e))?;

    let is_valid = jsonschema::is_valid(&schema_json, &artifact_json);
    if !is_valid {
        // Provide context about what failed: list top-level keys present vs expected.
        let (artifact_keys, schema_required) = extract_field_info(&artifact_json, &schema_json);
        return Err(anyhow!(
            "Artifact does not match schema. Fields present: [{}]. Schema requires: [{}].",
            artifact_keys,
            schema_required
        ));
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
