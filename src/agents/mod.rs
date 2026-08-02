use crate::artifacts::types::AgentRole;
use crate::artifacts::validate::validate_artifact;
use crate::llm::provider::{CompletionRequest, LlmProvider, StreamChunk, TokenUsage};
use anyhow::{Result, anyhow};
use minijinja::Environment;
use std::fs;
use std::time::Duration;

pub mod coder;
pub mod planner;
pub mod reviewer;
pub mod tester;

pub async fn run_agent(
    role: AgentRole,
    llm: &dyn LlmProvider,
    model: &str,
    template_name: &str,
    context: minijinja::Value,
    schema_path: &str,
    display: &mut crate::display::agent_stream::AgenticDisplay,
) -> Result<(String, TokenUsage)> {
    let mut env = Environment::new();
    let template_path = crate::resolve_asset(&format!("prompts/{}", template_name));
    let template_content = fs::read_to_string(&template_path).map_err(|e| {
        anyhow!(
            "Failed to read prompt template {}: {}",
            template_path.display(),
            e
        )
    })?;

    let schema_path_resolved = crate::resolve_asset(schema_path);
    let schema_content = fs::read_to_string(&schema_path_resolved).map_err(|e| {
        anyhow!(
            "Failed to read schema {}: {}",
            schema_path_resolved.display(),
            e
        )
    })?;

    env.add_template(template_name, &template_content)?;
    let tmpl = env.get_template(template_name)?;

    let mut ctx: serde_json::Value = serde_json::to_value(context)?;
    if let Some(obj) = ctx.as_object_mut() {
        obj.insert(
            "artifact_schema".to_string(),
            serde_json::Value::String(schema_content),
        );
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

    // Retry loop with exponential backoff for transient LLM errors.
    const MAX_RETRIES: u32 = 3;
    const BASE_DELAY_MS: u64 = 1000;
    let mut last_err = None;

    let mut stream = None;
    for attempt in 0..=MAX_RETRIES {
        match llm.stream(request.clone()).await {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                let is_transient = err_str.contains("timeout")
                    || err_str.contains("rate")
                    || err_str.contains("429")
                    || err_str.contains("503")
                    || err_str.contains("overloaded")
                    || err_str.contains("connection")
                    || err_str.contains("network");

                if is_transient && attempt < MAX_RETRIES {
                    let delay = BASE_DELAY_MS * 2u64.pow(attempt);
                    eprintln!(
                        "LLM transient error (attempt {}/{}), retrying in {}ms: {}",
                        attempt + 1,
                        MAX_RETRIES + 1,
                        delay,
                        e
                    );
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                    last_err = Some(e);
                    continue;
                }
                display.agent_failed(role, &e.to_string());
                return Err(e);
            }
        }
    }
    let mut stream = stream
        .ok_or_else(|| last_err.unwrap_or_else(|| anyhow!("LLM stream failed after retries")))?;

    use futures::StreamExt;
    let mut full_content = String::new();
    // Real usage is reported by the provider in a trailing Usage chunk. If the
    // stream ends without one (e.g. a gateway that omits usage), we fall back
    // to a char-based estimate so a token count is always present.
    let mut usage: Option<TokenUsage> = None;
    let mut estimated_output_tokens: u32 = 0;

    while let Some(chunk_res) = stream.next().await {
        match chunk_res {
            Ok(StreamChunk::Text(token)) => {
                full_content.push_str(&token);
                estimated_output_tokens += (token.len() / 4).max(1) as u32;
                display.stream_token(&token);
            }
            Ok(StreamChunk::Usage(u)) => {
                // Merge: anthropic/ollama may send input and output in separate
                // chunks, so take the larger of what we've seen per field.
                let input_tokens = u
                    .input_tokens
                    .max(usage.map(|x| x.input_tokens).unwrap_or(0));
                let output_tokens = u
                    .output_tokens
                    .max(usage.map(|x| x.output_tokens).unwrap_or(0));
                usage = Some(TokenUsage {
                    input_tokens,
                    output_tokens,
                });
            }
            Err(e) => {
                display.agent_failed(role, &e.to_string());
                return Err(e);
            }
        }
    }

    // Fall back to the estimate only when the API reported no usage at all.
    let token_usage = usage.unwrap_or_else(|| TokenUsage {
        input_tokens: 0,
        output_tokens: estimated_output_tokens,
    });

    let json_content = extract_json(&full_content);

    tracing::debug!(target: "niki::agent", role = ?role, raw_len = full_content.len(), raw = %full_content, extracted = %json_content, "agent response captured");

    if let Err(e) = validate_artifact(
        &json_content,
        schema_path_resolved.to_str().unwrap_or(schema_path),
    ) {
        let err_msg = e.to_string();
        display.agent_failed(role, &format!("Validation failed: {}", err_msg));
        return Err(crate::NikiError::ArtifactValidation {
            agent: role,
            errors: err_msg,
        }
        .into());
    }

    Ok((json_content, token_usage))
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
