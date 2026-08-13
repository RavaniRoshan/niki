use super::provider::{
    CompletionRequest, CompletionResponse, LlmProvider, StreamChunk, TokenUsage, redact_secrets,
};
use crate::config::ProviderConfig;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use serde_json::json;
use std::pin::Pin;

/// Resolve a base URL to the OpenAI chat-completions endpoint. `base_url`
/// follows the standard SDK convention (a base such as `https://api.openai.com/v1`),
/// with `/chat/completions` appended. A full endpoint is left untouched so an
/// explicit `base_url` is never double-suffixed.
fn openai_endpoint(base: &str) -> String {
    let b = base.trim_end_matches('/');
    if b.ends_with("/v1/chat/completions") || b.ends_with("/chat/completions") {
        b.to_string()
    } else if b.ends_with("/v1") {
        format!("{b}/chat/completions")
    } else {
        format!("{b}/v1/chat/completions")
    }
}

pub struct OpenAiProvider {
    config: ProviderConfig,
    client: Client,
    provider_name: String,
}

impl OpenAiProvider {
    pub fn new(config: &ProviderConfig) -> Result<Self> {
        let _api_key = config
            .api_key
            .clone()
            .ok_or_else(|| anyhow!("API key not configured for this provider"))?;
        // Derive provider name from base_url or config for logging
        let provider_name = config
            .base_url
            .as_ref()
            .and_then(|url| {
                if url.contains("openrouter") {
                    Some("openrouter")
                } else if url.contains("nvidia") {
                    Some("nvidia")
                } else if url.contains("together") {
                    Some("together")
                } else if url.contains("groq") {
                    Some("groq")
                } else if url.contains("deepseek") {
                    Some("deepseek")
                } else {
                    None
                }
            })
            .unwrap_or("openai")
            .to_string();
        Ok(Self {
            config: config.clone(),
            client: super::provider::http_client()?,
            provider_name,
        })
    }

    /// Resolve the base URL, applying provider-specific defaults.
    fn base_url(&self) -> &str {
        self.config
            .base_url
            .as_deref()
            .or_else(|| super::provider::default_base_url(&self.provider_name))
            .unwrap_or("https://api.openai.com/v1")
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| anyhow!("API key not configured for this provider"))?;
        let url = openai_endpoint(self.base_url());

        let payload = json!({
            "model": request.model,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "messages": [
                {
                    "role": "system",
                    "content": request.system_prompt
                },
                {
                    "role": "user",
                    "content": request.user_message
                }
            ]
        });

        let req = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("content-type", "application/json")
            .json(&payload);
        let resp = super::provider::send_request("openai request", || req.try_clone().unwrap().send()).await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(crate::NikiError::LlmProvider {
                provider: self.provider_name.clone(),
                message: format!("HTTP {}: {}", status, redact_secrets(&body)),
            }
            .into());
        }

        let data: serde_json::Value = resp.json().await?;
        let content = data["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let input_tokens = data["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32;
        let output_tokens = data["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32;

        Ok(CompletionResponse {
            content,
            model: request.model,
            usage: TokenUsage {
                input_tokens,
                output_tokens,
            },
        })
    }

    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| anyhow!("API key not configured for this provider"))?;
        let url = openai_endpoint(self.base_url());

        let payload = json!({
            "model": request.model,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "messages": [
                {
                    "role": "system",
                    "content": request.system_prompt
                },
                {
                    "role": "user",
                    "content": request.user_message
                }
            ],
            "stream": true,
            "stream_options": {
                "include_usage": true
            }
        });

        let req = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("content-type", "application/json")
            .json(&payload);
        let resp = super::provider::send_request("openai request", || req.try_clone().unwrap().send()).await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(crate::NikiError::LlmProvider {
                provider: self.provider_name.clone(),
                message: format!("HTTP {}: {}", status, redact_secrets(&body)),
            }
            .into());
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
                            buffer = buffer[pos + 1..].to_string();

                            let line = line.trim();
                            if let Some(data) = line.strip_prefix("data: ") {
                                if data == "[DONE]" {
                                    continue;
                                }
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                                    if let Some(usage) = json["usage"].as_object() {
                                        // Final usage chunk (choices is empty / absent).
                                        if tx
                                            .send(Ok(StreamChunk::Usage(TokenUsage {
                                                input_tokens: usage["prompt_tokens"]
                                                    .as_u64()
                                                    .unwrap_or(0)
                                                    as u32,
                                                output_tokens: usage["completion_tokens"]
                                                    .as_u64()
                                                    .unwrap_or(0)
                                                    as u32,
                                            })))
                                            .is_err()
                                        {
                                            return;
                                        }
                                    } else if let Some(choices) = json["choices"].as_array()
                                        && let Some(choice) = choices.first()
                                        && let Some(text) = choice["delta"]["content"].as_str()
                                        && tx.send(Ok(StreamChunk::Text(text.to_string()))).is_err()
                                    {
                                        return;
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

        Ok(Box::pin(
            tokio_stream::wrappers::UnboundedReceiverStream::new(rx),
        ))
    }

    fn provider_name(&self) -> &str {
        &self.provider_name
    }

    fn supports_structured_output(&self) -> bool {
        true
    }

    async fn request_structured(
        &self,
        request: CompletionRequest,
        schema: &serde_json::Value,
    ) -> Result<CompletionResponse> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| anyhow!("API key not configured for this provider"))?;
        let url = openai_endpoint(self.base_url());

        let payload = json!({
            "model": request.model,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "messages": [
                {
                    "role": "system",
                    "content": request.system_prompt
                },
                {
                    "role": "user",
                    "content": request.user_message
                }
            ],
            "response_format": {
                "type": "json_schema",
                "json_schema": {
                    "name": "structured_output",
                    "schema": schema
                }
            }
        });

        let req = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("content-type", "application/json")
            .json(&payload);
        let resp = super::provider::send_request("openai request", || req.try_clone().unwrap().send()).await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(crate::NikiError::LlmProvider {
                provider: self.provider_name.clone(),
                message: format!("HTTP {}: {}", status, redact_secrets(&body)),
            }
            .into());
        }

        let data: serde_json::Value = resp.json().await?;
        let content = data["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let input_tokens = data["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32;
        let output_tokens = data["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32;

        Ok(CompletionResponse {
            content,
            model: request.model,
            usage: TokenUsage {
                input_tokens,
                output_tokens,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::openai_endpoint;

    #[test]
    fn appends_standard_path_to_v1_base() {
        assert_eq!(
            openai_endpoint("https://api.openai.com/v1"),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn appends_v1_chat_completions_to_host_base() {
        assert_eq!(
            openai_endpoint("https://api.openai.com"),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn leaves_full_endpoint_untouched() {
        assert_eq!(
            openai_endpoint("https://gw.example.com/v1/chat/completions"),
            "https://gw.example.com/v1/chat/completions"
        );
        assert_eq!(
            openai_endpoint("https://gw.example.com/chat/completions"),
            "https://gw.example.com/chat/completions"
        );
    }

    #[test]
    fn trims_trailing_slash() {
        assert_eq!(
            openai_endpoint("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/chat/completions"
        );
    }
}
