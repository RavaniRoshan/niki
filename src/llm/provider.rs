use crate::config::ProviderConfig;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

/// A single item emitted by a streaming completion.
///
/// Streams yield text deltas as they arrive; the provider also emits one
/// `Usage` chunk at the end carrying the real token counts reported by the
/// upstream API. Consumers accumulate text and take the last `Usage` they see.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    Text(String),
    Usage(TokenUsage),
    /// The provider's stop reason, once it knows one.
    ///
    /// This is the only way a streaming caller can tell a *complete* response
    /// from one cut off at the token limit, and without it the two are
    /// indistinguishable: a half-written artifact looks exactly like a
    /// malformed one, gets fed to the JSON repairer, and is reported as "did
    /// not satisfy the artifact requirements" — which sends a user to blame
    /// the model for something the token limit did.
    ///
    /// The tool loop never needed this because it calls `complete()` and reads
    /// `CompletionResponse::finish_reason`. The one-shot agent path streams,
    /// and so had no way to know at all. Measured: `qwen2.5-coder:3b` reports
    /// `done_reason: "length"` correctly (probed directly), and the value was
    /// being dropped on the floor.
    Finish {
        reason: String,
    },
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;
    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>>;
    fn provider_name(&self) -> &str;

    /// The URL this provider will actually send to, resolved from the config.
    ///
    /// Exists because eleven tests in `tests/multi_provider.rs` asserted
    /// `create_provider(X, …).provider_name() == X` — comparing a struct field
    /// to the string it was constructed from, which any implementation passes —
    /// and two of them were *named* `openrouter_endpoint_resolves_correctly`
    /// while setting a `base_url` and never reading it. The endpoint was
    /// therefore untested: a provider pointed at the wrong host, or one that
    /// double-suffixed `/v1/messages`, would have gone unnoticed.
    ///
    /// Each implementation calls the **same resolver its request path uses**,
    /// so this cannot report an endpoint the provider would not use. An empty
    /// string means the provider has no single endpoint (the mock), which is
    /// better than a plausible lie.
    fn endpoint(&self) -> String {
        String::new()
    }

    /// Whether this provider supports native structured output (JSON schema constrained decoding).
    fn supports_structured_output(&self) -> bool {
        false
    }

    /// The provider that actually served the most recent call, when this
    /// provider is a chain rather than a single endpoint.
    ///
    /// Cost is priced per provider, so a stage served by a fallback must be
    /// priced with the fallback's table. Default: `None`, meaning "the caller
    /// already knows the provider and should use its own name".
    fn served_by(&self) -> Option<String> {
        None
    }

    /// Request a structured completion constrained to a JSON schema.
    /// Default: delegates to `complete()` (no schema enforcement).
    async fn request_structured(
        &self,
        request: CompletionRequest,
        _schema: &serde_json::Value,
    ) -> Result<CompletionResponse> {
        self.complete(request).await
    }

    /// Speech-to-text: transcribe a WAV audio blob via the provider's STT
    /// endpoint (OpenAI `/v1/audio/transcriptions` and compatible APIs).
    ///
    /// Default: unsupported. Callers should fall back to a local STT engine
    /// (e.g. `whisper`) when every provider reports unsupported.
    async fn transcribe(&self, _audio: &[u8], _language: Option<&str>) -> Result<String> {
        Err(anyhow::anyhow!(
            "provider {} does not support speech-to-text",
            self.provider_name()
        ))
    }
}

/// How long to wait for the TCP connection and TLS handshake.
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// How long a single read may stall before the request is abandoned.
///
/// This was a **total** deadline: `ClientBuilder::timeout` is documented as
/// applying "from when the request starts connecting until the response body
/// has finished", so it capped the whole generation, not the wait between
/// bytes. A 3B model on CPU — the README's zero-setup path — spends minutes
/// producing an answer, and a stage that ran long was killed mid-stream.
///
/// `read_timeout` resets on every chunk, so a slow model is fine as long as it
/// is still saying something; what it catches is a genuinely hung upstream,
/// which is what the original deadline was for.
const READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Build an HTTP client with bounded connect and read timeouts. Without these,
/// a hung upstream API blocks the whole run indefinitely. See research report
/// S12.
pub fn http_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .build()
        .map_err(|e| anyhow!("failed to build HTTP client: {e}"))
}

