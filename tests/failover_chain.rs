//! Phase 3.7 — failover chain correctness (wiremock).
//!
//! Primary 429 x N then fallback serves the request; assert the fallback's
//! usage is recorded and the schema honored when the failover wrapper
//! propagates `supports_structured_output`.

use niki::config::ProviderConfig;
use niki::llm::provider::{CompletionRequest, LlmProvider};
use std::collections::HashMap;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn openai_config(base_url: &str) -> ProviderConfig {
    ProviderConfig {
        api_key: Some("test-key".to_string()),
        base_url: Some(base_url.to_string()),
        default_model: "test-model".to_string(),
    }
}

fn anthropic_config(base_url: &str) -> ProviderConfig {
    ProviderConfig {
        api_key: Some("test-key".to_string()),
        base_url: Some(base_url.to_string()),
        default_model: "test-model".to_string(),
    }
}

#[tokio::test]
async fn failover_chain_falls_back_on_429() {
    let primary = MockServer::start().await;
    let fallback = MockServer::start().await;

    // Primary: 3 x 429 then would succeed, but we never get there because
    // the circuit breaker trips after 3 failures and the fallback serves.
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
            "error": {"message": "rate limited", "type": "rate_limit_error"}
        })))
        .mount(&primary)
        .await;

    // Fallback: serves a normal response (any path — OpenAI-compatible
    // endpoint is /v1/chat/completions, not /v1/messages).
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{
                "message": {
                    "content": "fallback served"
                }
            }],
            "usage": {"prompt_tokens": 20, "completion_tokens": 8}
        })))
        .mount(&fallback)
        .await;

    let mut configs = HashMap::new();
    configs.insert("anthropic".to_string(), anthropic_config(&primary.uri()));
    configs.insert("openai".to_string(), openai_config(&fallback.uri()));

    let provider =
        niki::llm::failover::FailoverProvider::new("anthropic", &["openai".to_string()], &configs)
            .unwrap();

    let request = CompletionRequest {
        model: "test-model".to_string(),
        system_prompt: "sys".to_string(),
        user_message: "hi".to_string(),
        max_tokens: 100,
        temperature: 0.0,
        json_schema: None,
        tools: None,
    };

    let resp = provider.complete(request).await.unwrap();
    assert!(
        resp.content.contains("fallback served"),
        "fallback must serve the request after primary 429s"
    );
    assert_eq!(resp.usage.input_tokens, 20);
    assert_eq!(resp.usage.output_tokens, 8);
}

#[tokio::test]
async fn failover_propagates_structured_output_capability() {
    // OpenAI supports structured output; FailoverProvider must report
    // the same capability so callers route through request_structured.
    let mut configs = HashMap::new();
    configs.insert("openai".to_string(), openai_config("http://localhost:9999"));
    configs.insert(
        "anthropic".to_string(),
        anthropic_config("http://localhost:9998"),
    );

    let provider =
        niki::llm::failover::FailoverProvider::new("openai", &["anthropic".to_string()], &configs)
            .unwrap();

    assert!(
        provider.supports_structured_output(),
        "FailoverProvider must propagate the primary's structured-output capability"
    );
    assert_eq!(provider.provider_name(), "openai");
}

#[tokio::test]
async fn failover_reports_false_when_primary_unsupported() {
    // Anthropic does not support structured output; FailoverProvider
    // must report false so callers fall back to plain complete().
    let mut configs = HashMap::new();
    configs.insert(
        "anthropic".to_string(),
        anthropic_config("http://localhost:9998"),
    );

    let provider = niki::llm::failover::FailoverProvider::new("anthropic", &[], &configs).unwrap();

    assert!(
        !provider.supports_structured_output(),
        "FailoverProvider must report false when the primary does not support structured output"
    );
}
