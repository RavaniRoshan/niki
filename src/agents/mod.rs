use crate::agents::errors::{classify_failure, validate_detailed};
use crate::artifacts::types::AgentRole;
use crate::artifacts::validate::validate_artifact;
use crate::llm::provider::{CompletionRequest, LlmProvider, StreamChunk, TokenUsage};
use crate::llm::repair::repair_json;
use anyhow::{Result, anyhow};
use minijinja::Environment;
use std::time::{Duration, Instant};

pub mod errors;
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
    // Render the stage-specific overlay first, then wrap it in the shared
    // base persona so every stage reads as one continuous assistant rather
    // than four disconnected prompts. Falls back to the stage prompt alone if
    // `prompts/base.md` is missing or fails to render (keeps eval behavior
    // intact when the base layer is absent).
    let stage_prompt = tmpl.render(ctx.clone())?;
    let system_prompt = match crate::load_asset("prompts/base.md") {
        Ok(base_content) => {
            let _ = env.add_template("__base", &base_content);
            match env.get_template("__base").and_then(|t| {
                let mut c = ctx.clone();
                if let Some(o) = c.as_object_mut() {
                    o.insert(
                        "role_additional".to_string(),
                        serde_json::Value::String(stage_prompt.clone()),
                    );
                }
                t.render(c)
            }) {
                Ok(s) => s,
                Err(_) => stage_prompt,
            }
        }
        Err(_) => stage_prompt,
    };

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
                // `.max()` is correct *within a single stream*: every usage
                // chunk here describes the same request. Anthropic emits two
                // disjoint chunks for one call (message_start carries
                // input_tokens, message_delta carries output_tokens), and
                // OpenAI-style providers emit a final cumulative snapshot, so
                // taking the per-field max avoids double counting. Summing
                // across separate requests is what would be wrong — see the
                // repair-retry path below.
                let input_tokens = u
                    .input_tokens
                    .max(usage.map(|x| x.input_tokens).unwrap_or(0));
                let output_tokens = u
                    .output_tokens
                    .max(usage.map(|x| x.output_tokens).unwrap_or(0));
                let cached_input_tokens = u
                    .cached_input_tokens
                    .max(usage.map(|x| x.cached_input_tokens).unwrap_or(0));
                let reasoning_tokens = u
                    .reasoning_tokens
                    .max(usage.map(|x| x.reasoning_tokens).unwrap_or(0));
                usage = Some(TokenUsage {
                    input_tokens,
                    output_tokens,
                    cached_input_tokens,
                    reasoning_tokens,
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

    // NB: the usage total is assembled at the *end* of this function, not
    // here. It used to be frozen at this point — before the repair loop, which
    // issues a second real request and accumulates its usage into `usage`.
    // The repair was therefore billed to nobody: `token_usage` carried only
    // the first attempt, so a stage that needed two rounds to produce a valid
    // artifact under-reported its own cost to the user and to the spend cap.

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
    // Set when the JSON is well-formed and field-valid but fails the stricter
    // artifact checks (an empty diff, a contentless verdict). It is NOT a JSON
    // parse error, and conflating the two makes the re-prompt tell the model
    // its syntax is broken when its *content* is what is wrong.
    let mut strict_error_detail: Option<String> = None;

    for repair_attempt in 0..=MAX_REPAIR_RETRIES {
        // Try to validate
        match validate_detailed(&json_content, &schema_json) {
            Ok(()) => {
                // Schema valid — also do strict validation
                if let Err(e) = validate_artifact(&json_content, schema_path) {
                    // Field-level valid, strict validation failed.
                    strict_error_detail = Some(e.to_string());
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
            strict_error_detail.as_deref(),
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
            // Budget exhausted — will fail-loud in final validation
            break;
        }

        // Phase 2a: Local repair (cheaper than LLM call)
        if strict_error_detail.is_some() || validation_errors.is_some() {
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
            } else if let Some(ref detail) = strict_error_detail {
                // The JSON parsed and matched the schema field-by-field; what
                // failed is the content. Saying "invalid JSON" here told the
                // model its syntax was broken, and a small model duly replied
                // with the same empty artifact — a retry loop that could not
                // possibly succeed.
                format!(
                    "Your previous response was valid JSON but did not satisfy the artifact \
                     requirements: {detail}. Fix the *content*, not the syntax. An artifact with \
                     no edits, no files changed, or empty notes is not acceptable — produce the \
                     actual change as JSON, with no markdown fences."
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
                    // The repair retry is a *second* request, so its usage
                    // adds to the first attempt's rather than being maxed
                    // against it. `.max()` here under-reported a repair by
                    // reporting only the more expensive of the two attempts.
                    let mut prev = usage.unwrap_or(TokenUsage {
                        input_tokens: 0,
                        output_tokens: estimated_output_tokens,
                        ..Default::default()
                    });
                    prev.accumulate(&response.usage);
                    usage = Some(prev);
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

    // Final validation — fail-loud: invalid artifacts never degrade silently.
    if let Err(e) = validate_artifact(&json_content, schema_path) {
        let err_msg = e.to_string();
        // A user who hits this needs to know the two things that actually fix
        // it. Without this the message is a schema dump and the run just stops:
        // in practice the cause is almost always a model too small to emit a
        // conformant artifact, which is invisible unless it is named.
        let hint = "The response was valid JSON but did not satisfy the artifact requirements.\n\
             Most often the model is too small to emit a conformant artifact — \
             `qwen2.5-coder:3b` fails here on ordinary tasks.\n\
             Try: a larger model (7b+), or run ./scripts/dogfood.sh to see where your \
             model stops.";
        display.agent_failed(role, &format!("Validation failed: {}", err_msg));
        return Err(crate::NikiError::ArtifactValidation {
            agent: role,
            errors: err_msg,
            hint: hint.to_string(),
        }
        .into());
    }

    let ttft_ms = first_text_time
        .map(|t| t.duration_since(stream_start).as_millis() as u32)
        .unwrap_or(0);

    let token_usage = usage.unwrap_or(TokenUsage {
        input_tokens: 0,
        output_tokens: estimated_output_tokens,
        ..Default::default()
    });

    Ok((json_content, token_usage, retry_count, ttft_ms))
}
