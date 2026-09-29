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

    use futures::StreamExt;
    let mut full_content = String::new();
    let mut usage: Option<TokenUsage> = None;
    let mut estimated_output_tokens: u32 = 0;
    let mut first_text_time: Option<Instant> = None;
    let mut mid_stream_retries: u32 = 0;
    // The provider's stop reason for the response currently being read.
    // Re-armed per attempt below; the value is only read once a stream has
    // been consumed.
    #[allow(unused_assignments)]
    let mut finish_reason: Option<String> = None;
    // The message of the last mid-stream failure, so an unchanged repeat is
    // recognised as the same problem rather than a new one.
    let mut last_mid_stream_error: Option<String> = None;
    // Reported in the stage metric, so it outlives any single attempt. TTFT is
    // measured per attempt, and restarts with the stream below.
    let mut retry_count: u32 = 0;
    // Re-armed per attempt below; read after the loop for the reported TTFT.
    let mut stream_start;

    'attempt: loop {
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
        let mut stream = stream.ok_or_else(|| {
            last_err.unwrap_or_else(|| anyhow!("LLM stream failed after retries"))
        })?;
        // A first token after a retry is not a first token for the original
        // request, so TTFT is measured from this attempt, not the first one.
        stream_start = Instant::now();

        // Re-armed per attempt: a stop reason describes one response, and a
        // retry produces another.
        finish_reason = None;

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
                Ok(StreamChunk::Finish { reason }) => {
                    // The provider says why it stopped. Nothing could read this
                    // before, so a response cut off at the token limit was
                    // indistinguishable from a malformed one and was reported
                    // as the latter.
                    finish_reason = Some(reason);
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
                Err(e)
                    if should_retry_mid_stream(
                        &e,
                        mid_stream_retries,
                        last_mid_stream_error.as_deref(),
                    ) =>
                {
                    // The connection dropped partway through. Restart the request
                    // rather than ending the run: everything the model produced so
                    // far is incomplete by definition, and re-asking is cheaper
                    // than throwing away a finished pipeline over a socket.
                    mid_stream_retries += 1;
                    retry_count += 1;
                    last_mid_stream_error = Some(e.to_string());
                    tracing::warn!(
                        target: "niki::agent",
                        role = ?role,
                        attempt = mid_stream_retries,
                        error = %e,
                        "stream dropped mid-response; re-establishing the request"
                    );
                    // Say so. A silent retry looks like a hang, and a user watching
                    // a three-minute run deserves to know it is still working.
                    display.agent_start(role);
                    full_content.clear();
                    usage = None;
                    estimated_output_tokens = 0;
                    first_text_time = None;
                    continue 'attempt;
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

        break 'attempt;
    }

    // NB: the usage total is assembled at the *end* of this function, not
    // here. It used to be frozen at this point — before the repair loop, which
    // issues a second real request and accumulates its usage into `usage`.
    // The repair was therefore billed to nobody: `token_usage` carried only
    // the first attempt, so a stage that needed two rounds to produce a valid
    // artifact under-reported its own cost to the user and to the spend cap.

    // A response the provider cut off is not a response.
    //
    // It used to arrive here as a half-written JSON object, go to the repairer,
    // and be reported as "did not satisfy the artifact requirements" — which
    // blames the model for something the token limit did, and whose real fix
    // (raise the limit) is nowhere in the message. The stop reason has been
    // available from the provider the whole time and was thrown away, because
    // `StreamChunk` had nowhere to put it.
    //
    // Caught on `qwen2.5-coder:3b`, which reports `done_reason: "length"`
    // correctly: a breadth run emitted a correct, well-formed artifact and
    // stopped mid-string, and the run failed with a message that named neither.
    if crate::runtime::tools::was_truncated(finish_reason.as_deref()) {
        let reason = finish_reason.as_deref().unwrap_or("length");
        display.agent_failed(
            role,
            &format!(
                "the response was cut off at the {reason} limit after {} characters, so the \
                 artifact is incomplete",
                full_content.len()
            ),
        );
        return Err(crate::NikiError::AgentFailure {
            agent: role,
            retries: retry_count,
            message: format!(
                "the response was truncated at the {reason} limit; the artifact was never \
                 finished. Raise the stage's max_tokens, or ask for a smaller change."
            ),
        }
        .into());
    }

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

/// The Coder prompt, rendered with the same context a real Coder stage gets.
///
/// Public so `niki doctor --measure` can probe the *actual* thing a model has
/// to do rather than a simplified version of it. The first version of that probe
/// used a two-line system prompt and scored qwen2.5-coder:3b at 0/4, while the
/// same model with the real prompt produces a valid edit — a strawman
/// measurement that would have recorded a false 0% and pushed every user of a
/// perfectly usable small model onto the slow path.
pub fn render_coder_probe_prompt() -> String {
    let (prompt_path, schema_path) = crate::orchestrator::pipeline::role_prompt(AgentRole::Coder);
    // `prompts/` is required: `role_prompt` returns a bare file name, and
    // `load_asset` needs a path to know which embedded directory to search.
    // Without it both loads failed, `unwrap_or_default` swallowed the failure
    // into an empty string, and the capability probe measured a model against
    // a blank system prompt and an empty schema — which is how a model that
    // completes full four-agent runs got scored 0/4. I had previously blamed
    // the probe for "asking a trivial question"; that was the wrong
    // diagnosis, and it was covering for an empty prompt.
    let template = crate::load_asset(&format!("prompts/{prompt_path}")).unwrap_or_default();
    let schema = crate::load_asset(schema_path).unwrap_or_default();
    //  borrows, so the prompt has to outlive the environment.
    let owned_template = template.clone();

    let current_files =
        "### File: src/lib.rs (action: Modify)\n```\npub fn old() -> u32 { 0 }\n```\n\n";
    let spec = serde_json::json!({
        "summary": "Rename `old` to `new`",
        "approach": "A single rename in src/lib.rs.",
        "files_to_modify": [{ "path": "src/lib.rs", "action": "modify" }],
        "acceptance_criteria": ["src/lib.rs defines `new`"],
        "constraints": [],
        "estimated_complexity": "low"
    })
    .to_string();

    let mut env = minijinja::Environment::new();
    env.add_template("probe", &owned_template).ok();
    let ctx = minijinja::context! {
        input_artifacts => vec![spec],
        revision_context => serde_json::Value::Null,
        revision_round => 0,
        project_knowledge => "",
        project_memory => "",
        current_files => current_files,
        mcp_tools => "",
        artifact_schema => schema,
        // The Coder runs as a tool loop in production, so the probe has to
        // render the prompt the Coder actually gets. Without this it renders
        // the one-shot variant and scores the model against a path that is no
        // longer the default.
        tool_loop => true,
    };
    env.get_template("probe")
        .and_then(|t| t.render(ctx))
        .unwrap_or_else(|_| String::new())
}

/// Whether a mid-stream failure earns another attempt.
///
/// Extracted so it can be *tested* rather than grepped for. The original guard
/// lived inline in the match arm, and the only thing that could check it was a
/// source-text assertion — which pins the shape of the code, not the decision.
/// A behavioural test is the only kind that survives a refactor.
///
/// Two things must be true at once: the error is a transport class worth
/// retrying at all, and it is not a repeat. A repeat of the *same* error means
/// the transport is down for this request, not that one unlucky read was
/// dropped — measured on a live run whose Tester stream kept failing, which
/// restarted the stage three times and then died with the identical error,
/// having spent three times the wall clock to learn nothing. A *different*
/// error is a different problem and still gets its retry.
pub fn should_retry_mid_stream(
    e: &anyhow::Error,
    retries_so_far: u32,
    last_error: Option<&str>,
) -> bool {
    is_mid_stream_retryable(e)
        && retries_so_far < MAX_MID_STREAM_RETRIES
        && last_error != Some(e.to_string().as_str())
}

/// Whether a mid-stream error is worth re-establishing the request for.
///
/// Scoped deliberately narrow. A decode failure, a truncated body, or a
/// dropped connection is the *transport* dying, and the model had no say in it.
/// A 429, a 500 or a timeout is already retried at connection time. Anything
/// that looks like the model itself — a refusal, a bad request, a context
/// overflow, a content filter — must NOT be retried here, because a second
/// identical request will fail identically and burn the user's tokens proving
/// it.
/// How many times a stream that dies *mid-response* is re-established.
///
/// Establishing the connection was already retried (`MAX_TRANSIENT_RETRIES`);
/// a connection that drops partway through a long response was not, and it is
/// the more common of the two against a local model. Measured: three live runs
/// against qwen2.5-coder:3b, the Coder succeeded in all three, and two of
/// them then died at the Tester with `Stream error: error decoding response
/// body` — discarding a finished run over a dropped connection.
///
/// Module-level so `should_retry_mid_stream` — which is public and tested —
/// sees the same bound the loop uses, rather than the loop keeping a private
/// copy no test could reach.
pub const MAX_MID_STREAM_RETRIES: u32 = 2;

pub fn is_mid_stream_retryable(e: &anyhow::Error) -> bool {
    let msg = e.to_string().to_ascii_lowercase();
    const RETRYABLE: [&str; 5] = [
        "error decoding response body",
        "connection reset",
        "connection closed",
        "incomplete message",
        "connection error",
    ];
    const FATAL: [&str; 5] = [
        "context length",
        "too long",
        "content filter",
        "invalid request",
        "refusal",
    ];
    if FATAL.iter().any(|f| msg.contains(f)) {
        return false;
    }
    RETRYABLE.iter().any(|r| msg.contains(r))
}
