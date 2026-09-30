//! `niki auth login` must be a real path, not a dead end.
//!
//! `niki auth login` writes the key to the OS keyring and tells you so:
//! "Credentials are stored securely in your OS keyring. Run `niki doctor` to
//! verify your setup." `niki doctor` reads it back. **Nothing on the request
//! path did** — `resolve_api_key` was called from `config.rs`, `recommend.rs`
//! and `providers.rs`, and not from `chat.rs` or `llm/provider.rs`.
//!
//! So the documented first run was:
//!
//!     $ niki auth login     "stored securely in your OS keyring"
//!     $ niki doctor         green
//!     $ niki                 the chat
//!     > hello                "No LLM provider is configured yet. Run `niki init`
//!                              (or `niki auth login`) to set one up"
//!
//! The chat naming as the fix the exact command that had just been run. The
//! keyring is a service a test cannot depend on, so the lookup is injected and
//! the wiring is what is asserted.

use niki::config::types::NikiConfig;

fn config() -> NikiConfig {
    let toml = r#"
[docker]
backend = "worktree"

[agents.coder]
provider = "anthropic"
model = "claude-sonnet-4-20250514"
"#;
    toml::from_str(toml).expect("config parses")
}

/// A stored key reaches the provider the request path will use.
#[test]
fn a_key_stored_in_the_keyring_reaches_the_configured_provider() {
    let mut c = config();
    assert!(
        c.providers
            .get("anthropic")
            .and_then(|p| p.api_key.clone())
            .is_none(),
        "precondition: no key yet"
    );
    c.resolve_keyring_with(&|name| (name == "anthropic").then(|| "sk-stored".to_string()));
    assert_eq!(
        c.providers
            .get("anthropic")
            .and_then(|p| p.api_key.clone())
            .as_deref(),
        Some("sk-stored"),
        "a key the user stored with `niki auth login` must be readable by the request path"
    );
}

/// The environment still wins, so a CI secret is not shadowed by a key on a
/// developer's machine.
#[test]
fn the_environment_still_beats_the_keyring() {
    let mut c = config();
    c.providers
        .entry("anthropic".to_string())
        .or_default()
        .api_key = Some("from-env".to_string());
    c.resolve_keyring_with(&|_| Some("from-keyring".to_string()));
    assert_eq!(
        c.providers
            .get("anthropic")
            .and_then(|p| p.api_key.clone())
            .as_deref(),
        Some("from-env"),
        "an explicit key must not be overwritten by a stored one"
    );
}

/// An unavailable keyring is not an error — it means "no stored key", which
/// the request path already reports clearly.
#[test]
fn a_missing_keyring_changes_nothing() {
    let mut c = config();
    c.resolve_keyring_with(&|_| None);
    assert!(
        c.providers
            .get("anthropic")
            .and_then(|p| p.api_key.clone())
            .is_none(),
        "a locked or absent keyring must leave the config exactly as it was"
    );
    // And an empty string is not a key.
    let mut c = config();
    c.resolve_keyring_with(&|_| Some(String::new()));
    assert!(
        c.providers
            .get("anthropic")
            .and_then(|p| p.api_key.clone())
            .is_none(),
        "an empty stored value must not become an api_key"
    );
}

/// Every provider the wizard and `niki auth login` offer is covered, so a
/// future slug cannot be added to the menu and forgotten here.
#[test]
fn every_authenticatable_provider_is_reachable_from_config() {
    for slug in [
        "anthropic",
        "openai",
        "google",
        "openrouter",
        "groq",
        "kimi",
    ] {
        let mut c = NikiConfig::default();
        c.resolve_keyring_with(&|name| (name == slug).then(|| format!("key-for-{slug}")));
        assert_eq!(
            c.providers
                .get(slug)
                .and_then(|p| p.api_key.clone())
                .as_deref(),
            Some(format!("key-for-{slug}").as_str()),
            "{slug} is offered by `niki auth login` and must resolve"
        );
    }

    // Every slug the product knows must be reachable, so a provider cannot be
    // added to the menu and forgotten here.
    let mut c = NikiConfig::default();
    c.resolve_keyring_with(&|_| Some("k".to_string()));
    for slug in niki::config::types::PROVIDER_SLUGS {
        assert!(
            c.providers.contains_key(slug),
            "{slug} is a known provider and must be resolvable from the keyring"
        );
    }
}

/// `NikiConfig::load` must consult the keyring.
///
/// The other tests here call `resolve_keyring_with` directly, so they would
/// stay green if the call were removed from `load` — which is the actual
/// defect: the keyring was readable and never read. This asserts the wiring on
/// the real path.
///
/// Source-scanning, deliberately. The same technique is already used in this
/// repo for exactly this class of bug — `tests/ci_contracts.rs` pins the
/// workflow's `|| true` placement, and `src/display/tui.rs`'s own tests scan
/// for a re-introduced event-read ladder. A behavioural test cannot reach this
/// without a real OS keyring service, and a test that needs one does not run on
/// a contributor's machine or in CI.
#[test]
fn loading_a_config_consults_the_keyring() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/config/types.rs"),
    )
    .expect("config/types.rs is readable");

    let load_body = src
        .split("pub fn load(")
        .nth(1)
        .and_then(|rest| rest.split("\n    pub fn ").next())
        .expect("NikiConfig::load exists");
    assert!(
        load_body.contains("resolve_keyring()"),
        "NikiConfig::load must call resolve_keyring(). Without it the keyring is \\
         written by `niki auth login` and read by `niki doctor`, and by nothing \\
         on the request path — so the documented first run ends with the chat \\
         saying \"Run niki auth login\" about the key that was just stored."
    );
}
