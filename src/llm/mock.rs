use crate::llm::provider::{
    CompletionRequest, CompletionResponse, LlmProvider, StreamChunk, TokenUsage,
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use futures::Stream;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, serde::Deserialize)]
struct MockResponse {
    text: Option<String>,
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
    error: Option<MockError>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct MockError {
    kind: String,
    message: String,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
struct ModelResponses {
    #[serde(default)]
    responses: Vec<MockResponse>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
struct MockScript {
    #[serde(default)]
    models: HashMap<String, ModelResponses>,
}

#[allow(dead_code)]
struct ModelCursor {
    index: usize,
    retry_count: u32,
}

pub struct MockProvider {
    #[allow(dead_code)]
    script_path: PathBuf,
    script: MockScript,
    cursors: Arc<Mutex<HashMap<String, ModelCursor>>>,
}

impl MockProvider {
    pub fn new(base_url: Option<&str>) -> Result<Self> {
        let path = base_url
            .ok_or_else(|| anyhow!("mock provider requires base_url as a script file path"))?;
        let path = PathBuf::from(path);
        let content = fs::read_to_string(&path)
            .map_err(|e| anyhow!("Failed to read mock script {}: {}", path.display(), e))?;
        let script: MockScript = serde_json::from_str(&content)
            .map_err(|e| anyhow!("Failed to parse mock script {}: {}", path.display(), e))?;
        Ok(Self {
            script_path: path,
            script,
            cursors: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    fn next_response(&self, model: &str) -> Result<MockResponse> {
        let mut guard = self.cursors.lock().unwrap();
        let cursor = guard
            .entry(model.to_string())
            .or_insert_with(|| ModelCursor {
                index: 0,
                retry_count: 0,
            });

        let responses = &self.script.models.get(model).ok_or_else(|| {
            anyhow!(
                "No mock responses for model '{}'. Script has models: {:?}",
                model,
                self.script.models.keys().collect::<Vec<_>>()
            )
        })?;

        if responses.responses.is_empty() {
            return Err(anyhow!(
                "No mock responses configured for model '{}'",
                model
            ));
        }

        let response = responses.responses[cursor.index % responses.responses.len()].clone();
        cursor.index += 1;
        Ok(response)
    }
}

#[async_trait]
impl LlmProvider for MockProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let response = self.next_response(&request.model)?;
        if let Some(err) = response.error {
            let msg = err.message.clone();
            match err.kind.as_str() {
                "transient" => Err(anyhow!("transient error: {}", msg)),
                "fatal" => Err(anyhow!("fatal error: {}", msg)),
                "invalid_json" => {
                    return Ok(CompletionResponse {
                        content: msg,
                        model: request.model.clone(),
                        usage: TokenUsage {
                            input_tokens: response.input_tokens.unwrap_or(0),
                            output_tokens: response.output_tokens.unwrap_or(0),
                        },
                    });
                }
                _ => Err(anyhow!("unknown error kind '{}': {}", err.kind, msg)),
            }
        } else {
            Ok(CompletionResponse {
                content: response.text.unwrap_or_default(),
                model: request.model.clone(),
                usage: TokenUsage {
                    input_tokens: response.input_tokens.unwrap_or(0),
                    output_tokens: response.output_tokens.unwrap_or(0),
                },
            })
        }
    }

    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>> {
        let response = self.next_response(&request.model)?;

        if let Some(err) = response.error {
            let msg = err.message.clone();
            match err.kind.as_str() {
                "transient" => {
                    return Err(anyhow!("transient error: {}", msg));
                }
                "fatal" => {
                    return Err(anyhow!("fatal error: {}", msg));
                }
                "invalid_json" => {
                    let usage = TokenUsage {
                        input_tokens: response.input_tokens.unwrap_or(0),
                        output_tokens: response.output_tokens.unwrap_or(0),
                    };
                    let chunks: Vec<Result<StreamChunk>> =
                        vec![Ok(StreamChunk::Text(msg)), Ok(StreamChunk::Usage(usage))];
                    return Ok(Box::pin(futures::stream::iter(chunks)));
                }
                _ => {
                    return Err(anyhow!("unknown error kind '{}': {}", err.kind, msg));
                }
            }
        }

        let text = response.text.unwrap_or_default();
        let usage = TokenUsage {
            input_tokens: response.input_tokens.unwrap_or(0),
            output_tokens: response.output_tokens.unwrap_or(0),
        };

        let mut chunks: Vec<Result<StreamChunk>> = Vec::new();
        for line in text.lines() {
            chunks.push(Ok(StreamChunk::Text(format!("{}\n", line))));
        }
        if !text.ends_with('\n') && !text.is_empty() && text.contains('\n') {
            // Already handled by lines() which skips trailing newline
        }
        chunks.push(Ok(StreamChunk::Usage(usage)));

        Ok(Box::pin(futures::stream::iter(chunks)))
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    fn supports_structured_output(&self) -> bool {
        true
    }

    async fn request_structured(
        &self,
        request: CompletionRequest,
        _schema: &serde_json::Value,
    ) -> Result<CompletionResponse> {
        // Mock provider: just delegate to complete(). The mock response content
        // is pre-scripted; the caller is responsible for configuring valid JSON.
        self.complete(request).await
    }
}
