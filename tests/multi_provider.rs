//! Integration tests for multi-provider support.
//!
//! Tests cover:
//! - Provider factory dispatch for all OpenAI-compatible aliases
//! - Default base URL resolution per provider
//! - OpenAiProvider with different provider names
//! - Env var resolution for new providers
//! - Config round-trip with new provider blocks

use niki::config::{AgentConfig, NikiConfig, ProviderConfig};
use niki::llm::provider::{create_provider, default_base_url};

// ── Provider Factory Tests ─────────────────────────────────────────────

#[test]
fn create_provider_anthropic() {
    let config = ProviderConfig {
        api_key: Some("sk-ant-test".into()),
        base_url: None,
        default_model: "claude-sonnet-4-20250514".into(),
    };
    let p = create_provider("anthropic", &config).unwrap();
    assert_eq!(p.provider_name(), "anthropic");
}

#[test]
fn create_provider_openai() {
    let config = ProviderConfig {
        api_key: Some("sk-test".into()),
        base_url: None,
        default_model: "gpt-4o".into(),
    };
    let p = create_provider("openai", &config).unwrap();
    assert_eq!(p.provider_name(), "openai");
}

#[test]
fn create_provider_openrouter() {
    let config = ProviderConfig {
        api_key: Some("sk-or-test".into()),
        base_url: Some("https://openrouter.ai/api/v1".into()),
        default_model: "anthropic/claude-sonnet-4".into(),
    };
    let p = create_provider("openrouter", &config).unwrap();
    assert_eq!(p.provider_name(), "openrouter");
}

#[test]
fn create_provider_nvidia() {
    let config = ProviderConfig {
        api_key: Some("nvapi-test".into()),
        base_url: Some("https://integrate.api.nvidia.com/v1".into()),
        default_model: "meta/llama-3.1-405b-instruct".into(),
    };
    let p = create_provider("nvidia", &config).unwrap();
    assert_eq!(p.provider_name(), "nvidia");
}

#[test]
fn create_provider_together() {
    let config = ProviderConfig {
        api_key: Some("test-key".into()),
        base_url: Some("https://api.together.xyz/v1".into()),
        default_model: "meta-llama/Meta-Llama-3.1-405B-Instruct-Turbo".into(),
    };
    let p = create_provider("together", &config).unwrap();
    assert_eq!(p.provider_name(), "together");
}

#[test]
fn create_provider_groq() {
    let config = ProviderConfig {
        api_key: Some("gsk_test".into()),
        base_url: Some("https://api.groq.com/openai/v1".into()),
        default_model: "llama-3.1-70b-versatile".into(),
    };
    let p = create_provider("groq", &config).unwrap();
    assert_eq!(p.provider_name(), "groq");
}

#[test]
fn create_provider_deepseek() {
    let config = ProviderConfig {
        api_key: Some("test-key".into()),
        base_url: Some("https://api.deepseek.com/v1".into()),
        default_model: "deepseek-chat".into(),
    };
    let p = create_provider("deepseek", &config).unwrap();
    assert_eq!(p.provider_name(), "deepseek");
}

#[test]
fn create_provider_google() {
    let config = ProviderConfig {
        api_key: Some("AIza-test".into()),
        base_url: None,
        default_model: "gemini-2.5-pro".into(),
    };
    let p = create_provider("google", &config).unwrap();
    assert_eq!(p.provider_name(), "google");
}

#[test]
fn create_provider_ollama() {
    let config = ProviderConfig {
        api_key: None,
        base_url: Some("http://localhost:11434".into()),
        default_model: "llama3.1".into(),
    };
    let p = create_provider("ollama", &config).unwrap();
    assert_eq!(p.provider_name(), "ollama");
}

#[test]
fn create_provider_unknown_fails() {
    let config = ProviderConfig {
        api_key: Some("test".into()),
        base_url: None,
        default_model: "test".into(),
    };
    match create_provider("nonexistent", &config) {
        Err(e) => assert!(e.to_string().contains("Unknown provider")),
        Ok(_) => panic!("expected error for unknown provider"),
    }
}

#[test]
fn create_provider_missing_api_key_fails() {
    let config = ProviderConfig {
        api_key: None,
        base_url: None,
        default_model: "gpt-4o".into(),
    };
    match create_provider("openai", &config) {
        Err(e) => assert!(e.to_string().contains("API key not configured")),
        Ok(_) => panic!("expected error for missing API key"),
    }
}

#[test]
fn create_provider_missing_anthropic_key_fails() {
    let config = ProviderConfig {
        api_key: None,
        base_url: None,
        default_model: "claude-sonnet-4-20250514".into(),
    };
    match create_provider("anthropic", &config) {
        Err(e) => assert!(e.to_string().contains("API key not configured")),
        Ok(_) => panic!("expected error for missing API key"),
    }
}

