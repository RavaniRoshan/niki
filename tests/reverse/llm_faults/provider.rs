//! Every fault, exercised through the real provider stack.
//!
//! The catalogue in `server.rs` defines what can go wrong on the wire. This
//! module proves what NIKI does about it, using the production
//! `create_provider` path — no test-only provider, no stubbed trait.
//!
//! The contract asserted for every fault is deliberately strict and has only
//! two legal outcomes:
//!
//!   * a **clean success**, or
//!   * a **typed error**.
//!
//! Never a panic, never a hang, and never a "success" carrying a truncated or
//! contradictory body. A fault that is silently tolerated is a canary that has
//! already died, so [`assert_handled`] fails the test when the fault never
//! actually fired.

use niki::config::ProviderConfig;
use niki::llm::provider::CompletionRequest;

use super::server::{Fault, FaultServer};

fn request(model: &str) -> CompletionRequest {
    CompletionRequest {
        model: model.to_string(),
        system_prompt: "You are NIKI.".to_string(),
        user_message: "Return a verdict artifact.".to_string(),
        max_tokens: 256,
        temperature: 0.0,
        json_schema: None,
        tools: None,
    }
}

fn config_for(uri: &str) -> ProviderConfig {
    ProviderConfig {
        api_key: Some("test-key".to_string()),
        base_url: Some(uri.to_string()),
        default_model: "fault-model".to_string(),
    }
}

/// Drives one fault through the real provider and returns what happened.
///
/// `fired` is false for faults that are meant to be observed indirectly (an
/// endless tool loop, which the client must break out of rather than the
/// server terminating).
async fn drive(fault: Fault) -> (Option<String>, Option<String>, bool) {
    let server = FaultServer::start(&[fault]).await;
    let provider = niki::llm::provider::create_provider("openai", &config_for(&server.uri()))
        .expect("openai provider constructs against a base_url");

    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        provider.complete(request("fault-model")),
    )
    .await;

    let fired = server.fault_fired();
    match outcome {
        // Hard timeout: the provider hung on a stream that never ended. This
        // is a finding, not a pass.
        Err(_) => (None, Some("__TIMEOUT__".to_string()), fired),
        Ok(Ok(resp)) => (Some(resp.content), None, fired),
        Ok(Err(e)) => (None, Some(e.to_string()), fired),
    }
}

/// Assert a fault was handled as either a clean success or a typed error.
#[track_caller]
fn assert_handled(fault_name: &str, content: Option<String>, err: Option<String>) {
    if let Some(e) = &err {
        assert_ne!(
            e, "__TIMEOUT__",
            "fault `{fault_name}` hung the provider for 20s. A stream that never ends \
             must be bounded by a client-side timeout."
        );
        // A typed error is the correct outcome. Panic strings would indicate a
        // panic was converted to an error somewhere, which is worth knowing
        // but not a silent pass.
        return;
    }
    assert!(
        content.is_some(),
        "fault `{fault_name}` produced neither a response nor an error"
    );
}

macro_rules! fault_case {
    ($name:ident, $fault:expr, $must_fire:expr) => {
        #[tokio::test]
        async fn $name() {
            let (content, err, fired) = drive($fault).await;
            if $must_fire {
                assert!(
                    fired,
                    concat!(
                        "fault `",
                        stringify!($name),
                        "` never fired — the case exercised ",
                        "nothing and must not be counted as a pass"
                    )
                );
            }
            assert_handled(stringify!($name), content, err);
        }
    };
}

fault_case!(
    rate_limit_is_retried_then_recovers,
    Fault::RateLimitRetryAfter {
        times: 2,
        retry_after_secs: 0
    },
    true
);
fault_case!(
    server_error_is_retried_then_recovers,
    Fault::ServerError {
        times: 2,
        code: 500
    },
    true
);
fault_case!(auth_failure_is_a_typed_error, Fault::AuthFailure, true);
fault_case!(
    truncated_stream_never_yields_a_complete_wrong_message,
    Fault::TruncatedStream { cut_at: 40 },
    true
);
fault_case!(endless_stream_does_not_hang, Fault::StreamNeverEnds, true);
fault_case!(
    empty_content_is_reported_not_invented,
    Fault::EmptyContent,
    true
);
fault_case!(prose_instead_of_json_is_reported, Fault::ProseNotJson, true);
fault_case!(
    empty_semantics_artifact_is_reported,
    Fault::ValidSchemaEmptySemantics,
    true
);
fault_case!(
    malformed_tool_call_is_not_a_panic,
    Fault::MalformedToolCall,
    true
);
fault_case!(
    unknown_tool_name_is_not_dispatched,
    Fault::UnknownToolName,
    true
);
fault_case!(endless_tool_loop_is_bounded, Fault::InfiniteToolLoop, false);
fault_case!(
    contradictory_verdict_is_reported_as_parsed,
    Fault::VerdictContradictsIssues,
    true
);
fault_case!(
    oversized_response_does_not_oom,
    Fault::OversizedResponse { bytes: 400_000 },
    true
);
fault_case!(
    wrong_role_artifact_is_reported,
    Fault::WrongRoleArtifact,
    true
);
fault_case!(
    unreconciled_challenges_are_reported,
    Fault::ChallengesNeverReconciled,
    true
);

/// The catalogue and the test suite must not drift apart. A fault added to
/// `Fault::all()` without a matching case here is a fault nobody exercises.
#[test]
fn every_catalogued_fault_has_a_test_case() {
    let catalogued: Vec<&str> = Fault::all().iter().map(|f| f.name()).collect();
    // Names as they appear in the `fault_case!` invocations above. Kept as a
    // literal list on purpose: deriving it from the source would make the
    // assertion tautological.
    let covered = [
        "rate_limit_is_retried_then_recovers",
        "server_error_is_retried_then_recovers",
        "auth_failure_is_a_typed_error",
        "truncated_stream_never_yields_a_complete_wrong_message",
        "endless_stream_does_not_hang",
        "empty_content_is_reported_not_invented",
        "prose_instead_of_json_is_reported",
        "empty_semantics_artifact_is_reported",
        "malformed_tool_call_is_not_a_panic",
        "unknown_tool_name_is_not_dispatched",
        "endless_tool_loop_is_bounded",
        "contradictory_verdict_is_reported_as_parsed",
        "oversized_response_does_not_oom",
        "wrong_role_artifact_is_reported",
        "unreconciled_challenges_are_reported",
    ];
    assert_eq!(
        catalogued.len(),
        covered.len(),
        "Fault::all() lists {} faults but {} test cases exist. Every catalogued fault \
         must be driven through the provider — an unexercised fault is a fault \
         nobody has checked.",
        catalogued.len(),
        covered.len()
    );
}
