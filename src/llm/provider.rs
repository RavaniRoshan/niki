use crate::config::ProviderConfig;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use futures::Stream;
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

#[derive(Clone)]
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
}

#[derive(Debug)]
pub struct CompletionResponse {
    pub content: String,
    pub model: String,
    pub usage: TokenUsage,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
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
