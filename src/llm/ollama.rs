use super::provider::{
    CompletionRequest, CompletionResponse, LlmProvider, StreamChunk, TokenUsage, redact_secrets,
};
use crate::config::ProviderConfig;
use anyhow::Result;
use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use serde_json::json;
use std::pin::Pin;

pub struct OllamaProvider {
    config: ProviderConfig,
    client: Client,
}

impl OllamaProvider {
    pub fn new(config: &ProviderConfig) -> Result<Self> {
        Ok(Self {
            config: config.clone(),
            client: super::provider::http_client()?,
        })
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let base_url = self
            .config
            .base_url
            .as_deref()
            .unwrap_or("http://localhost:11434");
        let url = format!("{}/api/chat", base_url.trim_end_matches('/'));

        // Ollama's `/api/chat` takes tools in the OpenAI shape. Sending them
        // conditionally keeps the request byte-identical to before for the
        // callers that pass none, which is every stage that is not the Coder
        // loop.
        let mut payload = json!({
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
        if let Some(specs) = &request.tools
            && !specs.is_empty()
        {
            payload["tools"] = serde_json::Value::Array(
                specs
                    .iter()
                    .map(|t| {
                        json!({
                            "type": "function",
                            "function": {
                                "name": t.name,
                                "description": t.description,
                                "parameters": t.parameters,
                            }
                        })
                    })
                    .collect(),
            );
        }

        let mut request = self
            .client
            .post(&url)
            .header("content-type", "application/json");

        if let Some(api_key) = &self.config.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }

        request = request.json(&payload);
        let resp =
            super::provider::send_request("ollama request", || request.try_clone().unwrap().send())
                .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(crate::NikiError::LlmProvider {
                provider: "ollama".into(),
                message: format!("HTTP {}: {}", status, redact_secrets(&body)),
            }
            .into());
        }

        let data: serde_json::Value = resp.json().await?;
        let content = crate::llm::json_path_str(&data, &["message", "content"]).to_string();

        // Ollama provides eval_count and prompt_eval_count
        let input_tokens = crate::llm::json_path_u32(&data, &["prompt_eval_count"]);
        let output_tokens = crate::llm::json_path_u32(&data, &["eval_count"]);

        Ok(CompletionResponse {
            finish_reason: data["done_reason"].as_str().map(|s| s.to_string()),
            content,
            model: self.config.default_model.clone(),
            usage: TokenUsage {
                input_tokens,
                output_tokens,
                ..Default::default()
            },
            tool_calls: parse_tool_calls(&data),
        })
    }

    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>> {
        let base_url = self
            .config
            .base_url
            .as_deref()
            .unwrap_or("http://localhost:11434");
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

        let mut request = self
            .client
            .post(&url)
            .header("content-type", "application/json");

        if let Some(api_key) = &self.config.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }

        request = request.json(&payload);
        let resp =
            super::provider::send_request("ollama request", || request.try_clone().unwrap().send())
                .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(crate::NikiError::LlmProvider {
                provider: "ollama".into(),
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
                            if line.is_empty() {
                                continue;
                            }

                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                                if json["done"].as_bool().unwrap_or(false) {
                                    // Final chunk carries real token counts.
                                    if tx
                                        .send(Ok(StreamChunk::Usage(TokenUsage {
                                            input_tokens: json["prompt_eval_count"]
                                                .as_u64()
                                                .unwrap_or(0)
                                                as u32,
                                            output_tokens: json["eval_count"].as_u64().unwrap_or(0)
                                                as u32,
                                            ..Default::default()
                                        })))
                                        .is_err()
                                    {
                                        return;
                                    }
                                } else if let Some(text) = json["message"]["content"].as_str()
                                    && !text.is_empty()
                                    && tx.send(Ok(StreamChunk::Text(text.to_string()))).is_err()
                                {
                                    return;
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
        "ollama"
    }
}

/// Read `message.tool_calls` out of an Ollama response.
///
/// This provider previously returned `tool_calls: Vec::new()` unconditionally,
/// with a comment saying Ollama had no native tool support. It does — and this
/// is the provider the README's zero-setup path uses (`ollama pull
/// qwen2.5-coder:3b`), so the Coder's tool loop could never have run: the model
/// was never shown the tools, and anything it did return was thrown away. The
/// loop then always fell back to a single call, and the stage failed
/// intermittently for reasons that had nothing to do with the model.
///
/// A malformed entry is skipped rather than fatal: one bad tool call should not
/// discard the others the model got right.
pub fn parse_tool_calls(data: &serde_json::Value) -> Vec<crate::llm::provider::ToolCall> {
    let Some(calls) = crate::llm::json_path(data, &["message", "tool_calls"]) else {
        return Vec::new();
    };
    let Some(calls) = calls.as_array() else {
        return Vec::new();
    };
    calls
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let name = crate::llm::json_path_str(c, &["function", "name"]);
            if name.is_empty() {
                return None;
            }
            // `arguments` is an object on current Ollama, but older builds sent
            // it as a JSON *string*. Accept both rather than silently dropping
            // every call on an older server.
            let arguments = match crate::llm::json_path(c, &["function", "arguments"]) {
                Some(v) if v.is_object() => v.clone(),
                Some(serde_json::Value::String(s)) => {
                    serde_json::from_str(s).unwrap_or_else(|_| serde_json::json!({}))
                }
                _ => serde_json::json!({}),
            };
            Some(crate::llm::provider::ToolCall {
                id: format!("ollama-{i}"),
                name: name.to_string(),
                arguments,
            })
        })
        .collect()
}
