use crate::agents::errors::{classify_failure, validate_detailed};
use crate::artifacts::types::AgentRole;
use crate::artifacts::validate::validate_artifact;
use crate::llm::provider::{CompletionRequest, LlmProvider, StreamChunk, TokenUsage};
use crate::llm::repair::repair_json;
use anyhow::{Result, anyhow};
use minijinja::Environment;
use std::time::{Duration, Instant};

pub mod coder;
pub mod errors;
pub mod planner;
pub mod reviewer;
pub mod tester;

/// Full-jitter exponential backoff delay.
fn jitter_delay(attempt: u32, base_ms: u64, max_ms: u64) -> u64 {
    let exp = 2u64.saturating_pow(attempt);
    let cap = base_ms.saturating_mul(exp).min(max_ms);
    // full jitter: random in [0, cap]

    fastrand::u64(0..=cap)
}

pub async fn run_agent(
    role: AgentRole,
    llm: &dyn LlmProvider,
    model: &str,
    template_name: &str,
    context: minijinja::Value,
    schema_path: &str,
    display: &mut crate::display::agent_stream::AgenticDisplay,
    degrade_on_invalid: bool,
    max_tokens: u32,
    temperature: f32,
    steer_rx: Option<&std::sync::Arc<std::sync::Mutex<Option<String>>>>,
) -> Result<(String, TokenUsage, u32, u32)> {
    let mut env = Environment::new();
    let template_content = crate::load_asset(&format!("prompts/{}", template_name))?;
    let schema_content = crate::load_asset(schema_path)?;

    let schema_json: serde_json::Value = serde_json::from_str(&schema_content)
        .map_err(|e| anyhow!("Failed to parse schema JSON: {}", e))?;

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

    let mut request = CompletionRequest {
        model: model.to_string(),
        system_prompt,
        user_message: "Please begin your task and produce the required JSON artifact.".to_string(),
        max_tokens,
        temperature,
        json_schema: None,
        tools: None,
    };

    display.agent_start(role);

    // ===== Phase 1: Retry transient API errors (429/503/timeout/network) =====
    const MAX_TRANSIENT_RETRIES: u32 = 3;
    let stream_start = Instant::now();
    let mut retry_count: u32 = 0;
    let mut last_err = None;
    let mut stream = None;
    for attempt in 0..=MAX_TRANSIENT_RETRIES {
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

                if is_transient && attempt < MAX_TRANSIENT_RETRIES {
                    retry_count += 1;
                    let delay = jitter_delay(attempt, 1000, 32000);
                    tracing::warn!(
                        target: "niki::agent",
                        role = ?role,
                        attempt = attempt + 1,
                        max = MAX_TRANSIENT_RETRIES + 1,
                        delay_ms = delay,
                        "LLM transient error, retrying"
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

    // ===== Stream and collect content =====
    use futures::StreamExt;
    let mut full_content = String::new();
    let mut usage: Option<TokenUsage> = None;
    let mut estimated_output_tokens: u32 = 0;
    let mut first_text_time: Option<Instant> = None;

    while let Some(chunk_res) = stream.next().await {
        match chunk_res {
            Ok(StreamChunk::Text(token)) => {
                if first_text_time.is_none() {
                    first_text_time = Some(Instant::now());
                }
                full_content.push_str(&token);
                estimated_output_tokens += (token.len() / 4).max(1) as u32;
                display.stream_token(&token);
            }
            Ok(StreamChunk::Usage(u)) => {
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

        // T12: Check for /steer corrections between chunks.
        if let Some(arc) = steer_rx {
            if let Ok(mut guard) = arc.lock() {
                if let Some(msg) = guard.take() {
                    tracing::info!(target: "niki::agent", role = ?role, "steer correction: {}", msg);
                    let _ = display.tui_tx().map(|tx| {
                        tx.send(crate::display::tui::DisplayEvent::ChatMessage {
                            role: "system".to_string(),
                            text: format!("[steer] {}", msg),
                        })
                    });
                }
            }
        }
    }

    let token_usage = usage.unwrap_or(TokenUsage {
        input_tokens: 0,
        output_tokens: estimated_output_tokens,
    });

    // ===== Phase 2: Resilient parsing + repair + re-prompt =====
    const MAX_REPAIR_RETRIES: u32 = 2;
    let mut json_content;
    let mut phase2_retries: u32 = 0;

    // First attempt: repair the raw output
    match repair_json(&full_content) {
        Ok(repaired) => {
            json_content = repaired;
        }
        Err(_) => {
            json_content = full_content.clone();
        }
    }

    // Validate and retry if needed
    let mut validation_errors: Option<Vec<String>> = None;
    let mut parse_error_detail: Option<String> = None;

    for repair_attempt in 0..=MAX_REPAIR_RETRIES {
        // Try to validate
        match validate_detailed(&json_content, &schema_json) {
            Ok(()) => {
                // Schema valid — also do strict validation
                if let Err(e) = validate_artifact(&json_content, schema_path) {
                    // Schema valid per field-level but strict validation failed
                    parse_error_detail = Some(e.to_string());
                } else {
                    // All validation passed — no need to clear state, we break
                    break;
                }
            }
            Err(fields) => {
                validation_errors = Some(fields);
            }
        }

        // Check if we should retry
        let failure = classify_failure(
            &full_content,
            None, // stop_reason not available in our streaming model
            parse_error_detail.as_deref(),
            validation_errors.clone(),
        );

        if !failure.is_retryable() {
            // Permanent failure — abort
            tracing::warn!(
                target: "niki::agent",
                role = ?role,
                failure = ?failure,
                "Permanent output failure, aborting"
            );
            break;
        }

        if repair_attempt >= MAX_REPAIR_RETRIES {
            // Budget exhausted — will degrade
            break;
        }

        // Phase 2a: Local repair (cheaper than LLM call)
        if parse_error_detail.is_some() || validation_errors.is_some() {
            phase2_retries += 1;
            retry_count += 1;

            // Try local repair first
            match repair_json(&json_content) {
                Ok(repaired) => {
                    json_content = repaired;
                    // Re-validate after local repair
                    if validate_detailed(&json_content, &schema_json).is_ok()
                        && validate_artifact(&json_content, schema_path).is_ok()
                    {
                        break; // Fixed by local repair
                    }
                }
                Err(_) => {} // Local repair failed, will re-prompt
            }

            // Phase 2b: Re-prompt with error feedback
            let error_feedback = if let Some(ref fields) = validation_errors {
                format!(
                    "Your previous response did not match the required schema. Violations: [{}]. \
                     Please fix these errors and respond with valid JSON only, no markdown fences.",
                    fields.join(", ")
                )
            } else if let Some(ref detail) = parse_error_detail {
                format!(
                    "Your previous response contained invalid JSON: {}. \
                     Please fix the JSON and respond with valid JSON only, no markdown fences.",
                    detail
                )
            } else {
                "Your previous response was invalid. Please respond with valid JSON only, no markdown fences.".to_string()
            };

            request.user_message = error_feedback;
            request.temperature = 0.0; // Lower temperature for repair pass

            // Re-request from LLM
            match llm.complete(request.clone()).await {
                Ok(response) => {
                    full_content = response.content.clone();
                    // Try repair on the new response
                    match repair_json(&full_content) {
                        Ok(repaired) => json_content = repaired,
                        Err(_) => json_content = full_content.clone(),
                    }
                    // Update usage
                    usage = Some(TokenUsage {
                        input_tokens: response
                            .usage
                            .input_tokens
                            .max(usage.map(|x| x.input_tokens).unwrap_or(0)),
                        output_tokens: response
                            .usage
                            .output_tokens
                            .max(usage.map(|x| x.output_tokens).unwrap_or(0)),
                    });
                }
                Err(e) => {
                    tracing::warn!(
                        target: "niki::agent",
                        role = ?role,
                        "Re-prompt failed: {}", e
                    );
                    break;
                }
            }
        }
    }

    tracing::debug!(
        target: "niki::agent",
        role = ?role,
        raw_len = full_content.len(),
        extracted_len = json_content.len(),
        retries = retry_count,
        phase2_retries = phase2_retries,
        "agent response captured"
    );

    // Final validation — if still invalid, surface the error (or degrade)
    if let Err(e) = validate_artifact(&json_content, schema_path) {
        let err_msg = e.to_string();
        if degrade_on_invalid {
            tracing::warn!(
                target: "niki::agent",
                role = ?role,
                error = %err_msg,
                "Validation failed — degrading to partial artifact"
            );
            display.agent_warning(role, &format!("Degraded: {}", err_msg));
            // Return repaired content without validation — caller handles degradation
        } else {
            display.agent_failed(role, &format!("Validation failed: {}", err_msg));
            return Err(crate::NikiError::ArtifactValidation {
                agent: role,
                errors: err_msg,
            }
            .into());
        }
    }

    let ttft_ms = first_text_time
        .map(|t| t.duration_since(stream_start).as_millis() as u32)
        .unwrap_or(0);

    Ok((json_content, token_usage, retry_count, ttft_ms))
}