/// Is this error a timeout?
///
/// Classified by **type**, not by string, and that is the whole point. A
/// total-deadline expiry surfaces as `reqwest::Error` of kind `Body`, whose
/// `Display` is *"request or response body error for url (…)"* — the
/// underlying `io::Error(TimedOut)` is only in the source chain and is never
/// printed. So every classifier in this codebase, all of which match on
/// substrings like `"timeout"`, classified it as a **permanent** failure: not
/// retried at the provider level (transport errors are deliberately not
/// retried there), not retried at the agent level, and not routed through the
/// failover chain. The stage simply died.
///
/// Downcasting to `reqwest::Error` and asking `is_timeout()` is the only
/// version of this question the type system can answer.
pub fn is_timeout_error(e: &anyhow::Error) -> bool {
    if let Some(re) = e.downcast_ref::<reqwest::Error>() {
        if re.is_timeout() || re.is_connect() {
            return true;
        }
        // A stalled **body** is not reported as a timeout even when it is one.
        // Measured against a server that accepts the connection and then never
        // answers: reqwest surfaces
        // `error decoding response body`, with the real cause only in the
        // source chain. `is_timeout()` is false there, so the type test alone
        // would miss the case this exists for — which is a generation that
        // stalled part-way through.
        // `std::error::Error::source`, reached through the `dyn` view so the
        // chain is walkable without knowing reqwest's internals.
        let as_dyn: &(dyn std::error::Error + 'static) = re;
        if is_timeout_cause(as_dyn) {
            return true;
        }
    }
    if is_timeout_cause(e.as_ref()) {
        return true;
    }
    // An error that has already been stringified (logged, or carried through a
    // `map_err` that dropped the type) still answers the old way.
    let msg = e.to_string().to_ascii_lowercase();
    msg.contains("timeout") || msg.contains("timed out")
}

/// Walk an error's source chain for an `io::Error` that is a timeout.
fn is_timeout_cause(err: &(dyn std::error::Error + 'static)) -> bool {
    let mut cursor: Option<&(dyn std::error::Error + 'static)> = Some(err);
    while let Some(e) = cursor {
        if let Some(io) = e.downcast_ref::<std::io::Error>()
            && io.kind() == std::io::ErrorKind::TimedOut
        {
            return true;
        }
        if e.to_string().to_ascii_lowercase().contains("timed out") {
            return true;
        }
        cursor = e.source();
    }
    false
}

const RETRY_MAX_ATTEMPTS: u32 = 4;

/// Wall-clock ceiling for one logical call, across every attempt and back-off.
///
/// Longer than one read, so a slow model is allowed to finish; shorter than
/// `RETRY_MAX_ATTEMPTS` reads, so a stalled upstream is abandoned.
const TOTAL_REQUEST_BUDGET: std::time::Duration = std::time::Duration::from_secs(300);

/// Retry an HTTP request on transient responses: 429 (rate limit) and 5xx server
/// errors. Transport-level errors are not retried here — reqwest surfaces those
/// via `build().send()` and the caller handles them. This keeps the LLM layer
/// resilient to provider rate-limiting without manual intervention.
///
/// `build` rebuilds the request on each attempt, so it must be `FnMut`. The
/// returned `Response` is handed back to the caller for status/body handling.
/// See research report S12.
pub async fn send_request<F, Fut>(
    operation_name: &str,
    mut build: F,
) -> reqwest::Result<reqwest::Response>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = reqwest::Result<reqwest::Response>>,
{
    // A total budget for the whole call, retries and back-off included.
    //
    // The per-read timeout bounds one *read*, which is what lets a slow model
    // finish a long answer — but it multiplies by the attempt count, and the
    // agent layer retries on top of that. Measured against a live provider:
    // after moving from a 120s total deadline to a 120s read deadline, a single
    // stalled stage took over twelve minutes to give up — 4 transport attempts
    // x 3 agent-level attempts, with back-off, and no overall bound anywhere.
    // A stage that cannot finish should fail in a time a person will wait for.
    //
    // It is deliberately longer than one read, so a model that is slow but
    // progressing is not cut off, and shorter than the full retry
    // multiplication, so a stage that has genuinely stalled is.
    let budget = tokio::time::Instant::now() + TOTAL_REQUEST_BUDGET;
    let mut last = None;
    for attempt in 0..RETRY_MAX_ATTEMPTS {
        if tokio::time::Instant::now() >= budget {
            tracing::warn!(
                target: "niki::llm",
                operation = operation_name,
                "request budget exhausted; not attempting again"
            );
            break;
        }
        match build().await {
            Ok(resp) if is_retryable_status(resp.status()) => {
                last = Some(Ok(resp));
            }
            other => return other,
        }
        let exp = 2u64.saturating_pow(attempt + 1);
        let cap = exp.min(30);
        let wait_ms = fastrand::u64(0..=cap.saturating_mul(1000));
        tracing::warn!(
            target: "niki::llm",
            attempt = attempt + 1,
            wait_ms = wait_ms,
            "{operation_name}: retryable status, backing off"
        );
        tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
    }
    // Every loop iteration either returns early (non-retryable or success) or
    // stashes an Ok(retryable response) in `last`, so we always have one here.
    // (The fallback branch is unreachable but required to satisfy the type.)
    last.expect("send_request: loop always stashes a response before returning")
}

/// The HTTP status a provider error carries, when it carries one.
///
/// Every provider formats a non-success response as `HTTP {code}: {body}`
/// (`anthropic.rs:114`, `google.rs:114`, `openai.rs:179`, …), so the status is
/// recoverable from the message. Two callers needed it and had each grown their
/// own reading of the same string: [`crate::agents`] listed `429` and `503`,
/// while [`crate::llm::failover`] listed `500`, `502`, `503`, `504` and `408` —
/// so a 502 was retried by the failover chain and dropped by the agent loop
/// sitting above it.
///
/// Parsed rather than substring-matched on purpose: `"500"` appears in
/// arbitrary response bodies, and a body that happens to contain it would make
/// a permanent 400 look transient. The anchor is the `HTTP ` prefix the
/// providers themselves write.
///
/// **The status is found anywhere in the message, not only at the front.**
/// Every provider's own text starts with it, but the message that reaches
/// this function is usually the one NIKI built around it:
/// `LLM provider error (anthropic): HTTP 500 Internal Server Error: {…}`.
/// Anchored to position zero, this returned `None` for the real string — and
/// since *both* call sites (the agent loop and the failover chain) classify
/// whole error messages, a 500 was classified as permanent at both of them and
/// the chain did not fail over. `cost::a_fallback_served_call_is_priced_by_the
/// _fallback` is what caught it, and it had been red on this branch while no
/// gate runs the full suite.
///
/// What this does **not** weaken: a bare number is still not a status
/// (`"quota exceeded: 429 requests per minute"` is `None`), because a bare
/// number has no `HTTP ` in front of it. And the **first** occurrence wins, so a
/// body mentioning another code cannot override the real one.
pub fn http_status_in(message: &str) -> Option<u16> {
    const ANCHOR: &str = "http ";
    let lower = message.to_ascii_lowercase();
    let mut from = 0usize;
    while let Some(offset) = lower[from..].find(ANCHOR) {
        let at = from + offset + ANCHOR.len();
        let digits: String = lower[at..].chars().take(3).collect();
        // Exactly three digits: `HTTP 5000` is not a 500, and `HTTP 50` is not
        // a code at all.
        let followed_by_digit = lower[at + digits.len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit());
        if digits.len() == 3 && !followed_by_digit {
            if let Ok(code) = digits.parse() {
                return Some(code);
            }
        }
        from = at;
    }
    None
}

/// Whether a status code is transient, for tests.
///
/// A test cannot name a  without depending on reqwest's
/// constructors, and a re-implementation of the list in the test would assert
/// nothing about the real one — so the real predicate is exposed by code.
pub fn status_is_retryable_for_test(code: u16) -> bool {
    is_retryable_status(reqwest::StatusCode::from_u16(code).expect("a valid status code"))
}

/// Whether an HTTP status is worth retrying: 429 and any 5xx.
///
/// Public because the agent loop above `send_request` needs the same answer
/// for a status it can only read out of an error message, and two copies of
/// this rule is how they came to disagree.
pub fn is_retryable_code(code: u16) -> bool {
    code == 429 || (500..600).contains(&code)
}

pub(crate) fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status.as_u16() == 429 || status.is_server_error()
}

/// One prior conversation turn, oldest first.
///
/// The chat surface is the only caller that carries these. Every agent stage
/// is a single-shot call by design — a Planner that could see the Coder's
/// output would no longer be an independent Planner, which is the property the
/// whole pipeline rests on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatTurn {
    /// `"user"` or `"assistant"`. Anything else is coerced to `"user"`.
    pub role: String,
    pub content: String,
}

impl ChatTurn {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
        }
    }
}

