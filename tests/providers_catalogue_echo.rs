//! A provider's model list must never print the credential that fetched it.
//!
//! `niki providers models` fetches `<base>/models` with
//! `Authorization: Bearer <api_key>` and then prints the `id` of each model in
//! the response. The id is attacker- or accident-controlled in a way the rest
//! of NIKI's output is not: it comes from whatever answered, so a proxy, a
//! corporate gateway, a self-hosted OpenAI-compatible server, or a compromised
//! upstream can put anything in it — including a verbatim echo of the header
//! it just received.
//!
//! That is what CodeQL's `rust/cleartext-logging` alert at that `println!`
//! describes, and it is a real path rather than a false positive: the tainted
//! value is the `api_key` parameter, and it reaches the sink through the HTTP
//! round trip. The other four provider response surfaces — the error bodies in
//! `anthropic.rs`, `openai.rs`, `google.rs` and `ollama.rs` — already pass
//! through `redact_secrets`. This file is the proof that the catalogue does too.
//!
//! The mock is at a real external boundary (a real socket, a real HTTP
//! request), so this is not a mock of the thing under test: `safe_model_id`
//! runs, the `println!` runs, and only the peer is substituted.

use assert_cmd::Command;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// Shaped so `redact_secrets`' `sk-[A-Za-z0-9_-]{20,}` rule matches it. If the
/// shape ever stops matching, this test silently stops testing anything, so it
/// is asserted separately below.
const CANARY_KEY: &str = "sk-canary0123456789abcdefghijklmnop";

/// Serve `/models` with the incoming `Authorization` header echoed back into
/// the model id — the exact leak, standing in for a hostile peer.
async fn echoing_provider() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(|req: &Request| {
            let auth = req
                .headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("no-authorization-header")
                .to_string();
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    { "id": format!("echo-{auth}") },
                    { "id": "claude-sonnet-4" }
                ]
            }))
        })
        .mount(&server)
        .await;
    server
}

/// Run `niki providers models --provider openai` against `server` and return
/// everything the user would see on their terminal.
async fn list_models(server: &MockServer, plain: bool) -> String {
    let cwd = tempfile::TempDir::new().expect("temp cwd, so the repo's niki.toml is not read");

    let mut cmd = Command::cargo_bin("niki").expect("binary niki exists");
    cmd.current_dir(cwd.path())
        .env("OPENAI_API_KEY", CANARY_KEY)
        .env("OPENAI_BASE_URL", format!("{}/v1", server.uri()))
        .env_remove("NIKI_PROVIDERS_ANTHROPIC_API_KEY")
        .args(["providers", "models", "--provider", "openai"])
        // Never let a real credential on the developer's machine answer.
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENROUTER_API_KEY")
        .env_remove("NVIDIA_API_KEY");
    if plain {
        cmd.arg("--plain");
    }

    let out = cmd.output().expect("niki runs");
    let mut seen = String::from_utf8_lossy(&out.stdout).to_string();
    seen.push_str(&String::from_utf8_lossy(&out.stderr));
    assert!(
        out.status.success(),
        "`niki providers models` exited {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    seen
}

/// A `--plain` listing must not contain the key, on either stream.
#[tokio::test]
async fn plain_listing_never_prints_the_credential() {
    let server = echoing_provider().await;
    let seen = list_models(&server, true).await;

    assert!(
        !seen.contains(CANARY_KEY),
        "the API key reached the terminal verbatim through the catalogue:\n{seen}"
    );
}

/// The annotated listing is a second `println!` over the same value, so it
/// gets the same assertion. The first test would still pass if someone fixed
/// only the `--plain` branch.
#[tokio::test]
async fn annotated_listing_never_prints_the_credential() {
    let server = echoing_provider().await;
    let seen = list_models(&server, false).await;

    assert!(
        !seen.contains(CANARY_KEY),
        "the API key reached the terminal verbatim through the catalogue:\n{seen}"
    );
}

/// The output must still be the model list. A redactor that blanks the whole
/// field, or a sanitiser that eats the response, satisfies the two tests above
/// by printing nothing at all.
#[tokio::test]
async fn the_listing_still_lists_models() {
    let server = echoing_provider().await;
    let seen = list_models(&server, true).await;

    assert!(
        seen.contains("claude-sonnet-4"),
        "a legitimate model id disappeared:\n{seen}"
    );
    assert!(
        seen.contains("[REDACTED]") || seen.contains("echo-Bearer"),
        "the echoed id was dropped rather than redacted, so this file no \
         longer exercises the path it was written for:\n{seen}"
    );
}

/// The canary itself. If `CANARY_KEY` stops matching a `redact_secrets` rule,
/// the tests above pass for the wrong reason — they would be asserting that a
/// string that is never treated as a secret is absent from the output.
#[test]
fn the_canary_is_a_shape_redact_secrets_actually_catches() {
    assert!(
        !niki::llm::provider::redact_secrets(CANARY_KEY).contains(CANARY_KEY),
        "redact_secrets no longer masks this key shape, so every test in this \
         file is asserting something trivially true"
    );
}
