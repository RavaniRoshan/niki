use niki::config::ProviderConfig;
use niki::llm::provider::{CompletionRequest, LlmProvider, create_provider};
use serde_json::json;

#[test]
fn test_openai_provider_supports_structured_output() {
    let config = ProviderConfig {
        api_key: Some("test-key".to_string()),
        base_url: None,
        ..Default::default()
    };
    let provider = create_provider("openai", &config).unwrap();
    assert!(provider.supports_structured_output());
    assert_eq!(provider.provider_name(), "openai");
}

#[test]
fn test_mock_provider_supports_structured_output() {
    let dir = tempfile::tempdir().unwrap();
    let script_path = dir.path().join("mock.json");
    let script = serde_json::json!({
        "models": {
            "test-model": {
                "responses": [
                    {"text": "{\"name\": \"test\"}", "input_tokens": 10, "output_tokens": 5}
                ]
            }
        }
    });
    std::fs::write(&script_path, serde_json::to_string(&script).unwrap()).unwrap();

    let config = ProviderConfig {
        api_key: None,
        base_url: Some(script_path.to_str().unwrap().to_string()),
        ..Default::default()
    };
    let provider = create_provider("mock", &config).unwrap();
    assert!(provider.supports_structured_output());
    assert_eq!(provider.provider_name(), "mock");
}

#[test]
fn test_anthropic_provider_no_structured_output() {
    let config = ProviderConfig {
        api_key: Some("test-key".to_string()),
        base_url: None,
        ..Default::default()
    };
    let provider = create_provider("anthropic", &config).unwrap();
    assert!(!provider.supports_structured_output());
}

#[test]
fn test_ollama_provider_no_structured_output() {
    let config = ProviderConfig {
        api_key: None,
        base_url: Some("http://localhost:11434".to_string()),
        ..Default::default()
    };
    let provider = create_provider("ollama", &config).unwrap();
    assert!(!provider.supports_structured_output());
}

#[test]
fn test_request_structured_default_fallback() {
    let config = ProviderConfig {
        api_key: Some("test-key".to_string()),
        base_url: None,
        ..Default::default()
    };
    let provider = create_provider("anthropic", &config).unwrap();
    let schema = json!({"type": "object", "properties": {"name": {"type": "string"}}});

    let request = CompletionRequest {
        model: "test".to_string(),
        system_prompt: "test".to_string(),
        user_message: "test".to_string(),
        max_tokens: 100,
        temperature: 0.0,
        json_schema: None,
    };

    // Just verify the method compiles and has the right type
    let _ = provider.request_structured(request, &schema);
}

#[test]
fn test_create_provider_unknown() {
    let config = ProviderConfig::default();
    let result = create_provider("nonexistent", &config);
    assert!(result.is_err());
}