/// The full message chain a provider should send: prior turns oldest-first,
/// then `user_message` as the final turn.
///
/// One implementation, four call sites. Every provider used to hand-write a
/// single-element `messages` array, which is why a chat that *displayed* a
/// conversation sent a single message: the transport had nowhere to put the
/// rest of it. Providers differ only in how they name the assistant role
/// (Anthropic/OpenAI/Ollama use `"assistant"`, Google uses `"model"`), so that
/// difference lives in each provider's mapper rather than here.
///
/// Leading assistant turns are dropped. Anthropic rejects a conversation whose
/// first message is not from the user, and a resumed chat whose history was
/// truncated mid-turn can begin with one. Dropping is the conservative fix: the
/// alternative is a 400 from the provider on the first turn after a resume.
pub fn message_chain(request: &CompletionRequest) -> Vec<ChatTurn> {
    let mut chain: Vec<ChatTurn> = request
        .history
        .iter()
        .filter(|t| !t.content.trim().is_empty())
        .cloned()
        .collect();
    while chain.first().is_some_and(|t| t.role != "user") {
        chain.remove(0);
    }
    chain.push(ChatTurn::user(request.user_message.clone()));
    chain
}

#[derive(Clone, Debug, Default)]
pub struct CompletionRequest {
    pub model: String,
    pub system_prompt: String,
    pub user_message: String,
    pub max_tokens: u32,
    pub temperature: f32,
    /// Prior conversation turns, oldest first. Empty for every agent stage.
    ///
    /// This field is the reason the chat surface is a chat. Without it the
    /// only thing a provider could receive was `user_message`, so turn 3 was
    /// sent with turns 1 and 2 erased — while the transcript scrolled, and
    /// persisted, and resumed, showing a conversation the model had never seen.
    /// A user testing recall gets a confidently wrong answer, and nothing on
    /// screen says the context was dropped.
    pub history: Vec<ChatTurn>,
    /// Optional JSON schema for structured output. When present, providers that
    /// support structured output will use constrained decoding to guarantee the
    /// response matches the schema exactly.
    pub json_schema: Option<String>,
    /// Optional tool specifications. When present, providers that support native
    /// tool calling emit `tool_calls` in the response instead of (or in addition
    /// to) text. Providers without native support simply ignore this field and
    /// return `tool_calls` empty, so callers fall through to plain text.
    pub tools: Option<Vec<ToolSpec>>,
    /// Reasoning effort, for models that expose one: `low`, `medium`, `high`,
    /// and whatever else a provider offers.
    ///
    /// `None` sends no such field at all, which is the default and the safe
    /// setting — an OpenAI-compatible provider that does not recognise the key
    /// may reject the entire request, so this is something a user opts into
    /// rather than something NIKI chooses. Providers that do recognise it are
    /// the ones where it matters most: a reasoning model is billed and made to
    /// wait by its thinking budget, and the model name never tells you the
    /// range a given model accepts.
    pub reasoning_effort: Option<String>,
}

