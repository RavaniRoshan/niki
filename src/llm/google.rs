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

pub struct GoogleProvider {
    config: ProviderConfig,
    client: Client,
}

impl GoogleProvider {
    pub fn new(config: &ProviderConfig) -> Result<Self> {
        // Presence check only — the key itself is read later, at request
        // time, out of the stored `config`. Cloning the secret to throw the
        // copy away was pointless work, and it is what CodeQL's
        // cleartext-logging rule has been flagging across every provider since
        // August: a cloned credential on a line the taint analysis believes
        // reaches a log sink. Borrowing removes the clone and the alert.
        if config.api_key.is_none() {
            return Err(super::provider::missing_key_error("google"));
        }
        Ok(Self {
            config: config.clone(),
            client: super::provider::http_client()?,
        })
    }
}

/// Resolve a base URL to a Google generative-language method endpoint.
///
/// `base_url` was ignored entirely until now: the URL was built from a
/// hardcoded `https://generativelanguage.googleapis.com`, so a user behind a
/// corporate egress proxy or an AI gateway could not reach Google at all, and
/// — the reason this surfaced — the provider was the one shipped implementation
/// whose wire format no test could exercise. Every other provider honours
/// `base_url`, and matching them is what makes this testable.
///
/// A `base_url` that already names a method (or a `models/…` segment) is left
/// alone, so an explicit endpoint is never double-suffixed.
fn google_endpoint(base: Option<&str>, model: &str, method: &str) -> String {
    let Some(base) = base else {
        return format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:{method}");
    };
    let b = base.trim_end_matches('/');
    if b.contains(":generateContent") || b.contains(":streamGenerateContent") {
        return b.to_string();
    }
    // `…/v1beta` and `…/v1beta/models` are both reasonable things to configure.
    let stem = b.strip_suffix("/models").unwrap_or(b);
    if stem.ends_with("/v1beta") || stem.ends_with("/v1") {
        format!("{stem}/models/{model}:{method}")
    } else {
        format!("{stem}/v1beta/models/{model}:{method}")
    }
}

