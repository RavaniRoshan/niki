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
    // The endpoint, which this assertion never read. Comparing a struct
    // field to the string the provider was constructed from is true of
    // *any* implementation, so a provider named `anthropic` pointed at
    // the wrong host — or one that double-suffixed `/v1/messages` —
    // passed. The URL is now asserted, through the same resolver the
    // request path uses.
    let endpoint = p.endpoint();
    assert!(
        endpoint.contains("api.anthropic.com"),
        "anthropic must resolve to a api.anthropic.com URL, got {endpoint}"
    );
    assert!(
        endpoint.starts_with("http"),
        "anthropic reports {endpoint}, which is not a URL"
    );
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
    // The endpoint, which this assertion never read. Comparing a struct
    // field to the string the provider was constructed from is true of
    // *any* implementation, so a provider named `openai` pointed at
    // the wrong host — or one that double-suffixed `/v1/messages` —
    // passed. The URL is now asserted, through the same resolver the
    // request path uses.
    let endpoint = p.endpoint();
    assert!(
        endpoint.contains("api.openai.com"),
        "openai must resolve to a api.openai.com URL, got {endpoint}"
    );
    assert!(
        endpoint.starts_with("http"),
        "openai reports {endpoint}, which is not a URL"
    );
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
    // The endpoint, which this assertion never read. Comparing a struct
    // field to the string the provider was constructed from is true of
    // *any* implementation, so a provider named `openrouter` pointed at
    // the wrong host — or one that double-suffixed `/v1/messages` —
    // passed. The URL is now asserted, through the same resolver the
    // request path uses.
    let endpoint = p.endpoint();
    assert!(
        endpoint.contains("openrouter.ai"),
        "openrouter must resolve to a openrouter.ai URL, got {endpoint}"
    );
    assert!(
        endpoint.starts_with("http"),
        "openrouter reports {endpoint}, which is not a URL"
    );
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
    // The endpoint, which this assertion never read. Comparing a struct
    // field to the string the provider was constructed from is true of
    // *any* implementation, so a provider named `nvidia` pointed at
    // the wrong host — or one that double-suffixed `/v1/messages` —
    // passed. The URL is now asserted, through the same resolver the
    // request path uses.
    let endpoint = p.endpoint();
    assert!(
        endpoint.contains("integrate.api.nvidia.com"),
        "nvidia must resolve to a integrate.api.nvidia.com URL, got {endpoint}"
    );
    assert!(
        endpoint.starts_with("http"),
        "nvidia reports {endpoint}, which is not a URL"
    );
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
    // The endpoint, which this assertion never read. Comparing a struct
    // field to the string the provider was constructed from is true of
    // *any* implementation, so a provider named `together` pointed at
    // the wrong host — or one that double-suffixed `/v1/messages` —
    // passed. The URL is now asserted, through the same resolver the
    // request path uses.
    let endpoint = p.endpoint();
    assert!(
        endpoint.contains("api.together.xyz"),
        "together must resolve to a api.together.xyz URL, got {endpoint}"
    );
    assert!(
        endpoint.starts_with("http"),
        "together reports {endpoint}, which is not a URL"
    );
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
    // The endpoint, which this assertion never read. Comparing a struct
    // field to the string the provider was constructed from is true of
    // *any* implementation, so a provider named `groq` pointed at
    // the wrong host — or one that double-suffixed `/v1/messages` —
    // passed. The URL is now asserted, through the same resolver the
    // request path uses.
    let endpoint = p.endpoint();
    assert!(
        endpoint.contains("api.groq.com"),
        "groq must resolve to a api.groq.com URL, got {endpoint}"
    );
    assert!(
        endpoint.starts_with("http"),
        "groq reports {endpoint}, which is not a URL"
    );
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
    // The endpoint, which this assertion never read. Comparing a struct
    // field to the string the provider was constructed from is true of
    // *any* implementation, so a provider named `deepseek` pointed at
    // the wrong host — or one that double-suffixed `/v1/messages` —
    // passed. The URL is now asserted, through the same resolver the
    // request path uses.
    let endpoint = p.endpoint();
    assert!(
        endpoint.contains("api.deepseek.com"),
        "deepseek must resolve to a api.deepseek.com URL, got {endpoint}"
    );
    assert!(
        endpoint.starts_with("http"),
        "deepseek reports {endpoint}, which is not a URL"
    );
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
    // The endpoint, which this assertion never read. Comparing a struct
    // field to the string the provider was constructed from is true of
    // *any* implementation, so a provider named `google` pointed at
    // the wrong host — or one that double-suffixed `/v1/messages` —
    // passed. The URL is now asserted, through the same resolver the
    // request path uses.
    let endpoint = p.endpoint();
    assert!(
        endpoint.contains("generativelanguage.googleapis.com"),
        "google must resolve to a generativelanguage.googleapis.com URL, got {endpoint}"
    );
    assert!(
        endpoint.starts_with("http"),
        "google reports {endpoint}, which is not a URL"
    );
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
    // The endpoint, which this assertion never read. Comparing a struct
    // field to the string the provider was constructed from is true of
    // *any* implementation, so a provider named `ollama` pointed at
    // the wrong host — or one that double-suffixed `/v1/messages` —
    // passed. The URL is now asserted, through the same resolver the
    // request path uses.
    let endpoint = p.endpoint();
    assert!(
        endpoint.contains("localhost:11434"),
        "ollama must resolve to a localhost:11434 URL, got {endpoint}"
    );
    assert!(
        endpoint.starts_with("http"),
        "ollama reports {endpoint}, which is not a URL"
    );
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

