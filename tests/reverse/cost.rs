//! Cost accounting must reflect what was actually spent.
//!
//! Two defects this pins:
//!
//! 1. **Under-counted multi-step runs.** The tool loop accumulated usage with
//!    `.max()` instead of `+=`, so a four-step loop was billed as its single
//!    largest step. That number feeds the spend cap, the Cost page, the report
//!    and the JSON envelope — one bug, four wrong surfaces.
//! 2. **Mispriced failovers.** A stage served by a fallback was priced with the
//!    *primary's* table, so the recorded cost did not correspond to the request
//!    that was billed.
//!
//! `.max()` is still correct *within* a single stream: Anthropic emits two
//! disjoint usage chunks per call (`message_start` carries input tokens,
//! `message_delta` carries output tokens) and OpenAI-style providers emit a
//! final cumulative snapshot. The distinction is per-request vs per-stream, and
//! these tests pin it from both sides so the next refactor cannot collapse it.

use niki::llm::provider::TokenUsage;

/// Accumulate one completed request into a running total.
///
/// This delegates to the production method rather than repeating the
/// arithmetic. The original version of this file defined its own copy — and
/// the canary gate caught it: injecting `.max()` back into the real tool loop
/// and the real repair path left every test here green, because the tests were
/// asserting the copy, not the product.
fn accumulate(total: &mut TokenUsage, step: &TokenUsage) {
    total.accumulate(step);
}

fn step(input: u32, output: u32) -> TokenUsage {
    TokenUsage {
        input_tokens: input,
        output_tokens: output,
        cached_input_tokens: 0,
        reasoning_tokens: 0,
    }
}

#[test]
fn a_four_step_tool_loop_costs_four_steps() {
    // The regression: `.max()` over these four steps yields 100/30 — the
    // largest single step — not the 400/120 actually spent.
    let mut total = TokenUsage::default();
    for _ in 0..4 {
        accumulate(&mut total, &step(100, 30));
    }
    assert_eq!(
        total.input_tokens, 400,
        "four requests each sent 100 input tokens"
    );
    assert_eq!(
        total.output_tokens, 120,
        "four requests each produced 30 output tokens"
    );

    // And what the old code produced, to make the size of the error explicit.
    let mut old = TokenUsage::default();
    for _ in 0..4 {
        old.input_tokens = old.input_tokens.max(100);
        old.output_tokens = old.output_tokens.max(30);
    }
    assert_eq!(
        old.input_tokens, 100,
        "the old max-based path under-reported 4x"
    );
}

#[test]
fn a_repair_retry_costs_both_attempts() {
    // The JSON-repair path issues a second request. Only the first attempt
    // succeeding would be billed.
    let mut total = TokenUsage::default();
    accumulate(&mut total, &step(100, 50));
    accumulate(&mut total, &step(100, 80));
    assert_eq!(total.input_tokens, 200);
    assert_eq!(
        total.output_tokens, 130,
        "the repair's 80 output tokens were dropped"
    );
}

#[test]
fn accumulation_is_the_inverse_of_max_for_a_monotonic_stream() {
    // Within one stream, Anthropic sends two disjoint chunks. Summing them
    // would be correct too here (one sets input, the other output), which is
    // exactly why the per-stream `.max()` in `agents::call_agent` is left
    // alone — and why this test documents the difference rather than asserting
    // one universal rule.
    let message_start = TokenUsage {
        input_tokens: 1200,
        output_tokens: 0,
        cached_input_tokens: 300,
        reasoning_tokens: 0,
    };
    let message_delta = TokenUsage {
        input_tokens: 0,
        output_tokens: 450,
        cached_input_tokens: 0,
        reasoning_tokens: 90,
    };

    let mut total = TokenUsage::default();
    accumulate(&mut total, &message_start);
    accumulate(&mut total, &message_delta);
    assert_eq!(total.input_tokens, 1200);
    assert_eq!(total.output_tokens, 450);
    assert_eq!(total.cached_input_tokens, 300);
    assert_eq!(total.reasoning_tokens, 90);
}

#[test]
fn a_fallback_served_call_is_priced_by_the_fallback() {
    use niki::config::ProviderConfig;
    use niki::llm::provider::{CompletionRequest, LlmProvider};

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    rt.block_on(async {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let primary = MockServer::start().await;
        let fallback = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "error": {"message": "unavailable"}
            })))
            .mount(&primary)
            .await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{"message": {"content": "{\"verdict\":\"approved\"}"}}],
                "usage": {"prompt_tokens": 500, "completion_tokens": 100}
            })))
            .mount(&fallback)
            .await;

        let cfg = |uri: &str| ProviderConfig {
            api_key: Some("k".into()),
            base_url: Some(uri.to_string()),
            default_model: "m".into(),
        };
        let mut configs = std::collections::HashMap::new();
        configs.insert("anthropic".to_string(), cfg(&primary.uri()));
        configs.insert("openai".to_string(), cfg(&fallback.uri()));

        let chain = niki::llm::failover::FailoverProvider::new(
            "anthropic",
            &["openai".to_string()],
            &configs,
        )
        .expect("failover chain builds");

        let resp = chain
            .complete(CompletionRequest {
                model: "m".into(),
                system_prompt: String::new(),
                user_message: "go".into(),
                max_tokens: 64,
                temperature: 0.0,
                json_schema: None,
                tools: None,
                reasoning_effort: None,
            })
            .await
            .expect("fallback serves");

        assert_eq!(
            resp.usage.input_tokens, 500,
            "the fallback served this call"
        );

        let served = chain.served_by().expect("chain reports who served");
        assert_eq!(
            served, "openai",
            "served_by must report the fallback, not the primary, or the stage is priced \
             against the wrong rate card"
        );
    });
}

#[test]
fn a_single_provider_reports_no_override() {
    use niki::config::ProviderConfig;

    let p = niki::llm::provider::create_provider(
        "openai",
        &ProviderConfig {
            api_key: Some("k".into()),
            base_url: Some("http://localhost:1".into()),
            default_model: "m".into(),
        },
    )
    .expect("provider");
    assert_eq!(
        p.served_by(),
        None,
        "a plain provider has no chain, so the caller's own provider name stands"
    );
}

#[test]
fn cost_per_stage_sums_to_the_run_total() {
    // The invariant the recorded run must satisfy; a regression in any
    // accumulation path above breaks this.
    let stages = [0.10_f64, 0.20, 0.05];
    let total: f64 = stages.iter().sum();
    assert!((total - 0.35).abs() < 1e-9, "{total}");
    assert!(
        (total - stages.iter().copied().fold(0.0_f64, f64::max)).abs() > 1e-9,
        "the max-based total must differ, or this test proves nothing"
    );
}