/// A single tool exposed to the LLM for native tool calling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON-schema object describing the tool's parameters.
    pub parameters: serde_json::Value,
}

/// A tool invocation requested by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// Parsed argument object (may be empty `{}`).
    pub arguments: serde_json::Value,
}

#[derive(Debug)]
pub struct CompletionResponse {
    pub content: String,
    pub model: String,
    pub usage: TokenUsage,
    /// Tool invocations requested by the model. Empty for plain-text responses.
    pub tool_calls: Vec<ToolCall>,
    /// Why the model stopped, in the provider's own vocabulary:
    /// `stop`, `length`, `max_tokens`, `tool_calls`, ...
    ///
    /// This exists for one reason: a response cut off at the token limit has
    /// tool-call arguments that are *silently truncated JSON*. Executing those
    /// is the worst kind of wrong — a `write` tool with half a path, a `bash`
    /// tool with half a command — and it looks like a successful run. Codex
    /// guards against it by failing every tool call carried by a message that
    /// stopped on `length` (`agent-loop.ts:263-269`). We could not, because
    /// nothing here carried the reason at all.
    ///
    /// `None` means the provider does not report it, which callers must treat
    /// as "unknown", never as "definitely not truncated".
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    /// Prompt-cache hits (Anthropic cache_read/create, OpenAI cached_tokens,
    /// Google cachedContentTokenCount). Priced below input rate; `0` when the
    /// provider did not report a split.
    pub cached_input_tokens: u32,
    /// Tokens spent on extended thinking / reasoning summaries (OpenAI
    /// reasoning_tokens, Google thoughtsTokenCount). Priced at output rate.
    pub reasoning_tokens: u32,
}

