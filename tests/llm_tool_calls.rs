//! Phase 3.1 — tool serialization + tool-call parsing (wiremock).
//!
//! A wiremock upstream returning one tool call must yield a non-empty
//! `CompletionResponse.tool_calls` for the OpenAI-compatible and Anthropic
//! paths. Request matchers prove the capped tool specs were serialized.

use niki::config::ProviderConfig;
use niki::llm::provider::{CompletionRequest, LlmProvider, ToolSpec};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn tool_request(model: &str) -> CompletionRequest {
    CompletionRequest {
        model: model.to_string(),
        system_prompt: "sys".to_string(),
        user_message: "use the tool".to_string(),
        max_tokens: 128,
        temperature: 0.0,
        json_schema: None,
        tools: Some(vec![ToolSpec {
            name: "read".to_string(),
            description: "Read a file".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {"path": {"type": "string"}}
            }),
        }]),
        reasoning_effort: None,
    }
}

fn test_config(base_url: &str) -> ProviderConfig {
    ProviderConfig {
        api_key: Some("test-key".to_string()),
        base_url: Some(base_url.to_string()),
        default_model: "test-model".to_string(),
    }
}

#[tokio::test]
async fn openai_tool_call_roundtrip() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains("tools"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{
                "message": {
                    "content": "",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "read",
                            "arguments": "{\"path\":\"src/main.rs\"}"
                        }
                    }]
                }
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5}
        })))
        .mount(&server)
        .await;

    let provider = niki::llm::openai::OpenAiProvider::new(&test_config(&server.uri())).unwrap();
    let resp = provider.complete(tool_request("test-model")).await.unwrap();
    assert_eq!(resp.tool_calls.len(), 1, "expected one tool call");
    assert_eq!(resp.tool_calls[0].name, "read");
    assert_eq!(resp.tool_calls[0].id, "call_1");
    assert_eq!(
        resp.tool_calls[0].arguments["path"],
        serde_json::json!("src/main.rs")
    );
}

#[tokio::test]
async fn anthropic_tool_use_roundtrip() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(body_string_contains("tools"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [
                {"type": "text", "text": "Using the tool."},
                {"type": "tool_use", "id": "toolu_1", "name": "read",
                 "input": {"path": "src/main.rs"}}
            ],
            "usage": {"input_tokens": 10, "output_tokens": 5}
        })))
        .mount(&server)
        .await;

    let provider =
        niki::llm::anthropic::AnthropicProvider::new(&test_config(&server.uri())).unwrap();
    let resp = provider.complete(tool_request("test-model")).await.unwrap();
    assert_eq!(resp.tool_calls.len(), 1, "expected one tool_use block");
    assert_eq!(resp.tool_calls[0].name, "read");
    assert_eq!(resp.tool_calls[0].id, "toolu_1");
    assert_eq!(
        resp.tool_calls[0].arguments["path"],
        serde_json::json!("src/main.rs")
    );
    assert!(resp.content.contains("Using the tool."));
}

#[test]
fn tool_spec_caps_bound_serialization() {
    // 20 tools in → 16 max out; oversized params collapse to `{type: object}`.
    let tools: Vec<ToolSpec> = (0..20)
        .map(|i| ToolSpec {
            name: format!("tool_{i}"),
            description: "d".repeat(5000),
            parameters: serde_json::json!({"type": "object", "blob": "x".repeat(9000)}),
        })
        .collect();
    let capped = niki::llm::provider::capped_tool_specs(&tools);
    assert_eq!(capped.len(), 16);
    assert!(capped[0].description.len() <= 2000);
    assert_eq!(capped[0].parameters, serde_json::json!({"type": "object"}));
}

/// Every provider's word for "I was cut off" must be recognised.
///
/// Three providers name the same condition three different ways, and the
/// symptom of missing one is always the same and never mentions truncation: a
/// half-written artifact is reported as a malformed one, and a user is sent to
/// blame a model for a token limit. Ollama's `length` was found by measurement
/// (probed against the running server); the other two are read off the
/// protocols, so they are pinned here rather than trusted.
#[test]
fn every_providers_word_for_truncation_is_recognised() {
    for (provider, reason) in [
        ("ollama", "length"),
        ("anthropic", "max_tokens"),
        ("openai", "length"),
        ("google", "MAX_TOKENS"),
        ("google", "max_tokens"),
    ] {
        assert!(
            niki::runtime::tools::was_truncated(Some(reason)),
            "{provider} reports truncation as {reason:?} and we do not recognise it"
        );
    }

    // And the reasons that are *not* truncation, so the guard cannot fire on
    // a normal finish or on a refusal.
    for reason in [
        "stop",
        "end_turn",
        "tool_use",
        "STOP",
        "content_filter",
        "error",
    ] {
        assert!(
            !niki::runtime::tools::was_truncated(Some(reason)),
            "{reason:?} is not truncation and must not be treated as one"
        );
    }

    // No reason at all is not evidence of anything.
    assert!(!niki::runtime::tools::was_truncated(None));
}
