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
    // Passed to the provider as-is. `None` sends no such field at all, which
    // is the safe default: a provider that does not know the key may reject
    // the whole request, and a value guessed from a model name turns a
    // working run into a 400.
    reasoning_effort: Option<&str>,
    steer_rx: Option<&std::sync::Arc<std::sync::Mutex<Option<String>>>>,
    // Filled in with what was actually spent, **including on the error path**.
    //
    // A stage that fails has usually already paid for the request that failed
    // it. The caller needs that number to record a cost, and a `Result` that
    // carries only the error cannot give it — which is why every failed stage
    // used to be recorded as free, in `task.json`, in the cost page, and in
    // the JSON envelope. The failure is the expensive case and it was the one
    // that showed nothing.
    spent_out: &mut Option<TokenUsage>,
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
        reasoning_effort: reasoning_effort.map(str::to_string),
        // An agent stage is single-shot by design. History here would let a
        // Planner see what the Coder later did, which is the independence the
        // whole pipeline rests on. Only the chat surface carries history.
        history: Vec::new(),
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
    // Set only when a re-prompt could not be delivered. Distinguishes "the
    // model produced something invalid" from "we never got to ask again".
    let mut transport_failure: Option<String> = None;
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
                    // By type first: a total-deadline expiry prints as
                    // "request or response body error" and carries "timeout"
                    // only in its source chain, so the substring test below
                    // cannot see it — and the stage died unretried.
                    let is_transient = crate::llm::provider::is_timeout_error(&e);
                    let err_str = e.to_string().to_lowercase();
                    let is_transient = is_transient
                        || err_str.contains("timeout")
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
                    // Remembered, because the alternative is telling a user
                    // their model is too small when the model never got the
                    // chance to try again. The `break` below falls through to
                    // the same final validation as a genuine schema failure,
                    // and that path names model capability as the cause — so a
                    // 429, a dropped connection or an expired key was reported
                    // as "use a bigger model", with a remedy that cannot help.
                    transport_failure = Some(e.to_string());
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

    // Work out what this stage cost *before* deciding whether it succeeded.
    // Everything after this point can return early, and everything after this
    // point is a path the user was still billed for.
    let token_usage = usage.unwrap_or(TokenUsage {
        input_tokens: 0,
        output_tokens: estimated_output_tokens,
        ..Default::default()
    });
    *spent_out = Some(token_usage);

    // Final validation — fail-loud: invalid artifacts never degrade silently.
    if let Err(e) = validate_artifact(&json_content, schema_path) {
        let err_msg = e.to_string();
        // A user who hits this needs to know the two things that actually fix
        // it. Without this the message is a schema dump and the run just stops:
        // in practice the cause is almost always a model too small to emit a
        // conformant artifact, which is invisible unless it is named.
        // The diagnosis has to match the evidence.
        //
        // When the re-prompt could not be delivered, the artifact we are
        // holding is the *first* attempt — the one that was already invalid.
        // Reporting that as "the model is too small" is often true and
        // always beside the point: the thing that actually happened is a 429,
        // a dropped connection, or a key that expired, and the remedy it
        // suggests cannot fix any of them.
        //
        // The other half of the old hint pointed at `./scripts/dogfood.sh`,
        // which exists in this repository and in nobody's install. A remedy a
        // user cannot run is not a remedy.
        let hint = artifact_validation_hint(transport_failure.as_deref());
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

    Ok((json_content, token_usage, retry_count, ttft_ms))
}

/// What to tell the user when a stage's artifact does not validate.
///
/// Split out so the *choice between the two diagnoses* is testable, because
/// that choice is the defect. One code path produced both messages: a
/// re-prompt that could not be delivered (`break` on a transport error) fell
/// through to the same final validation as a genuinely malformed response, and
/// the user was told their model was too small. Often true — the first
/// response *was* invalid — and always beside the point, because what actually
/// happened was a 429, a dropped connection or an expired key, and the remedy
/// it suggested cannot fix any of them.
///
/// The other half of that hint pointed at `./scripts/dogfood.sh`, which lives
/// in this repository and in nobody's install. A remedy a user cannot run is
/// not a remedy.
pub fn artifact_validation_hint(transport_failure: Option<&str>) -> String {
    match transport_failure {
        Some(cause) => format!(
            "The first response did not satisfy the artifact requirements, and the correction \
             could not be delivered, so the model was never asked again.\n\
             What went wrong: {cause}\n\
             That is a connection or credentials problem, not a model-capability one, so \
             changing models will not help.\n\
             Try: `niki doctor` to check the provider is reachable and the key works."
        ),
        None => "The response was valid JSON but did not satisfy the artifact requirements.\n\
             Most often the model is too small to emit a conformant artifact — \
             `qwen2.5-coder:3b` fails here on ordinary tasks.\n\
             Try: a larger model (7b+), then `niki doctor` to confirm the provider is \
             healthy."
            .to_string(),
    }
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
    // A stall mid-answer is the same event as a stall before one: the upstream
    // stopped sending. `is_timeout_error` sees it by type where the string
    // list below sees only "request or response body error".
    if super::llm::provider::is_timeout_error(e) {
        return true;
    }
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

#[cfg(test)]
mod tests {
    use super::artifact_validation_hint;

    /// A run that failed because the network failed must not be told the model
    /// was too small.
    ///
    /// This is the false-positive shape the whole harness is built to catch,
    /// and it was sitting in the product: one code path, two unrelated causes,
    /// and a hint that named model capability for both. A user whose key had
    /// expired mid-run was told to buy a bigger model.
    #[test]
    fn a_transport_failure_is_not_reported_as_a_model_problem() {
        let hint = artifact_validation_hint(Some("429 Too Many Requests"));
        assert!(
            hint.contains("429 Too Many Requests"),
            "the actual cause must be quoted back: {hint}"
        );
        assert!(
            !hint.contains("too small to emit"),
            "a request that was never delivered says nothing about the model's \
             capability, and this hint claims it does: {hint}"
        );
        assert!(
            hint.contains("niki doctor"),
            "and it must offer a next step that can actually diagnose it: {hint}"
        );
    }

    /// The other direction. When the model really did answer and the artifact
    /// really was wrong, model capability *is* the likeliest cause and saying
    /// so is the useful thing. A fix that only ever softened the message would
    /// lose that.
    #[test]
    fn a_real_schema_failure_still_blames_the_model() {
        let hint = artifact_validation_hint(None);
        assert!(
            hint.contains("too small to emit"),
            "a conformant-looking answer that fails the schema is still a model \
             problem: {hint}"
        );
    }

    /// A remedy a user cannot run is not a remedy. `scripts/dogfood.sh` is a
    /// repository script, referenced from inside a Homebrew, Scoop and winget
    /// install — three of which do not contain it.
    #[test]
    fn no_hint_points_at_a_script_the_user_does_not_have() {
        for hint in [
            artifact_validation_hint(None),
            artifact_validation_hint(Some("connection refused")),
        ] {
            assert!(
                !hint.contains("dogfood.sh") && !hint.contains("./scripts/"),
                "the hint points at a file that is not shipped: {hint}"
            );
        }
    }
}