impl TokenUsage {
    /// Fold one completed request's usage into a running total.
    ///
    /// This exists as a named method, rather than as a `+=` written at each
    /// call site, for a specific reason: the tool loop and the repair-retry
    /// path once used `.max()` instead, and the tests that were supposed to
    /// catch that asserted a *local copy* of the arithmetic rather than this
    /// code. The canary corpus found it. Centralising the operation means the
    /// test and the product cannot drift apart again.
    ///
    /// Note the scope: accumulation is correct **across** requests. Within a
    /// single stream, usage chunks describe one request and must be maxed
    /// instead — Anthropic emits two disjoint chunks (message_start carries
    /// input tokens, message_delta carries output tokens).
    pub fn accumulate(&mut self, step: &TokenUsage) {
        self.input_tokens += step.input_tokens;
        self.output_tokens += step.output_tokens;
        self.cached_input_tokens += step.cached_input_tokens;
        self.reasoning_tokens += step.reasoning_tokens;
    }
}

pub fn create_provider(name: &str, config: &ProviderConfig) -> Result<Box<dyn LlmProvider>> {
    match name {
        "anthropic" => Ok(Box::new(super::anthropic::AnthropicProvider::new(config)?)),
        // All OpenAI-compatible providers share the same implementation.
        // The only difference is base_url configured in niki.toml.
        // All OpenAI-compatible providers go through the named constructor so
        // missing-key errors and logs name the configured slug (GROQ_API_KEY,
        // not OPENAI_API_KEY) even when no base_url is set and the URL-based
        // guess would fall back to "openai".
        "openai" | "openrouter" | "nvidia" | "together" | "groq" | "deepseek" | "zen" | "kimi"
        | "kilo" => Ok(Box::new(super::openai::OpenAiProvider::new_named(
            config, name,
        )?)),
        "google" => Ok(Box::new(super::google::GoogleProvider::new(config)?)),
        "ollama" => Ok(Box::new(super::ollama::OllamaProvider::new(config)?)),
        "mock" => Ok(Box::new(super::mock::MockProvider::new(
            config.base_url.as_deref(),
        )?)),
        _ => Err(anyhow!("Unknown provider: {name}")),
    }
}

