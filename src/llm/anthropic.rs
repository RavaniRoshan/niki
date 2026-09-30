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
        // Presence check only — the key itself is read later, at request
        // time, out of the stored `config`. Cloning the secret to throw the
        // copy away was pointless work, and it is what CodeQL's
        // cleartext-logging rule has been flagging across every provider since
        // August: a cloned credential on a line the taint analysis believes
        // reaches a log sink. Borrowing removes the clone and the alert.
        if config.api_key.is_none() {
            return Err(super::provider::missing_key_error("anthropic"));
        }
        Ok(Self {
            config: config.clone(),
            client: super::provider::http_client()?,
        })
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| super::provider::missing_key_error("anthropic"))?;
        let url = anthropic_endpoint(
            self.config
                .base_url
                .as_deref()
                .unwrap_or("https://api.anthropic.com"),
        );

        let messages: Vec<serde_json::Value> = super::provider::message_chain(&request)
            .iter()
            .map(|t| json!({ "role": t.role, "content": t.content }))
            .collect();
        let mut payload = json!({
            "model": request.model,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "system": request.system_prompt,
            "messages": messages
        });

        // Native tool calling (Phase 3.1): Anthropic `tool_use` blocks.
        if let Some(tools) = request.tools.as_deref()
            && !tools.is_empty()
        {
            let specs = super::provider::capped_tool_specs(tools);
            let anthropic_tools: Vec<serde_json::Value> = specs
                .iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "input_schema": t.parameters,
                    })
                })
                .collect();
            payload["tools"] = serde_json::Value::Array(anthropic_tools);
        }

        // Build the request once; send_request rebuilds it on each retry attempt
        // (RequestBuilder is Clone). Retries on 429/5xx; 120s timeout on the shared
        // client bounds a hung call. See research report S12.
        let req = self
            .client
            .post(url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&payload);
        let resp =
            super::provider::send_request("anthropic complete", || req.try_clone().unwrap().send())
                .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(crate::NikiError::LlmProvider {
                provider: "anthropic".into(),
                message: format!("HTTP {}: {}", status, redact_secrets(&body)),
            }
            .into());
        }

        let data: serde_json::Value = resp.json().await?;
        // Concatenate all text blocks; tool_use blocks are parsed separately.
        let mut content = String::new();
        let mut tool_calls = Vec::new();
        if let Some(blocks) = data["content"].as_array() {
            for block in blocks {
                match block["type"].as_str().unwrap_or("") {
                    "text" => {
                        if let Some(text) = block["text"].as_str() {
                            content.push_str(text);
                        }
                    }
                    "tool_use" => {
                        let name = block["name"].as_str().unwrap_or("").to_string();
                        if name.is_empty() {
                            continue;
                        }
                        tool_calls.push(super::provider::ToolCall {
                            id: block["id"].as_str().unwrap_or("").to_string(),
                            name,
                            arguments: super::provider::capped_tool_arguments(&block["input"]),
                        });
                    }
                    _ => {}
                }
            }
        }

        let input_tokens = data["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32;
        let output_tokens = data["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32;
        // Prompt-cache hits price below input rate; reasoning effort is billed
        // as output tokens by Anthropic, so it stays inside output_tokens.
        let cached_input_tokens =
            crate::llm::json_path_u32(&data, &["usage", "cache_creation_input_tokens"])
                + crate::llm::json_path_u32(&data, &["usage", "cache_read_input_tokens"]);

        Ok(CompletionResponse {
            finish_reason: data["stop_reason"].as_str().map(|s| s.to_string()),
            content,
            model: request.model,
            usage: TokenUsage {
                input_tokens,
                output_tokens,
                cached_input_tokens,
                ..Default::default()
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
            .ok_or_else(|| super::provider::missing_key_error("anthropic"))?;
        let url = anthropic_endpoint(
            self.config
                .base_url
                .as_deref()
                .unwrap_or("https://api.anthropic.com"),
        );

        let messages: Vec<serde_json::Value> = super::provider::message_chain(&request)
            .iter()
            .map(|t| json!({ "role": t.role, "content": t.content }))
            .collect();
        let payload = json!({
            "model": request.model,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "system": request.system_prompt,
            "messages": messages,
            "stream": true
        });

        // Through `send_request`, like `complete()`.
        //
        // This called `.send()` directly, so the streaming path got **no HTTP
        // retry at all**: one 429 or one 503 from Anthropic killed the stream
        // on its first attempt, while the identical non-streaming call four
        // lines above retries. The agent-level matcher catches 429 and 503 but
        // not 500 or 502, so those were terminal too.
        //
        // Retrying is safe for a stream because nothing has been yielded to
        // the caller yet: `send_request` returns once the *response headers*
        // are in, and the body is read afterwards. A retry after the first
        // byte would duplicate output, so the bound is deliberately here and
        // not around the body read.
        let req = self
            .client
            .post(url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&payload);
        let resp = super::provider::send_request("anthropic stream", || {
            req.try_clone().expect("a RequestBuilder clones").send()
        })
        .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(crate::NikiError::LlmProvider {
                provider: "anthropic".into(),
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
                                    if json["type"] == "content_block_delta" {
                                        if let Some(text) = json["delta"]["text"].as_str()
                                            && tx
                                                .send(Ok(StreamChunk::Text(text.to_string())))
                                                .is_err()
                                        {
                                            return;
                                        }
                                    } else if json["type"] == "message_start" {
                                        // input_tokens are known up front
                                        if let Some(input) =
                                            json["message"]["usage"]["input_tokens"].as_u64()
                                            && tx
                                                .send(Ok(StreamChunk::Usage(TokenUsage {
                                                    input_tokens: input as u32,
                                                    output_tokens: 0,
                                                    cached_input_tokens: crate::llm::json_path_u32(
                                                        &json,
                                                        &[
                                                            "message",
                                                            "usage",
                                                            "cache_creation_input_tokens",
                                                        ],
                                                    )
                                                        + crate::llm::json_path_u32(
                                                            &json,
                                                            &[
                                                                "message",
                                                                "usage",
                                                                "cache_read_input_tokens",
                                                            ],
                                                        ),
                                                    ..Default::default()
                                                })))
                                                .is_err()
                                        {
                                            return;
                                        }
                                    } else if json["type"] == "message_delta" {
                                        // The stop reason, before the usage.
                                        //
                                        // `max_tokens` is Anthropic's name for
                                        // being cut off, and without this a
                                        // truncated response is
                                        // indistinguishable from a malformed
                                        // one — the same gap Ollama had, found
                                        // by measurement there and fixed here by
                                        // reading the protocol rather than
                                        // waiting for the same bug to show up.
                                        if let Some(reason) = json["delta"]["stop_reason"].as_str()
                                            && tx
                                                .send(Ok(StreamChunk::Finish {
                                                    reason: reason.to_string(),
                                                }))
                                                .is_err()
                                        {
                                            return;
                                        }
                                        // output_tokens (and possibly the final input_tokens) arrive here
                                        if let Some(output) =
                                            json["usage"]["output_tokens"].as_u64()
                                            && tx
                                                .send(Ok(StreamChunk::Usage(TokenUsage {
                                                    input_tokens: 0,
                                                    output_tokens: output as u32,
                                                    ..Default::default()
                                                })))
                                                .is_err()
                                        {
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

        Ok(Box::pin(
            tokio_stream::wrappers::UnboundedReceiverStream::new(rx),
        ))
    }

    /// The same resolution the request path uses, so this cannot report a URL
    /// the provider would not send to.
    fn endpoint(&self) -> String {
        anthropic_endpoint(
            self.config
                .base_url
                .as_deref()
                .unwrap_or("https://api.anthropic.com"),
        )
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
