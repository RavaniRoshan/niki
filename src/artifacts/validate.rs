use anyhow::{anyhow, Result};
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
        return Err(anyhow!("Validation failed"));
    }

    Ok(())
}