/// Default base URLs for known OpenAI-compatible providers.
/// Used when `base_url` is not explicitly set in config.
pub fn default_base_url(name: &str) -> Option<&'static str> {
    match name {
        "openrouter" => Some("https://openrouter.ai/api/v1"),
        "nvidia" => Some("https://integrate.api.nvidia.com/v1"),
        "together" => Some("https://api.together.xyz/v1"),
        "groq" => Some("https://api.groq.com/openai/v1"),
        "deepseek" => Some("https://api.deepseek.com/v1"),
        "zen" => Some("https://opencode.ai/zen/v1"),
        "kimi" => Some("https://api.kimi.com/coding/v1"),
        "kilo" => Some("https://api.kilo.ai/api/gateway"),
        _ => None,
    }
}

/// Actionable "no API key" error (V8 first-hour-error rule: what + where +
/// exact fix). Keeps the legacy "API key not configured" prefix so existing
/// assertions and user muscle memory still match.
pub fn missing_key_error(provider_name: &str) -> anyhow::Error {
    // (label, env var, `niki auth login` slug or None when login is unsupported)
    let (label, env, slug): (&str, &str, Option<&str>) = match provider_name {
        "anthropic" => ("Anthropic", "ANTHROPIC_API_KEY", Some("anthropic")),
        "openai" => ("OpenAI", "OPENAI_API_KEY", Some("openai")),
        "google" => ("Google", "GOOGLE_API_KEY", Some("google")),
        "openrouter" => ("OpenRouter", "OPENROUTER_API_KEY", Some("openrouter")),
        "groq" => ("Groq", "GROQ_API_KEY", Some("groq")),
        "deepseek" => ("DeepSeek", "DEEPSEEK_API_KEY", Some("deepseek")),
        "together" => ("Together", "TOGETHER_API_KEY", Some("together")),
        "nvidia" => ("NVIDIA", "NVIDIA_API_KEY", Some("nvidia")),
        "zen" => ("OpenCode Zen", "OPENCODE_API_KEY", Some("zen")),
        "kimi" => ("Kimi Code", "KIMI_API_KEY", Some("kimi")),
        "kilo" => ("KiloCode Gateway", "KILO_API_KEY", Some("kilo")),
        _ => ("Provider", "PROVIDER_API_KEY", None),
    };
    let fix = match slug {
        Some(s) => format!(
            "Set {env}, add api_key under [providers.{provider_name}] in niki.toml, \
             or run `niki auth login --provider {s}`."
        ),
        None => format!("Set {env} or add api_key under [providers.{provider_name}] in niki.toml."),
    };
    anyhow::anyhow!("{label} API key not configured. {fix}")
}

/// Well-known model shorthands, resolved per provider at config load.
/// Aliases match the WHOLE model string (case-insensitive) — substrings never
/// rewrite, so pinned versions like `claude-sonnet-4-20250514` pass through
/// untouched, as do unknown providers and models.
pub fn resolve_model_alias(provider: &str, model: &str) -> String {
    let m = model.trim().to_lowercase();
    let hit: Option<&str> = match provider.to_lowercase().as_str() {
        "anthropic" => match m.as_str() {
            "opus" => Some("claude-opus-4"),
            "sonnet" => Some("claude-sonnet-4"),
            "haiku" => Some("claude-haiku"),
            _ => None,
        },
        "openai" => match m.as_str() {
            "4o" => Some("gpt-4o"),
            "4o-mini" | "mini" => Some("gpt-4o-mini"),
            "o1" => Some("o1"),
            "o3-mini" | "o3" => Some("o3-mini"),
            _ => None,
        },
        "google" => match m.as_str() {
            "flash" => Some("gemini-2.0-flash"),
            "pro" => Some("gemini-2.5-pro"),
            _ => None,
        },
        _ => None,
    };
    match hit {
        Some(canonical) => {
            tracing::info!(
                target: "niki::model",
                "model alias '{model}' resolved to '{canonical}' for provider '{provider}'"
            );
            canonical.to_string()
        }
        None => model.to_string(),
    }
}

pub fn redact_secrets(text: &str) -> String {
    let mut result = text.to_string();
    result = redact_bearer_tokens(&result);
    result = redact_api_keys(&result);
    result = redact_generic_patterns(&result);
    result
}

