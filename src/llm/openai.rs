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
    /// Provider slug from a base URL (for key-error hints and logging).
    fn name_from_base_url(base_url: Option<&str>) -> String {
        base_url
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
                } else if url.contains("opencode.ai/zen") {
                    Some("zen")
                } else if url.contains("api.kimi.com") {
                    Some("kimi")
                } else if url.contains("api.kilo.ai") {
                    Some("kilo")
                } else {
                    None
                }
            })
            .unwrap_or("openai")
            .to_string()
    }

    pub fn new(config: &ProviderConfig) -> Result<Self> {
        // Derive provider name first so the missing-key error names the exact
        // env var (OPENAI_API_KEY vs GROQ_API_KEY paint very different fixes).
        let provider_name = Self::name_from_base_url(config.base_url.as_deref());
        Self::new_named(config, &provider_name)
    }

    /// Construct with an explicit provider slug (used for named gateways whose
    /// base_url may be user-overridden, e.g. `zen`, `kimi`, `kilo`).
    pub fn new_named(config: &ProviderConfig, provider_name: &str) -> Result<Self> {
        // Presence check only — the key itself is read later, at request
        // time, out of the stored `config`. Cloning the secret to throw the
        // copy away was pointless work, and it is what CodeQL's
        // cleartext-logging rule has been flagging across every provider since
        // August: a cloned credential on a line the taint analysis believes
        // reaches a log sink. Borrowing removes the clone and the alert.
        if config.api_key.is_none() {
            return Err(super::provider::missing_key_error(provider_name));
        }
        Ok(Self {
            config: config.clone(),
            client: super::provider::http_client()?,
            provider_name: provider_name.to_string(),
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
            .ok_or_else(|| super::provider::missing_key_error(&self.provider_name))?;
        let url = openai_endpoint(self.base_url());

        let mut payload = json!({
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
        // Only when set. An OpenAI-compatible provider that does not know the
        // key may reject the whole request, so this is opt-in and never
        // inferred from a model name — the accepted range is a property of the
        // model, and guessing it turns a working run into a 400.
        if let Some(effort) = &request.reasoning_effort
            && !effort.is_empty()
        {
            payload["reasoning_effort"] = json!(effort);
        }

        // Native tool calling (Phase 3.1): serialize capped specs so the model
        // can emit `tool_calls`; absent/empty tools leave the payload unchanged.
        if let Some(tools) = request.tools.as_deref()
            && !tools.is_empty()
        {
            let specs = super::provider::capped_tool_specs(tools);
            let openai_tools: Vec<serde_json::Value> = specs
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
                .collect();
            payload["tools"] = serde_json::Value::Array(openai_tools);
            payload["tool_choice"] = json!("auto");
        }

        let req = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("content-type", "application/json")
            .json(&payload);
        let resp =
            super::provider::send_request("openai request", || req.try_clone().unwrap().send())
                .await?;

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
        // Every read below goes through `json_path*` rather than `Index`.
        // `data["usage"]` on a response with no `usage` key is `Null`, and the
        // next `["prompt_tokens"]` on that is a panic, not a default. An
        // OpenAI-compatible server is not obliged to send usage on every
        // response, and a provider that panics on one is a provider that takes
        // the run down with it.
        let content =
            crate::llm::json_path_str(&data, &["choices", "0", "message", "content"]).to_string();

        let input_tokens = crate::llm::json_path_u32(&data, &["usage", "prompt_tokens"]);
        let output_tokens = crate::llm::json_path_u32(&data, &["usage", "completion_tokens"]);
        let cached_input_tokens =
            crate::llm::json_path_u32(&data, &["usage", "prompt_tokens_details", "cached_tokens"]);
        let reasoning_tokens = crate::llm::json_path_u32(
            &data,
            &["usage", "completion_tokens_details", "reasoning_tokens"],
        );

        // Parse native tool calls (Phase 3.1), capping returned arguments.
        let mut tool_calls = Vec::new();
        if let Some(calls) = data["choices"][0]["message"]["tool_calls"].as_array() {
            for call in calls {
                let id = call["id"].as_str().unwrap_or("").to_string();
                let name = call["function"]["name"].as_str().unwrap_or("").to_string();
                if name.is_empty() {
                    continue;
                }
                let args_str = call["function"]["arguments"].as_str().unwrap_or("{}");
                let args: serde_json::Value =
                    serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));
                tool_calls.push(super::provider::ToolCall {
                    id,
                    name,
                    arguments: super::provider::capped_tool_arguments(&args),
                });
            }
        }

        Ok(CompletionResponse {
            finish_reason: data["choices"][0]["finish_reason"]
                .as_str()
                .map(|s| s.to_string()),
            content,
            model: request.model,
            usage: TokenUsage {
                input_tokens,
                output_tokens,
                cached_input_tokens,
                reasoning_tokens,
            },
            tool_calls,
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
            .ok_or_else(|| super::provider::missing_key_error(&self.provider_name))?;
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
        let resp =
            super::provider::send_request("openai request", || req.try_clone().unwrap().send())
                .await?;

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
                                    // The stop reason, when the chunk carries
                                    // one. `length` here is the same signal
                                    // Ollama sends as `done_reason` and
                                    // Anthropic as `stop_reason`, and without
                                    // it a truncated response is
                                    // indistinguishable from a malformed one.
                                    if let Some(reason) =
                                        json["choices"][0]["finish_reason"].as_str()
                                        && !reason.is_empty()
                                        && tx
                                            .send(Ok(StreamChunk::Finish {
                                                reason: reason.to_string(),
                                            }))
                                            .is_err()
                                    {
                                        return;
                                    }
                                    if json.get("usage").map(|u| u.is_object()).unwrap_or(false) {
                                        // Final usage chunk (choices is empty / absent).
                                        // `json["usage"]` here is safe — it is
                                        // guarded — but the reads inside are not,
                                        // because `prompt_tokens_details` is
                                        // optional and indexing a missing one
                                        // panics. The whole path is walked
                                        // instead.
                                        if tx
                                            .send(Ok(StreamChunk::Usage(TokenUsage {
                                                input_tokens: crate::llm::json_path_u32(
                                                    &json,
                                                    &["usage", "prompt_tokens"],
                                                ),
                                                output_tokens: crate::llm::json_path_u32(
                                                    &json,
                                                    &["usage", "completion_tokens"],
                                                ),
                                                cached_input_tokens: crate::llm::json_path_u32(
                                                    &json,
                                                    &[
                                                        "usage",
                                                        "prompt_tokens_details",
                                                        "cached_tokens",
                                                    ],
                                                ),
                                                reasoning_tokens: crate::llm::json_path_u32(
                                                    &json,
                                                    &[
                                                        "usage",
                                                        "completion_tokens_details",
                                                        "reasoning_tokens",
                                                    ],
                                                ),
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
            .ok_or_else(|| super::provider::missing_key_error(&self.provider_name))?;
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
        let resp =
            super::provider::send_request("openai request", || req.try_clone().unwrap().send())
                .await?;

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
        // Every read below goes through `json_path*` rather than `Index`.
        // `data["usage"]` on a response with no `usage` key is `Null`, and the
        // next `["prompt_tokens"]` on that is a panic, not a default. An
        // OpenAI-compatible server is not obliged to send usage on every
        // response, and a provider that panics on one is a provider that takes
        // the run down with it.
        let content =
            crate::llm::json_path_str(&data, &["choices", "0", "message", "content"]).to_string();

        let input_tokens = crate::llm::json_path_u32(&data, &["usage", "prompt_tokens"]);
        let output_tokens = crate::llm::json_path_u32(&data, &["usage", "completion_tokens"]);
        let cached_input_tokens =
            crate::llm::json_path_u32(&data, &["usage", "prompt_tokens_details", "cached_tokens"]);
        let reasoning_tokens = crate::llm::json_path_u32(
            &data,
            &["usage", "completion_tokens_details", "reasoning_tokens"],
        );

        Ok(CompletionResponse {
            finish_reason: data["choices"][0]["finish_reason"]
                .as_str()
                .map(|s| s.to_string()),
            content,
            model: request.model,
            usage: TokenUsage {
                input_tokens,
                output_tokens,
                cached_input_tokens,
                reasoning_tokens,
            },
            tool_calls: Vec::new(),
        })
    }

    async fn transcribe(&self, audio: &[u8], language: Option<&str>) -> Result<String> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| super::provider::missing_key_error(&self.provider_name))?;
        // OpenAI-compatible STT endpoint: <base>/audio/transcriptions
        let base = self
            .config
            .base_url
            .as_deref()
            .unwrap_or("https://api.openai.com/v1");
        let base = base.trim_end_matches('/');
        let url = if base.ends_with("/audio/transcriptions") {
            base.to_string()
        } else {
            format!("{base}/audio/transcriptions")
        };

        let mut form = reqwest::multipart::Form::new()
            .part(
                "file",
                reqwest::multipart::Part::bytes(audio.to_vec())
                    .mime_str("audio/wav")
                    .unwrap(),
            )
            .text("model", "whisper-1");
        if let Some(lang) = language {
            form = form.text("language", lang.to_string());
        }

        let resp = crate::llm::provider::http_client()?
            .post(&url)
            .bearer_auth(api_key)
            .multipart(form)
            .send()
            .await?;
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("STT request failed: {body}"));
        }
        let data: serde_json::Value = resp.json().await?;
        let text = data["text"].as_str().unwrap_or("").to_string();
        Ok(text)
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