#[async_trait]
impl LlmProvider for GoogleProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| super::provider::missing_key_error("google"))?;

        let url = google_endpoint(
            self.config.base_url.as_deref(),
            &request.model,
            "generateContent",
        );

        let payload = json!({
            "contents": super::provider::message_chain(&request)
                .iter()
                .map(|t| json!({
                    // Google names the assistant role "model", not "assistant".
                    "role": if t.role == "assistant" { "model" } else { "user" },
                    "parts": [{"text": t.content}]
                }))
                .collect::<Vec<serde_json::Value>>(),
            "systemInstruction": {
                "parts": [{"text": request.system_prompt}]
            },
            "generationConfig": {
                "maxOutputTokens": request.max_tokens,
                "temperature": request.temperature,
            }
        });

        // Build the request once; send_request rebuilds it on each retry attempt
        // (RequestBuilder::try_clone). Retries on 429/5xx; 120s timeout on the shared
        // client bounds a hung call. See research report S12.
        let req = self
            .client
            .post(&url)
            .header("x-goog-api-key", api_key)
            .header("content-type", "application/json")
            .json(&payload);
        let resp =
            super::provider::send_request("google complete", || req.try_clone().unwrap().send())
                .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(crate::NikiError::LlmProvider {
                provider: "google".into(),
                message: format!("HTTP {}: {}", status, redact_secrets(&body)),
            }
            .into());
        }

        let data: serde_json::Value = resp.json().await?;
        let content =
            crate::llm::json_path_str(&data, &["candidates", "0", "content", "parts", "0", "text"])
                .to_string();

        let input_tokens = crate::llm::json_path_u32(&data, &["usageMetadata", "promptTokenCount"]);
        let output_tokens =
            crate::llm::json_path_u32(&data, &["usageMetadata", "candidatesTokenCount"]);
        let cached_input_tokens =
            crate::llm::json_path_u32(&data, &["usageMetadata", "cachedContentTokenCount"]);
        let reasoning_tokens =
            crate::llm::json_path_u32(&data, &["usageMetadata", "thoughtsTokenCount"]);

        Ok(CompletionResponse {
            finish_reason: data["candidates"][0]["finishReason"]
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
            // Phase 3.1: no native tool support on this provider —
            // `tools` is ignored and `tool_calls` stays empty by design.
            tool_calls: Vec::new(),
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
            .ok_or_else(|| super::provider::missing_key_error("google"))?;

        let mut url = google_endpoint(
            self.config.base_url.as_deref(),
            &request.model,
            "streamGenerateContent",
        );
        url.push_str("?alt=sse");

        let payload = json!({
            "contents": super::provider::message_chain(&request)
                .iter()
                .map(|t| json!({
                    // Google names the assistant role "model", not "assistant".
                    "role": if t.role == "assistant" { "model" } else { "user" },
                    "parts": [{"text": t.content}]
                }))
                .collect::<Vec<serde_json::Value>>(),
            "systemInstruction": {
                "parts": [{"text": request.system_prompt}]
            },
            "generationConfig": {
                "maxOutputTokens": request.max_tokens,
                "temperature": request.temperature,
            }
        });

        let req = self
            .client
            .post(&url)
            .header("x-goog-api-key", api_key)
            .header("content-type", "application/json")
            .json(&payload);
        let resp =
            super::provider::send_request("google stream", || req.try_clone().unwrap().send())
                .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(crate::NikiError::LlmProvider {
                provider: "google".into(),
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

                            if !handle_sse_line(&line, &tx) {
                                return;
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
        "google"
    }
}

/// Apply one SSE line from a `streamGenerateContent` response.
///
/// Returns `false` when the receiver is gone and the reader should stop.
///
/// Extracted from the spawned reader so it can be tested without an HTTP
/// server: the whole of Google's streaming behaviour — which chunks it emits,
/// what it drops — lived inside a `tokio::spawn` closure over a live response,
/// which is why this file had no tests at all. The first thing a test then
/// found is below, in `a_truncated_response_keeps_its_stop_reason`.
pub fn handle_sse_line(
    line: &str,
    tx: &tokio::sync::mpsc::UnboundedSender<Result<StreamChunk>>,
) -> bool {
    let line = line.trim();
    if let Some(data) = line.strip_prefix("data: ") {
        if data == "[DONE]" {
            return true;
        }
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
            if let Some(usage) = json["usageMetadata"].as_object()
                && tx
                    .send(Ok(StreamChunk::Usage(TokenUsage {
                        input_tokens: usage["promptTokenCount"].as_u64().unwrap_or(0) as u32,
                        output_tokens: usage["candidatesTokenCount"].as_u64().unwrap_or(0) as u32,
                        cached_input_tokens: usage["cachedContentTokenCount"].as_u64().unwrap_or(0)
                            as u32,
                        reasoning_tokens: usage["thoughtsTokenCount"].as_u64().unwrap_or(0) as u32,
                    })))
                    .is_err()
            {
                return false;
            }
            if let Some(candidates) = json["candidates"].as_array()
                && let Some(candidate) = candidates.first()
            {
                if let Some(parts) = candidate["content"]["parts"].as_array()
                    && let Some(part) = parts.first()
                    && let Some(text) = part["text"].as_str()
                    && tx.send(Ok(StreamChunk::Text(text.to_string()))).is_err()
                {
                    return false;
                }
                // The stop reason, which the
                // non-streaming path above already
                // reads and this one dropped.
                //
                // Without it a Google response cut off
                // at the token limit is
                // indistinguishable from a malformed
                // one, gets fed to the JSON repairer,
                // and is reported as "did not satisfy
                // the artifact requirements" — sending
                // a user to blame the model for
                // something the token limit did. The
                // enum exists to prevent exactly that.
                if let Some(reason) = candidate["finishReason"].as_str()
                    && !reason.is_empty()
                    && tx
                        .send(Ok(StreamChunk::Finish {
                            reason: reason.to_string(),
                        }))
                        .is_err()
                {
                    return false;
                }
            }
        }
    }
    true
}