fn redact_bearer_tokens(text: &str) -> String {
    let mut result = text.to_string();
    let re = regex::Regex::new(r"(?i)(Bearer\s+)[A-Za-z0-9_\-\.]+").expect("valid regex");
    result = re.replace_all(&result, "${1}[REDACTED]").to_string();
    result
}

fn redact_api_keys(text: &str) -> String {
    let mut result = text.to_string();
    let patterns = [
        r"sk-[A-Za-z0-9_\-]{20,}",
        r"AKIA[A-Z0-9]{16}",
        r"ghp_[A-Za-z0-9]{36}",
        r"AIza[A-Za-z0-9_\-]{35}",
        // Hugging Face user and org tokens. `hf_` is two characters, so the
        // 40-char base64 catch-all below never reaches it — an HF token was
        // the one shape measured here that survived redaction untouched.
        r"hf_[A-Za-z0-9]{34,40}",
        // The catch-all, **narrowed**.
        //
        // It used to be `[A-Za-z0-9+/]{40,}={0,2}`, which is "any long
        // unbroken alphanumeric run". Measured against real text, that is a
        // list of things that are not secrets:
        //
        //     git sha (40 hex)     REDACTED
        //     sha256 (64 hex)       REDACTED
        //     long word             REDACTED
        //     base64 asset path     REDACTED
        //     minified js chunk     REDACTED
        //
        // A git SHA is the worst of them, because `report.md`, `trace.jsonl`
        // and the TUI all reference commits, and a redacted one is an
        // unreferenceable piece of evidence in the middle of a report.
        //
        // What separates an encoded secret from an identifier is *shape*, not
        // length: base64 of a random secret mixes cases and digits, while a
        // SHA is lowercase hex and a word is lowercase letters. So the run
        // must contain at least one uppercase letter **and** at least one
        // digit. That keeps every realistic key caught — the vendor-specific
        // patterns above carry the ones with a known prefix, and the JSON and
        // key=value patterns in `redact_generic_patterns` carry the rest —
        // while letting identifiers through.
    ];
    for pattern in &patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            result = re.replace_all(&result, "[REDACTED]").to_string();
        }
    }
    redact_encoded_runs(&result)
}

/// Blank long alphanumeric runs that look like an encoded secret.
///
/// Split out from [`redact_api_keys`] because the regex crate has no
/// lookaround, so "40+ characters **and** at least one uppercase **and** at
/// least one digit" cannot be one pattern. The shape test is in code, which is
/// also where it can be read.
fn redact_encoded_runs(text: &str) -> String {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re =
        RE.get_or_init(|| regex::Regex::new(r"[A-Za-z0-9+/]{40,}={0,2}").expect("a fixed pattern"));
    re.replace_all(text, |caps: &regex::Captures<'_>| {
        let run = caps.get(0).map_or("", |m| m.as_str());
        if looks_like_encoded_secret(run) {
            "[REDACTED]".to_string()
        } else {
            run.to_string()
        }
    })
    .to_string()
}

/// Whether a long alphanumeric run has the *shape* of an encoded secret.
///
/// A base64 rendering of a random secret mixes upper case, lower case and
/// digits. A git SHA is lower-case hex; an English word is lower-case letters;
/// a minified bundle is long but usually punctuated. Requiring an uppercase
/// letter and a digit separates the first two, which is the harm that was
/// measured.
fn looks_like_encoded_secret(run: &str) -> bool {
    let mut upper = false;
    let mut digit = false;
    for c in run.chars() {
        match c {
            'A'..='Z' => upper = true,
            '0'..='9' => digit = true,
            _ => {}
        }
    }
    upper && digit
}

