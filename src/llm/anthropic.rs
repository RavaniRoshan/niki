use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;
use reqwest::Client;
use serde_json::json;
use crate::config::ProviderConfig;
use super::provider::{CompletionRequest, CompletionResponse, LlmProvider, StreamChunk, TokenUsage};

/// Resolve a base URL to the Anthropic messages endpoint. `base_url` follows the
/// standard SDK convention (a host/base, e.g. `https://api.anthropic.com`), with
/// the `/v1/messages` path appended. A full endpoint is left untouched so an
/// explicit `base_url` is never double-suffixed.
fn anthropic_endpoint(base: &str) -> String {
    let b = base.trim_end_matches('/');
    if b.ends_with("/v1/messages") || b.ends_with("/messages") {
        b.to_string()
    } else if b.ends_with("/v1") {
        format!("{b}/messages")
    } else {
        format!("{b}/v1/messages")
    }
}

pub struct AnthropicProvider {
    config: ProviderConfig,
    client: Client,
}

impl AnthropicProvider {
    pub fn new(config: &ProviderConfig) -> Result<Self> {
        let _api_key = config.api_key.clone().ok_or_else(|| anyhow!("Anthropic API key not configured"))?;
        Ok(Self {
            config: config.clone(),
            client: Client::new(),
        })
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let api_key = self.config.api_key.as_ref().unwrap();
        let url = anthropic_endpoint(
            self.config
                .base_url
                .as_deref()
                .unwrap_or("https://api.anthropic.com"),
        );

        let payload = json!({
            "model": request.model,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "system": request.system_prompt,
            "messages": [
                {
                    "role": "user",
                    "content": request.user_message
                }
            ]
        });

        let resp = self.client.post(url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(crate::NikiError::LlmProvider { provider: "anthropic".into(), message: format!("HTTP {}: {}", status, body) }.into());
        }

        let data: serde_json::Value = resp.json().await?;
        let content = data["content"][0]["text"].as_str().unwrap_or("").to_string();
        
        let input_tokens = data["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32;
        let output_tokens = data["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32;

        Ok(CompletionResponse {
            content,
            model: request.model,
            usage: TokenUsage { input_tokens, output_tokens },
        })
    }

    async fn stream(&self, request: CompletionRequest) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>> {
        let api_key = self.config.api_key.as_ref().unwrap();
        let url = anthropic_endpoint(
            self.config
                .base_url
                .as_deref()
                .unwrap_or("https://api.anthropic.com"),
        );

        let payload = json!({
            "model": request.model,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "system": request.system_prompt,
            "messages": [
                {
                    "role": "user",
                    "content": request.user_message
                }
            ],
            "stream": true
        });

        let resp = self.client.post(url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(crate::NikiError::LlmProvider { provider: "anthropic".into(), message: format!("HTTP {}: {}", status, body) }.into());
        }

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        
        tokio::spawn(async move {
            use futures::StreamExt;
            let mut stream = resp.bytes_stream();
            let mut buffer = String::new();
            
            while let Some(chunk_res) = stream.next().await {
                match chunk_res {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        while let Some(pos) = buffer.find('\n') {
                            let line = buffer[..pos].to_string();
                            buffer = buffer[pos+1..].to_string();
                            
                            let line = line.trim();
                            if line.starts_with("data: ") {
                                let data = &line[6..];
                                if data == "[DONE]" {
                                    continue;
                                }
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                                    if json["type"] == "content_block_delta" {
                                        if let Some(text) = json["delta"]["text"].as_str() {
                                            if tx.send(Ok(StreamChunk::Text(text.to_string()))).is_err() {
                                                return;
                                            }
                                        }
                                    } else if json["type"] == "message_start" {
                                        // input_tokens are known up front
                                        if let Some(input) = json["message"]["usage"]["input_tokens"].as_u64() {
                                            if tx.send(Ok(StreamChunk::Usage(TokenUsage {
                                                input_tokens: input as u32,
                                                output_tokens: 0,
                                            }))).is_err() {
                                                return;
                                            }
                                        }
                                    } else if json["type"] == "message_delta" {
                                        // output_tokens (and possibly the final input_tokens) arrive here
                                        if let Some(output) = json["usage"]["output_tokens"].as_u64() {
                                            if tx.send(Ok(StreamChunk::Usage(TokenUsage {
                                                input_tokens: 0,
                                                output_tokens: output as u32,
                                            }))).is_err() {
                                                return;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(anyhow::anyhow!("Stream error: {}", e)));
                        return;
                    }
                }
            }
        });

        Ok(Box::pin(tokio_stream::wrappers::UnboundedReceiverStream::new(rx)))
    }

    fn provider_name(&self) -> &str {
        "anthropic"
    }
}

#[cfg(test)]
mod tests {
    use super::anthropic_endpoint;

    #[test]
    fn appends_standard_path_to_base() {
        assert_eq!(
            anthropic_endpoint("https://api.anthropic.com"),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[test]
    fn appends_messages_to_v1_base() {
        assert_eq!(
            anthropic_endpoint("https://api.anthropic.com/v1"),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[test]
    fn leaves_full_endpoint_untouched() {
        assert_eq!(
            anthropic_endpoint("https://gw.example.com/v1/messages"),
            "https://gw.example.com/v1/messages"
        );
        assert_eq!(
            anthropic_endpoint("https://gw.example.com/messages"),
            "https://gw.example.com/messages"
        );
    }

    #[test]
    fn trims_trailing_slash() {
        assert_eq!(
            anthropic_endpoint("https://api.anthropic.com/"),
            "https://api.anthropic.com/v1/messages"
        );
    }
}
