use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;
use reqwest::Client;
use serde_json::json;
use crate::config::ProviderConfig;
use super::provider::{CompletionRequest, CompletionResponse, LlmProvider, TokenUsage};

pub struct GoogleProvider {
    config: ProviderConfig,
    client: Client,
}

impl GoogleProvider {
    pub fn new(config: &ProviderConfig) -> Result<Self> {
        let _api_key = config.api_key.clone().ok_or_else(|| anyhow!("Google API key not configured"))?;
        Ok(Self {
            config: config.clone(),
            client: Client::new(),
        })
    }
}

#[async_trait]
impl LlmProvider for GoogleProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let api_key = self.config.api_key.as_ref().unwrap();
        
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            request.model,
            api_key
        );

        let payload = json!({
            "contents": [{
                "parts": [{"text": request.user_message}]
            }],
            "systemInstruction": {
                "parts": [{"text": request.system_prompt}]
            },
            "generationConfig": {
                "maxOutputTokens": request.max_tokens,
                "temperature": request.temperature,
            }
        });

        let resp = self.client.post(&url)
            .header("content-type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(crate::NikiError::LlmProvider { provider: "google".into(), message: format!("HTTP {}: {}", status, body) }.into());
        }

        let data: serde_json::Value = resp.json().await?;
        let content = data["candidates"][0]["content"]["parts"][0]["text"].as_str().unwrap_or("").to_string();
        
        let input_tokens = data["usageMetadata"]["promptTokenCount"].as_u64().unwrap_or(0) as u32;
        let output_tokens = data["usageMetadata"]["candidatesTokenCount"].as_u64().unwrap_or(0) as u32;

        Ok(CompletionResponse {
            content,
            model: request.model,
            usage: TokenUsage { input_tokens, output_tokens },
        })
    }

    async fn stream(&self, request: CompletionRequest) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let api_key = self.config.api_key.as_ref().unwrap();
        
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
            request.model,
            api_key
        );

        let payload = json!({
            "contents": [{
                "parts": [{"text": request.user_message}]
            }],
            "systemInstruction": {
                "parts": [{"text": request.system_prompt}]
            },
            "generationConfig": {
                "maxOutputTokens": request.max_tokens,
                "temperature": request.temperature,
            }
        });

        let resp = self.client.post(&url)
            .header("content-type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(crate::NikiError::LlmProvider { provider: "google".into(), message: format!("HTTP {}: {}", status, body) }.into());
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
                                    if let Some(candidates) = json["candidates"].as_array() {
                                        if let Some(candidate) = candidates.get(0) {
                                            if let Some(parts) = candidate["content"]["parts"].as_array() {
                                                if let Some(part) = parts.get(0) {
                                                    if let Some(text) = part["text"].as_str() {
                                                        if tx.send(Ok(text.to_string())).is_err() {
                                                            return;
                                                        }
                                                    }
                                                }
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
        "google"
    }
}
