mod common;

use niki::llm::provider::redact_secrets;

#[test]
fn redact_secrets_replaces_bearer_token() {
    let input = "Authorization: Bearer sk-abc123def456ghi789jkl012mno345pqr678";
    let result = redact_secrets(input);
    assert!(
        !result.contains("sk-abc123def456ghi789jkl012mno345pqr678"),
        "bearer token should be redacted: {}",
        result
    );
    assert!(
        result.contains("[REDACTED]"),
        "result should contain [REDACTED]: {}",
        result
    );
}

#[test]
fn redact_secrets_replaces_api_key() {
    let input = r#"{"error": "invalid_request_error", "param": null, "code": null, "type": "invalid_api_key", "message": "Incorrect API key provided: sk-proj-abc123def456ghi789jkl012mno345pqr"}."#;
    let result = redact_secrets(input);
    assert!(
        !result.contains("sk-proj-abc123def456ghi789jkl012mno345pqr"),
        "API key should be redacted: {}",
        result
    );
    assert!(
        result.contains("[REDACTED]"),
        "result should contain [REDACTED]: {}",
        result
    );
}

#[test]
fn redact_secrets_replaces_openai_key() {
    let input = "Error: sk-1234567890abcdefghijklmnopqrstuv not found";
    let result = redact_secrets(input);
    assert!(
        !result.contains("sk-1234567890abcdefghijklmnopqrstuv"),
        "openai key should be redacted: {}",
        result
    );
}

#[test]
fn redact_secrets_replaces_github_token() {
    let input = "ghp_abcdefghijklmnopqrstuvwxyz0123456789AB";
    let result = redact_secrets(input);
    assert!(
        !result.contains("ghp_abcdefghijklmnopqrstuvwxyz0123456789AB"),
        "github token should be redacted: {}",
        result
    );
}

#[test]
fn redact_secrets_preserves_context() {
    let input = "HTTP 401: {\"error\":{\"message\":\"Invalid API key\"}}";
    let result = redact_secrets(input);
    assert!(
        result.contains("HTTP 401"),
        "should preserve status: {}",
        result
    );
    assert!(
        result.contains("Invalid API key"),
        "should preserve message: {}",
        result
    );
}

#[test]
fn redact_secrets_handles_empty_string() {
    let result = redact_secrets("");
    assert_eq!(result, "");
}

#[test]
fn redact_secrets_handles_no_secrets() {
    let input = "Some error message without any secrets";
    let result = redact_secrets(input);
    assert_eq!(result, input);
}

#[test]
fn redact_secrets_replaces_multiple_keys() {
    let input = "Key1: sk-aaaaaaaaaaaaaaaaaaaa Key2: sk-bbbbbbbbbbbbbbbbbbbbbbbb";
    let result = redact_secrets(input);
    assert!(
        !result.contains("sk-aaaaaaaaaaaaaaaaaaaa"),
        "first key should be redacted: {}",
        result
    );
    assert!(
        !result.contains("sk-bbbbbbbbbbbbbbbbbbbbbbbb"),
        "second key should be redacted: {}",
        result
    );
}

#[test]
fn a_too_short_string_is_left_alone_rather_than_mangled() {
    // The old assertion was `assert!(!result.is_empty())`, which cannot fail:
    // the function always returns a String. The documented behaviour is that
    // `sk-` needs a real key body after it, so a 5-character tail is not a
    // credential and must survive untouched — redacting it would corrupt
    // ordinary prose.
    let input = "sk-short";
    let result = redact_secrets(input);
    assert_eq!(
        result, input,
        "a string too short to be a key must be returned unchanged"
    );
}

#[test]
fn a_real_length_key_is_redacted() {
    // The positive counterpart, so the test above cannot pass by redacting
    // everything.
    let key = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAA";
    let out = redact_secrets(&format!("using {key} now"));
    assert!(!out.contains("AAAAAAAAAAAAAAAAAAAAAAAA"), "{out:?}");
    assert!(
        out.contains("using "),
        "surrounding text must survive: {out:?}"
    );
}

#[test]
fn redact_secrets_replaces_authorization_header() {
    let input =
        "authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0";
    let result = redact_secrets(input);
    assert!(
        result.contains("[REDACTED]"),
        "should redact authorization header: {}",
        result
    );
}

#[test]
fn redact_secrets_preserves_non_secret_content() {
    let input = "HTTP 401: {\"error\":{\"message\":\"Invalid API key\", \"type\": \"invalid_request_error\", \"code\": null}}";
    let result = redact_secrets(input);
    assert!(result.contains("HTTP 401"));
    assert!(result.contains("Invalid API key"));
    assert!(result.contains("invalid_request_error"));
    assert!(!result.contains("password"));
}
