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
        "openai" => Ok(Box::new(super::openai::OpenAiProvider::new(config)?)),
        "google" => Ok(Box::new(super::google::GoogleProvider::new(config)?)),
        "ollama" => Ok(Box::new(super::ollama::OllamaProvider::new(config)?)),
        "mock" => Ok(Box::new(super::mock::MockProvider::new(config.base_url.as_deref())?)),
        _ => Err(anyhow!("Unknown provider: {}", name)),
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
    let re = regex::Regex::new(r"(?i)(Bearer\s+)[A-Za-z0-9_\-\.]+")
        .expect("valid regex");
    result = re.replace_all(&result, "${1}[REDACTED]").to_string();
    result
}

fn redact_api_keys(text: &str) -> String {
    let mut result = text.to_string();
    let patterns = [
        r"sk-[A-Za-z0-9_\-]{20,}",
        r"AKIA[A-Z0-9]{16}",
        r"ghp_[A-Za-z0-9]{36}",
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
