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
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;
    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>>;
    fn provider_name(&self) -> &str;

    /// Whether this provider supports native structured output (JSON schema constrained decoding).
    fn supports_structured_output(&self) -> bool {
        false
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

/// Build an HTTP client with a bounded request timeout. Without this, a hung
/// upstream API blocks the whole run indefinitely. See research report S12.
pub fn http_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| anyhow!("failed to build HTTP client: {e}"))
}

const RETRY_MAX_ATTEMPTS: u32 = 4;

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
    let mut last = None;
    for attempt in 0..RETRY_MAX_ATTEMPTS {
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

pub(crate) fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status.as_u16() == 429 || status.is_server_error()
}

#[derive(Clone, Debug, Default)]
pub struct CompletionRequest {
    pub model: String,
    pub system_prompt: String,
    pub user_message: String,
    pub max_tokens: u32,
    pub temperature: f32,
    /// Optional JSON schema for structured output. When present, providers that
    /// support structured output will use constrained decoding to guarantee the
    /// response matches the schema exactly.
    pub json_schema: Option<String>,
    /// Optional tool specifications. When present, providers that support native
    /// tool calling emit `tool_calls` in the response instead of (or in addition
    /// to) text. Providers without native support simply ignore this field and
    /// return `tool_calls` empty, so callers fall through to plain text.
    pub tools: Option<Vec<ToolSpec>>,
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

pub fn create_provider(name: &str, config: &ProviderConfig) -> Result<Box<dyn LlmProvider>> {
    match name {
        "anthropic" => Ok(Box::new(super::anthropic::AnthropicProvider::new(config)?)),
        // All OpenAI-compatible providers share the same implementation.
        // The only difference is base_url configured in niki.toml.
        "openai" | "openrouter" | "nvidia" | "together" | "groq" | "deepseek" => {
            Ok(Box::new(super::openai::OpenAiProvider::new(config)?))
        }
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
        "openrouter" => ("OpenRouter", "OPENROUTER_API_KEY", None),
        "groq" => ("Groq", "GROQ_API_KEY", None),
        "deepseek" => ("DeepSeek", "DEEPSEEK_API_KEY", None),
        "together" => ("Together", "TOGETHER_API_KEY", None),
        "nvidia" => ("NVIDIA", "NVIDIA_API_KEY", None),
        _ => ("Provider", "PROVIDER_API_KEY", None),
    };
    let fix = match slug {
        Some(s) => format!(
            "Set {env}, add api_key under [providers.{provider_name}] in niki.toml, \
             or run `niki auth login --provider {s}`."
        ),
        None => format!(
            "Set {env} or add api_key under [providers.{provider_name}] in niki.toml \
             (`niki auth login` supports anthropic/openai/google only)."
        ),
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
        r"[A-Za-z0-9+/]{40,}={0,2}",
    ];
    for pattern in &patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            result = re.replace_all(&result, "[REDACTED]").to_string();
        }
    }
    result
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
    ];
    for pattern in &patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            result = re.replace_all(&result, "${1}[REDACTED]").to_string();
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_key_error_names_exact_fix() {
        // Legacy prefix preserved for existing assertions and muscle memory.
        let e = missing_key_error("anthropic").to_string();
        assert!(e.contains("Anthropic API key not configured"), "{e}");
        assert!(e.contains("ANTHROPIC_API_KEY"), "{e}");
        assert!(e.contains("niki auth login --provider anthropic"), "{e}");

        // Providers without keyring login get env/file guidance instead of a
        // login command that would fail.
        let e = missing_key_error("groq").to_string();
        assert!(e.contains("GROQ_API_KEY"), "{e}");
        assert!(!e.contains("auth login --provider"), "{e}");
    }
}