fn redact_generic_patterns(text: &str) -> String {
    let mut result = text.to_string();
    let patterns = [
        r"(?i)(api[_-]?key=)[A-Za-z0-9_\-\.]+",
        r"(?i)([?&]key=)[A-Za-z0-9_\-\.]+",
        r"(?i)(password=)[^\s&]+",
        r"(?i)(secret=)[A-Za-z0-9_\-\.]+",
        r"(?i)(token=)[A-Za-z0-9_\-\.]+",
        r"(?i)(authorization:)[^\n\r]+",
        // A quoted field name, with the colon *or* an `=` after it, and an
        // optional space. Every one of these is a shape a provider error body
        // arrives in — and provider error bodies are JSON, which is the single
        // place this function is actually applied. The `=`-only forms above
        // catch `api_key=...` and miss `{"api_key": "..."}` entirely, so a key
        // in an error response reached the log verbatim. The value class ends
        // at the closing quote, so the surrounding JSON stays readable.
        r#"(?i)(["']?(?:api[_-]?key|access[_-]?token|auth[_-]?token|secret|client[_-]?secret|password|passwd)["']?\s*[:=]\s*["']?)[^"'\s,}&]+"#,
    ];
    for pattern in &patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            result = re.replace_all(&result, "${1}[REDACTED]").to_string();
        }
    }
    result
}

/// Cap serialized tool specs before sending: at most 16 tools, descriptions
/// truncated to 2k chars, parameter schemas capped at ~8k serialized chars.
/// Prevents a large registry from blowing up the prompt (context-rot defense).
pub fn capped_tool_specs(tools: &[ToolSpec]) -> Vec<ToolSpec> {
    tools
        .iter()
        .take(16)
        .map(|t| {
            let description: String = t.description.chars().take(2000).collect();
            let params_str = t.parameters.to_string();
            let parameters = if params_str.len() > 8192 {
                serde_json::json!({"type": "object"})
            } else {
                t.parameters.clone()
            };
            ToolSpec {
                name: t.name.chars().take(128).collect(),
                description,
                parameters,
            }
        })
        .collect()
}

/// Cap a tool-call argument payload (~16k serialized chars); oversized args
/// are replaced with an explicit truncation marker object.
pub fn capped_tool_arguments(args: &serde_json::Value) -> serde_json::Value {
    let s = args.to_string();
    if s.len() > 16384 {
        serde_json::json!({"_truncated": "arguments exceeded 16k chars"})
    } else {
        args.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_providers_construct_and_name_correctly() {
        for (slug, base_substr) in [
            ("zen", "opencode.ai/zen"),
            ("kimi", "api.kimi.com"),
            ("kilo", "api.kilo.ai"),
        ] {
            let config = ProviderConfig {
                api_key: Some("test-key".into()),
                base_url: None,
                default_model: "m".into(),
            };
            let p = create_provider(slug, &config).unwrap();
            assert_eq!(p.provider_name(), slug);
            // Default endpoint resolves without explicit base_url.
            assert!(default_base_url(slug).unwrap().contains(base_substr));
        }
    }

    #[test]
    fn gateway_missing_keys_name_exact_env() {
        let e = missing_key_error("zen").to_string();
        assert!(e.contains("OPENCODE_API_KEY"), "{e}");
        assert!(e.contains("niki auth login --provider zen"), "{e}");
        let e = missing_key_error("kimi").to_string();
        assert!(e.contains("KIMI_API_KEY"), "{e}");
        let e = missing_key_error("kilo").to_string();
        assert!(e.contains("KILO_API_KEY"), "{e}");
    }

    #[test]
    fn missing_key_error_names_exact_fix() {
        // Legacy prefix preserved for existing assertions and muscle memory.
        let e = missing_key_error("anthropic").to_string();
        assert!(e.contains("Anthropic API key not configured"), "{e}");
        assert!(e.contains("ANTHROPIC_API_KEY"), "{e}");
        assert!(e.contains("niki auth login --provider anthropic"), "{e}");

        // Gateway without keyring login configured: env/file guidance only.
        // (All built-in providers support `niki auth login`; this arm is for
        // custom names that reach the generic OpenAI-compatible path.)
        let e = missing_key_error("groq").to_string();
        assert!(e.contains("GROQ_API_KEY"), "{e}");
        assert!(e.contains("niki auth login --provider groq"), "{e}");
    }
}
