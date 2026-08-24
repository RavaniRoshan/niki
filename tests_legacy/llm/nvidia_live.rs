mod common;

#[tokio::test]
#[ignore = "requires NVIDIA_API_KEY env var + network"]
async fn nvidia_nim_live_chat() {
    let api_key = std::env::var("NVIDIA_API_KEY").expect("set NVIDIA_API_KEY");
    let mut cfg = niki::config::ProviderConfig::default();
    cfg.api_key = Some(api_key);
    cfg.base_url = Some("https://integrate.api.nvidia.com/v1".to_string());

    let provider = niki::llm::provider::create_provider("nvidia", &cfg)
        .expect("create nvidia provider");
    let req = niki::llm::provider::CompletionRequest {
        model: "meta/llama-3-70b-instruct".to_string(),
        system_prompt: "You are NIKI, a concise coding assistant.".to_string(),
        user_message: "Say hello in a fenced code block".to_string(),
        max_tokens: 128,
        temperature: 0.7,
        json_schema: None,
        tools: None,
    };
    let resp = provider.complete(req).await.expect("LLM call");
    assert!(resp.usage.output_tokens > 0, "should produce output tokens");
    assert!(resp.content.contains("```"), "should contain a code fence");
}