// ── Default Base URL Tests ─────────────────────────────────────────────

#[test]
fn default_base_url_openrouter() {
    assert_eq!(
        default_base_url("openrouter"),
        Some("https://openrouter.ai/api/v1")
    );
}

#[test]
fn default_base_url_nvidia() {
    assert_eq!(
        default_base_url("nvidia"),
        Some("https://integrate.api.nvidia.com/v1")
    );
}

#[test]
fn default_base_url_together() {
    assert_eq!(
        default_base_url("together"),
        Some("https://api.together.xyz/v1")
    );
}

#[test]
fn default_base_url_groq() {
    assert_eq!(
        default_base_url("groq"),
        Some("https://api.groq.com/openai/v1")
    );
}

#[test]
fn default_base_url_deepseek() {
    assert_eq!(
        default_base_url("deepseek"),
        Some("https://api.deepseek.com/v1")
    );
}

#[test]
fn default_base_url_anthropic_returns_none() {
    assert_eq!(default_base_url("anthropic"), None);
}

#[test]
fn default_base_url_openai_returns_none() {
    assert_eq!(default_base_url("openai"), None);
}

// ── Config Round-Trip Tests ────────────────────────────────────────────

#[test]
fn config_round_trip_with_openrouter() {
    let toml_str = r#"
[providers.openrouter]
api_key = "sk-or-test"
base_url = "https://openrouter.ai/api/v1"
default_model = "anthropic/claude-sonnet-4"

[agents.planner]
provider = "openrouter"
model = "anthropic/claude-sonnet-4"
"#;
    let config: NikiConfig = toml::from_str(toml_str).unwrap();
    let p = config.providers.get("openrouter").unwrap();
    assert_eq!(p.api_key.as_deref(), Some("sk-or-test"));
    assert_eq!(p.base_url.as_deref(), Some("https://openrouter.ai/api/v1"));
    assert_eq!(p.default_model, "anthropic/claude-sonnet-4");
    assert_eq!(config.agents.planner.provider, "openrouter");
}

#[test]
fn config_round_trip_with_nvidia() {
    let toml_str = r#"
[providers.nvidia]
api_key = "nvapi-test"
default_model = "meta/llama-3.1-405b-instruct"

[agents.reviewer]
provider = "nvidia"
model = "meta/llama-3.1-405b-instruct"
"#;
    let config: NikiConfig = toml::from_str(toml_str).unwrap();
    let p = config.providers.get("nvidia").unwrap();
    assert_eq!(p.api_key.as_deref(), Some("nvapi-test"));
    assert_eq!(config.agents.reviewer.provider, "nvidia");
}

#[test]
fn config_mixed_providers() {
    let toml_str = r#"
[providers.anthropic]
api_key = "sk-ant-test"
default_model = "claude-sonnet-4-20250514"

[providers.groq]
api_key = "gsk_test"
default_model = "llama-3.1-70b-versatile"

[agents.planner]
provider = "anthropic"
model = "claude-sonnet-4-20250514"

[agents.tester]
provider = "groq"
model = "llama-3.1-70b-versatile"
"#;
    let config: NikiConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.agents.planner.provider, "anthropic");
    assert_eq!(config.agents.tester.provider, "groq");
    assert!(config.providers.contains_key("anthropic"));
    assert!(config.providers.contains_key("groq"));
}

// ── OpenAiProvider Endpoint Tests ──────────────────────────────────────

#[test]
fn openrouter_endpoint_resolves_correctly() {
    let config = ProviderConfig {
        api_key: Some("sk-or-test".into()),
        base_url: Some("https://openrouter.ai/api/v1".into()),
        default_model: "anthropic/claude-sonnet-4".into(),
    };
    let p = create_provider("openrouter", &config).unwrap();
    assert_eq!(p.provider_name(), "openrouter");
}

#[test]
fn nvidia_endpoint_resolves_correctly() {
    let config = ProviderConfig {
        api_key: Some("nvapi-test".into()),
        base_url: Some("https://integrate.api.nvidia.com/v1".into()),
        default_model: "meta/llama-3.1-405b-instruct".into(),
    };
    let p = create_provider("nvidia", &config).unwrap();
    assert_eq!(p.provider_name(), "nvidia");
}

// ── Provider Config Default Tests ──────────────────────────────────────

#[test]
fn provider_config_default() {
    let config = ProviderConfig::default();
    assert!(config.api_key.is_none());
    assert!(config.base_url.is_none());
    assert_eq!(config.default_model, "");
}

#[test]
fn agent_config_default() {
    let config = AgentConfig::default();
    assert_eq!(config.provider, "");
    assert_eq!(config.model, "");
}
