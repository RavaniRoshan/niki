use anyhow::{anyhow, Result};
use minijinja::{Environment, context};
use crate::llm::provider::{LlmProvider, CompletionRequest};
use crate::artifacts::types::AgentRole;
use crate::artifacts::validate::validate_artifact;
use std::fs;

pub mod planner;
pub mod coder;
pub mod tester;
pub mod reviewer;

pub async fn run_agent(
    role: AgentRole,
    llm: &dyn LlmProvider,
    model: &str,
    template_name: &str,
    context: minijinja::Value,
    schema_path: &str,
    display: &mut crate::display::agent_stream::AgenticDisplay,
) -> Result<(String, u32)> {
    let mut env = Environment::new();
    let template_path = crate::resolve_asset(&format!("prompts/{}", template_name));
    let template_content = fs::read_to_string(&template_path)
        .map_err(|e| anyhow!("Failed to read prompt template {}: {}", template_path.display(), e))?;

    let schema_path_resolved = crate::resolve_asset(schema_path);
    let schema_content = fs::read_to_string(&schema_path_resolved)
        .map_err(|e| anyhow!("Failed to read schema {}: {}", schema_path_resolved.display(), e))?;

    env.add_template(template_name, &template_content)?;
    let tmpl = env.get_template(template_name)?;

    let mut ctx: serde_json::Value = serde_json::to_value(context)?;
    if let Some(obj) = ctx.as_object_mut() {
        obj.insert("artifact_schema".to_string(), serde_json::Value::String(schema_content));
    }
    let system_prompt = tmpl.render(ctx)?;

    let request = CompletionRequest {
        model: model.to_string(),
        system_prompt,
        user_message: "Please begin your task and produce the required JSON artifact.".to_string(),
        max_tokens: 8192,
        temperature: 0.2,
    };

    display.agent_start(role);

    use futures::StreamExt;
    let mut stream = llm.stream(request).await?;
    let mut full_content = String::new();
    let mut token_count = 0; // naive estimation for prototype
    
    while let Some(chunk_res) = stream.next().await {
        match chunk_res {
            Ok(token) => {
                full_content.push_str(&token);
                // Rough token estimation: 1 token per ~4 chars
                token_count += (token.len() / 4).max(1) as u32;
                display.stream_token(&token);
            }
            Err(e) => {
                display.agent_failed(role, &e.to_string());
                return Err(e);
            }
        }
    }

    let json_content = extract_json(&full_content);

    tracing::debug!(target: "niki::agent", role = ?role, raw_len = full_content.len(), raw = %full_content, extracted = %json_content, "agent response captured");

    if let Err(e) = validate_artifact(&json_content, schema_path_resolved.to_str().unwrap_or(schema_path)) {
        let err_msg = e.to_string();
        display.agent_failed(role, &format!("Validation failed: {}", err_msg));
        return Err(crate::NikiError::ArtifactValidation {
            agent: role,
            errors: err_msg,
        }.into());
    }

    Ok((json_content, token_count))
}

fn extract_json(text: &str) -> String {
    if let Some(start) = text.find("```json") {
        if let Some(end) = text[start + 7..].find("```") {
            return text[start + 7..start + 7 + end].trim().to_string();
        }
    }
    if let Some(start) = text.find("{") {
        if let Some(end) = text.rfind("}") {
            return text[start..=end].trim().to_string();
        }
    }
    text.trim().to_string()
}
