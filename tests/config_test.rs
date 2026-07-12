use niki::config::NikiConfig;
use std::fs;
use tempfile::TempDir;

/// `apply_env_vars` prefers `ANTHROPIC_AUTH_TOKEN` / `OPENROUTER_API_KEY` over
/// `ANTHROPIC_API_KEY` and over toml keys, so a sandbox-provided value would
/// mask the key under test. Clear them (to empty) so the tests are
/// deterministic regardless of the surrounding environment.
fn clear_priority_auth_vars() {
    // SAFETY: we only set environment variables for test isolation.
    unsafe {
        std::env::set_var("ANTHROPIC_AUTH_TOKEN", "");
        std::env::set_var("OPENROUTER_API_KEY", "");
    }
}

#[test]
fn test_config_loads_from_toml() {
    clear_priority_auth_vars();
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
    // No TOML present; only an environment variable supplies the key.
    clear_priority_auth_vars();
    unsafe { std::env::set_var("ANTHROPIC_API_KEY", "env-key-456"); }
    let dir = TempDir::new().unwrap();

    let config = NikiConfig::load(dir.path()).expect("load config");

    let anthropic = config
        .providers
        .get("anthropic")
        .expect("anthropic provider should be created from env var");
    assert_eq!(anthropic.api_key.as_deref(), Some("env-key-456"));

    unsafe { std::env::remove_var("ANTHROPIC_API_KEY"); }
}
