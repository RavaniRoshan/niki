use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;
use reqwest::Client;
use serde_json::json;
use crate::config::ProviderConfig;
use super::provider::{CompletionRequest, CompletionResponse, LlmProvider, TokenUsage};

pub struct OllamaProvider {
    config: ProviderConfig,
    client: Client,
}

impl OllamaProvider {
    pub fn new(config: &ProviderConfig) -> Result<Self> {
        Ok(Self {
            config: config.clone(),
            client: Client::new(),
        })
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let base_url = self.config.base_url.as_deref().unwrap_or("http://localhost:11434");
        let url = format!("{}/api/chat", base_url.trim_end_matches('/'));

        let payload = json!({
            "model": request.model,
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
            "stream": false,
            "options": {
                "temperature": request.temperature,
                "num_predict": request.max_tokens,
            }
        });

        let mut req = self.client.post(&url)
            .header("content-type", "application/json");
        
        if let Some(api_key) = &self.config.api_key {
            req = req.header("Authorization", format!("Bearer {}", api_key));
        }

        let resp = req.json(&payload).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(crate::NikiError::LlmProvider { provider: "ollama".into(), message: format!("HTTP {}: {}", status, body) }.into());
        }

        let data: serde_json::Value = resp.json().await?;
        let content = data["message"]["content"].as_str().unwrap_or("").to_string();
        
        // Ollama provides eval_count and prompt_eval_count
        let input_tokens = data["prompt_eval_count"].as_u64().unwrap_or(0) as u32;
        let output_tokens = data["eval_count"].as_u64().unwrap_or(0) as u32;

        Ok(CompletionResponse {
            content,
            model: request.model,
            usage: TokenUsage { input_tokens, output_tokens },
        })
    }

    async fn stream(&self, request: CompletionRequest) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let base_url = self.config.base_url.as_deref().unwrap_or("http://localhost:11434");
        let url = format!("{}/api/chat", base_url.trim_end_matches('/'));

        let payload = json!({
            "model": request.model,
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
            "options": {
                "temperature": request.temperature,
                "num_predict": request.max_tokens,
            }
        });

        let mut req = self.client.post(&url)
            .header("content-type", "application/json");
        
        if let Some(api_key) = &self.config.api_key {
            req = req.header("Authorization", format!("Bearer {}", api_key));
        }

        let resp = req.json(&payload).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(crate::NikiError::LlmProvider { provider: "ollama".into(), message: format!("HTTP {}: {}", status, body) }.into());
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
                            if line.is_empty() {
                                continue;
                            }
                            
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                                if let Some(text) = json["message"]["content"].as_str() {
                                    if !text.is_empty() {
                                        if tx.send(Ok(text.to_string())).is_err() {
                                            return;
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
        "ollama"
    }
}
