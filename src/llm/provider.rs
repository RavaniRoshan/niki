use anyhow::{anyhow, Result};
use futures::Stream;
use std::pin::Pin;
use crate::config::ProviderConfig;
use async_trait::async_trait;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;
    async fn stream(&self, request: CompletionRequest) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>>;
    fn provider_name(&self) -> &str;
}

pub struct CompletionRequest {
    pub model: String,
    pub system_prompt: String,
    pub user_message: String,
    pub max_tokens: u32,
    pub temperature: f32,
}

pub struct CompletionResponse {
    pub content: String,
    pub model: String,
    pub usage: TokenUsage,
}

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
        _ => Err(anyhow!("Unknown provider: {}", name)),
    }
}