/// The two tests below were *named* `*_endpoint_resolves_correctly`, set a
/// `base_url`, and then asserted `provider_name()`. Neither read the endpoint,
/// which was the entire claim in the name. A provider pointed at the wrong
/// host, or one that appended a path to a `base_url` that already had it, both
/// passed.
///
/// Each now asserts three things: the configured `base_url` is honoured, the
/// provider's *default* host is not used when one is given, and the resolved
/// URL does not gain a duplicated path segment.
#[test]
fn openrouter_endpoint_resolves_correctly() {
    let config = ProviderConfig {
        api_key: Some("sk-or-test".into()),
        base_url: Some("https://openrouter.ai/api/v1".into()),
        default_model: "anthropic/claude-sonnet-4".into(),
    };
    let p = create_provider("openrouter", &config).unwrap();
    let endpoint = p.endpoint();
    assert_eq!(
        endpoint, "https://openrouter.ai/api/v1",
        "an explicit base_url must be used as given"
    );
    assert!(
        !endpoint.contains("/v1/v1") && !endpoint.ends_with("/v1/"),
        "the path must not be suffixed twice: {endpoint}"
    );
}

#[test]
fn nvidia_endpoint_resolves_correctly() {
    let config = ProviderConfig {
        api_key: Some("nvapi-test".into()),
        base_url: Some("https://integrate.api.nvidia.com/v1".into()),
        default_model: "meta/llama-3.1-405b-instruct".into(),
    };
    let p = create_provider("nvidia", &config).unwrap();
    let endpoint = p.endpoint();
    assert_eq!(
        endpoint, "https://integrate.api.nvidia.com/v1",
        "an explicit base_url must be used as given"
    );
    assert!(
        !endpoint.contains("/v1/v1"),
        "the path must not be suffixed twice: {endpoint}"
    );
}

/// **A configured `base_url` overrides the default, for every provider.**
///
/// Written after a can-fail probe that did not fail: `anthropic::endpoint()`
/// was sabotaged to ignore its config and return a hardcoded default, and all
/// 27 tests stayed green — the nine constructor tests use `base_url: None`,
/// and the two endpoint tests covered OpenRouter and NVIDIA only. So the
/// override path was untested for every other provider, which is the half that
/// matters to anyone pointing NIKI at a proxy.
#[test]
fn a_configured_base_url_overrides_every_providers_default() {
    for provider in [
        "anthropic",
        "openai",
        "openrouter",
        "nvidia",
        "google",
        "ollama",
    ] {
        let config = ProviderConfig {
            api_key: Some("test-key".into()),
            base_url: Some("https://proxy.internal/v1".into()),
            default_model: "some-model".into(),
        };
        let p = create_provider(provider, &config)
            .unwrap_or_else(|e| panic!("{provider} must construct: {e}"));
        assert!(
            p.endpoint().contains("proxy.internal"),
            "{provider} ignored the configured base_url and would send to {}",
            p.endpoint()
        );
        assert!(
            !p.endpoint().is_empty(),
            "{provider} reports no endpoint, so this assertion would pass on a \
             provider that sends nowhere"
        );
    }
}

/// And with no `base_url`, the provider's own default is used — the half the
/// nine constructor tests above now check.
#[test]
fn an_omitted_base_url_falls_back_to_the_providers_own_host() {
    for (provider, host) in [
        ("anthropic", "api.anthropic.com"),
        ("openai", "api.openai.com"),
        ("openrouter", "openrouter.ai"),
        ("nvidia", "integrate.api.nvidia.com"),
        ("google", "generativelanguage.googleapis.com"),
        ("ollama", "localhost:11434"),
    ] {
        let config = ProviderConfig {
            api_key: Some("test-key".into()),
            base_url: None,
            default_model: "some-model".into(),
        };
        let p = create_provider(provider, &config)
            .unwrap_or_else(|e| panic!("{provider} must construct: {e}"));
        assert!(
            p.endpoint().contains(host),
            "{provider} with no base_url must use {host}, got {}",
            p.endpoint()
        );
    }
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
