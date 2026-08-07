mod common;

use niki::llm::mock::MockProvider;
use niki::llm::provider::{CompletionRequest, LlmProvider, StreamChunk};
use std::path::PathBuf;

use common::mock_llm::MockScriptBuilder;

fn make_mock_script_path(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join(".niki-mock-script.json")
}

fn mock_provider_with_script(script_json: &str) -> (tempfile::TempDir, MockProvider) {
    let dir = tempfile::TempDir::new().unwrap();
    let script_path = make_mock_script_path(&dir);
    std::fs::create_dir_all(script_path.parent().unwrap()).unwrap();
    std::fs::write(&script_path, script_json).unwrap();
    let base_url = script_path.to_string_lossy().to_string();
    let provider = MockProvider::new(Some(&base_url)).unwrap();
    (dir, provider)
}

fn single_response_json(text: &str, input: u32, output: u32) -> String {
    serde_json::json!({
        "models": {
            "mock-test": {
                "responses": [
                    { "text": text, "input_tokens": input, "output_tokens": output }
                ]
            }
        }
    })
    .to_string()
}

fn error_response_json(kind: &str, message: &str) -> String {
    serde_json::json!({
        "models": {
            "mock-test": {
                "responses": [
                    { "error": { "kind": kind, "message": message } }
                ]
            }
        }
    })
    .to_string()
}

fn make_request() -> CompletionRequest {
    CompletionRequest {
        model: "mock-test".to_string(),
        system_prompt: "System prompt".to_string(),
        user_message: "Do a task".to_string(),
        max_tokens: 8192,
        temperature: 0.2,
        json_schema: None,
    }
}

#[test]
fn mock_provider_creates_successfully() {
    let script = single_response_json("hello", 10, 5);
    let (_dir, provider) = mock_provider_with_script(&script);
    assert_eq!(provider.provider_name(), "mock");
}

#[tokio::test]
async fn mock_provider_complete_returns_text_and_usage() {
    let script = single_response_json("Hello from mock", 20, 10);
    let (_dir, provider) = mock_provider_with_script(&script);

    let resp = provider.complete(make_request()).await.unwrap();
    assert_eq!(resp.content, "Hello from mock");
    assert_eq!(resp.usage.input_tokens, 20);
    assert_eq!(resp.usage.output_tokens, 10);
    assert_eq!(resp.model, "mock-test");
}

#[tokio::test]
async fn mock_provider_complete_returns_error_when_configured() {
    let script = error_response_json("rate_limit", "Rate limit exceeded");
    let (_dir, provider) = mock_provider_with_script(&script);

    let result = provider.complete(make_request()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("rate_limit") || err.contains("Rate limit"),
        "error should mention rate limit: {}",
        err
    );
}

#[tokio::test]
async fn mock_provider_stream_yields_text_and_usage_chunks() {
    let script = single_response_json(r#"{"test":"data"}"#, 10, 5);
    let (_dir, provider) = mock_provider_with_script(&script);

    let mut stream = provider.stream(make_request()).await.unwrap();
    use futures::StreamExt;
    let mut full_text = String::new();
    let mut usage_seen = false;
    while let Some(chunk) = stream.next().await {
        match chunk.unwrap() {
            StreamChunk::Text(t) => full_text.push_str(&t),
            StreamChunk::Usage(u) => {
                usage_seen = true;
                assert_eq!(u.input_tokens, 10);
                assert_eq!(u.output_tokens, 5);
            }
        }
    }
    assert!(
        full_text.contains("test"),
        "text should contain 'test': {}",
        full_text
    );
    assert!(usage_seen, "should receive a Usage chunk");
}

#[tokio::test]
async fn mock_provider_serves_sequential_responses() {
    let script = serde_json::json!({
        "models": {
            "mock-test": {
                "responses": [
                    { "text": "first", "input_tokens": 10, "output_tokens": 5 },
                    { "text": "second", "input_tokens": 15, "output_tokens": 8 }
                ]
            }
        }
    })
    .to_string();
    let (_dir, provider) = mock_provider_with_script(&script);

    let resp1 = provider.complete(make_request()).await.unwrap();
    assert_eq!(resp1.content, "first");
    assert_eq!(resp1.usage.input_tokens, 10);

    let resp2 = provider.complete(make_request()).await.unwrap();
    assert_eq!(resp2.content, "second");
    assert_eq!(resp2.usage.input_tokens, 15);
}

#[tokio::test]
async fn mock_provider_returns_error_on_unknown_model() {
    let script = single_response_json("hello", 10, 5);
    let (_dir, provider) = mock_provider_with_script(&script);

    let req = CompletionRequest {
        model: "nonexistent-model".to_string(),
        system_prompt: "".to_string(),
        user_message: "test".to_string(),
        max_tokens: 8192,
        temperature: 0.2,
        json_schema: None,
    };
    let result = provider.complete(req).await;
    assert!(result.is_err());
}

#[test]
fn mock_script_builder_creates_valid_json() {
    let script = MockScriptBuilder::new()
        .add_response("mock-test", "test output", 10, 5)
        .to_json_string();
    let parsed: serde_json::Value = serde_json::from_str(&script).unwrap();
    assert!(parsed["models"]["mock-test"]["responses"].is_array());
    assert_eq!(
        parsed["models"]["mock-test"]["responses"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn mock_script_builder_adds_error_entries() {
    let script = MockScriptBuilder::new()
        .add_error("mock-test", "rate_limit", "Too many requests")
        .to_json_string();
    let parsed: serde_json::Value = serde_json::from_str(&script).unwrap();
    assert!(parsed["models"]["mock-test"]["responses"][0]["error"].is_object());
    assert_eq!(
        parsed["models"]["mock-test"]["responses"][0]["error"]["kind"],
        "rate_limit"
    );
}

#[test]
fn mock_script_builder_multiple_responses() {
    let script = MockScriptBuilder::new()
        .add_response("mock-test", "first", 10, 5)
        .add_response("mock-test", "second", 20, 10)
        .add_response("mock-test", "third", 30, 15)
        .to_json_string();
    let parsed: serde_json::Value = serde_json::from_str(&script).unwrap();
    let responses = parsed["models"]["mock-test"]["responses"]
        .as_array()
        .unwrap();
    assert_eq!(responses.len(), 3);
    assert_eq!(responses[0]["text"], "first");
    assert_eq!(responses[1]["text"], "second");
    assert_eq!(responses[2]["text"], "third");
}

#[test]
fn mock_script_builder_can_write_to_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("script.json");
    let _ = MockScriptBuilder::new()
        .add_response("mock-test", "hello", 10, 5)
        .write(&path);
    assert!(path.exists());
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("hello"));
}
