use niki::config::NikiConfig;
use std::fs;
use std::sync::{Mutex, OnceLock};
use tempfile::TempDir;

/// Serialize every test that mutates process-global environment variables.
/// `cargo test` runs test fns in the same binary in parallel, so without this
/// lock two tests setting/reading the same var would race.
static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
fn env_lock() -> &'static Mutex<()> {
    ENV_LOCK.get_or_init(|| Mutex::new(()))
}

/// Neutralize every env var NIKI's `apply_env_vars` reads, so each test is
/// deterministic regardless of the surrounding environment. `apply_env_vars`
/// prefers ANTHROPIC_AUTH_TOKEN / OPENROUTER_API_KEY over ANTHROPIC_API_KEY and
/// over toml keys, and now also reads ANTHROPIC_MODEL / OPENAI_MODEL and the
/// `*_BASE_URL` vars. Clearing them (to empty) drops that precedence so the
/// value under test is the one that wins.
unsafe fn clear_provider_env_vars() {
    // Use remove_var (not set to "") so a present-but-empty value can't be
    // mistaken for a real key/url/model by `apply_env_vars`.
    std::env::remove_var("ANTHROPIC_AUTH_TOKEN");
    std::env::remove_var("OPENROUTER_API_KEY");
    std::env::remove_var("NIKI_PROVIDERS_ANTHROPIC_API_KEY");
    std::env::remove_var("ANTHROPIC_API_KEY");
    std::env::remove_var("OPENAI_API_KEY");
    std::env::remove_var("GOOGLE_API_KEY");
    std::env::remove_var("ANTHROPIC_BASE_URL");
    std::env::remove_var("OPENAI_BASE_URL");
    std::env::remove_var("ANTHROPIC_MODEL");
    std::env::remove_var("OPENAI_MODEL");
}

#[test]
fn test_config_loads_from_toml() {
    let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
    unsafe { clear_provider_env_vars() }
    let dir = TempDir::new().unwrap();
    let toml = r#"
[general]
max_revision_rounds = 2
output_dir = ".niki"

[providers.anthropic]
api_key = "test-key-123"
default_model = "claude-sonnet-4-20250514"

[agents.planner]
provider = "anthropic"
model = "claude-sonnet-4-20250514"
"#;
    fs::write(dir.path().join("niki.toml"), toml).unwrap();

    let config = NikiConfig::load(dir.path()).expect("load config");

    assert_eq!(config.general.max_revision_rounds, 2);
    assert_eq!(config.general.output_dir, ".niki");

    let anthropic = config
        .providers
        .get("anthropic")
        .expect("anthropic provider present");
    assert_eq!(anthropic.api_key.as_deref(), Some("test-key-123"));
    assert_eq!(anthropic.default_model, "claude-sonnet-4-20250514");
}

#[test]
fn test_config_env_var_override() {
    let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
    // No TOML present; only an environment variable supplies the key.
    unsafe { clear_provider_env_vars() }
    unsafe { std::env::set_var("ANTHROPIC_API_KEY", "env-key-456"); }
    let dir = TempDir::new().unwrap();

    let config = NikiConfig::load(dir.path()).expect("load config");

    let anthropic = config
        .providers
        .get("anthropic")
        .expect("anthropic provider should be created from env var");
    assert_eq!(anthropic.api_key.as_deref(), Some("env-key-456"));
}

#[test]
fn test_env_base_url_and_model() {
    let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
    unsafe { clear_provider_env_vars() }
    unsafe {
        std::env::set_var("ANTHROPIC_BASE_URL", "https://gw.example.com");
        std::env::set_var("ANTHROPIC_MODEL", "claude-opus-4-20250514");
        std::env::set_var("OPENAI_BASE_URL", "https://ow.example.com/v1");
        std::env::set_var("OPENAI_MODEL", "gpt-4o");
    }
    let dir = TempDir::new().unwrap();
    let config = NikiConfig::load(dir.path()).expect("load config");

    let anthropic = config.providers.get("anthropic").expect("anthropic provider");
    assert_eq!(anthropic.base_url.as_deref(), Some("https://gw.example.com"));
    assert_eq!(anthropic.default_model, "claude-opus-4-20250514");

    let openai = config.providers.get("openai").expect("openai provider");
    assert_eq!(openai.base_url.as_deref(), Some("https://ow.example.com/v1"));
    assert_eq!(openai.default_model, "gpt-4o");

    // Agents still on the provider default model pick up the env model.
    assert_eq!(config.agents.planner.model, "claude-opus-4-20250514");
    assert_eq!(config.agents.coder.model, "claude-opus-4-20250514");
    assert_eq!(config.agents.reviewer.model, "claude-opus-4-20250514");
    assert_eq!(config.agents.tester.model, "gpt-4o");
}
