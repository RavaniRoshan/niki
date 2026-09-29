//! A failed run must still say what it cost.
//!
//! Two defects, both found by reading the failure paths rather than the happy
//! ones, and both with the same shape: the money is recorded somewhere the
//! user cannot see it.
//!
//! 1. **The Coder's tool loop was billed only when it worked.** A loop that
//!    explored for a dozen steps and never called `submit_artifact` had spent
//!    a dozen requests, and the pipeline then ran a *second*, full one-shot
//!    call as the fallback. The user paid twice; `task.json` recorded one of
//!    the two. The failure is the expensive case, and it was the case that
//!    recorded nothing.
//!
//! 2. **`--output-format json` reported every failed run as free.** The
//!    success envelope reads the record's real total. The error envelope
//!    hardcoded `0.0` — on the one path where the money has already been spent
//!    and the user most needs the number. A CI script summing `cost_usd`
//!    against a budget gets a clean figure for exactly the runs that went
//!    wrong.
//!
//! The second is a direct test of the envelope's own output. The first is a
//! test of the accounting function, plus a source-level check that the
//! failure paths call it — which is weaker, and is labelled as such below
//! rather than dressed up as behavioural.

use niki::artifacts::types::AgentRole;
use niki::llm::provider::TokenUsage;
use niki::orchestrator::pipeline::record_loop_cost;
use niki::orchestrator::state::StageMetric;
use niki::runtime::tools::LoopOutput;

/// A provider that answers nothing, for the call above. `record_loop_cost`
/// only asks the provider who served it, and an answer of `None` is the
/// ordinary case — no failover happened.
struct Probe;

#[async_trait::async_trait]
impl niki::llm::provider::LlmProvider for Probe {
    fn provider_name(&self) -> &str {
        "probe"
    }
    async fn stream(
        &self,
        _r: niki::llm::provider::CompletionRequest,
    ) -> anyhow::Result<
        std::pin::Pin<
            Box<
                dyn futures::Stream<Item = anyhow::Result<niki::llm::provider::StreamChunk>> + Send,
            >,
        >,
    > {
        unimplemented!("not called")
    }
    async fn complete(
        &self,
        _r: niki::llm::provider::CompletionRequest,
    ) -> anyhow::Result<niki::llm::provider::CompletionResponse> {
        unimplemented!("not called")
    }
}

/// A loop that ran, spent, and produced nothing.
///
/// The shape of the two early returns in `run_coder_tool_loop`: no
/// `submit_artifact` was ever called, and `artifact` is `None`. `usage` is
/// whatever those steps cost, which is the number that used to be dropped.
fn empty_loop(input: u32, output: u32) -> LoopOutput {
    LoopOutput {
        content: "I could not find a way to do that.".to_string(),
        steps: 6,
        tool_calls: vec![
            ("read_file".to_string(), true),
            ("grep".to_string(), true),
            ("bash".to_string(), false),
        ],
        usage: TokenUsage {
            input_tokens: input,
            output_tokens: output,
            cached_input_tokens: 0,
            reasoning_tokens: 0,
        },
        feedback_turns: 1,
        truncated: false,
        artifact: None,
    }
}

#[test]
fn a_loop_that_produced_nothing_is_still_billed() {
    let out = empty_loop(4_000, 600);
    let mut metrics: Vec<StageMetric> = Vec::new();

    record_loop_cost(
        AgentRole::Coder,
        &Probe,
        "openrouter",
        "some/model",
        &out,
        1_234,
        &mut metrics,
    );

    assert_eq!(metrics.len(), 1, "the loop's spend must leave a record");
    let m = &metrics[0];
    assert_eq!(m.role, AgentRole::Coder);
    assert_eq!(
        m.input_tokens, 4_000,
        "input tokens from a failed loop are still tokens that were paid for"
    );
    assert_eq!(m.output_tokens, 600);
    assert_eq!(m.latency_ms, 1_234);
    assert_eq!(
        m.retry_count, 0,
        "the loop is the mechanism, not a retry; RunBudget adds 1 + retry_count, \\
         so charging steps here would double-count against --max-steps"
    );
}

/// A failed run's JSON envelope must carry the money, not zero.
///
/// `error_envelope` hardcoded `cost_usd: 0.0`. Read together with
/// `result_envelope`, which reads the record's real total, the shape of the
/// output said: this run was free, and this run was not.
#[test]
fn a_failed_run_reports_what_it_spent() {
    let env = niki::cli::run::error_envelope(
        None,
        "error",
        "the Reviewer rejected the change",
        None,
        Some((0.4213, 91_000, 7_400)),
    );
    assert_eq!(
        env["cost_usd"], 0.4213,
        "a run that spent money must say so on the failure path too"
    );
    assert_eq!(env["input_tokens"], 91_000);
    assert_eq!(env["output_tokens"], 7_400);
}

/// The inverse, and the reason the test above is not enough: a run that spent
/// nothing must still say zero, and one that failed before any model call has
/// no record to read. A function that always printed a number would pass the
/// first test and be wrong every other time.
#[test]
fn a_run_that_spent_nothing_says_zero() {
    let env = niki::cli::run::error_envelope(None, "error", "no such file", None, None);
    assert_eq!(env["cost_usd"], 0.0);
    assert_eq!(env["input_tokens"], 0);
    assert_eq!(env["output_tokens"], 0);
}

/// Both early returns in `run_coder_tool_loop` must bill before bailing.
///
/// This is a **source-level** check, and it is weaker than the behavioural
/// ones above: it proves the call is there, not that the path is taken. It is
/// here because the alternative is a test that drives a Coder loop to failure
/// through the whole pipeline and asserts on `PipelineResult::metrics` — which
/// is the right test, is not written yet, and is better written when someone
/// can watch it fail before fixing it rather than guessed at remotely.
///
/// The two `return None` sites after the loop call are the defect: the metric
/// push used to sit after them, so every failure discarded the spend. The
/// success path pushes too, and the count here is deliberately "at least
/// three" so the assertion is about the failure paths existing at all.
#[test]
fn the_coder_loop_bills_before_every_bail_out() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/orchestrator/pipeline.rs"),
    )
    .expect("pipeline.rs");

    let start = src
        .find("async fn run_coder_tool_loop(")
        .expect("run_coder_tool_loop exists");
    let body = &src[start..];
    let end = body
        .find("\n/// What to tell the user when the Coder's tool loop")
        .unwrap_or(body.len());
    let body = &body[..end];

    let bails = body.matches("return None;").count();
    let bills = body.matches("record_loop_cost(").count();
    assert_eq!(
        bails, 2,
        "expected the two failure returns (no artifact, invalid artifact); a \\
         third would be an unbilled exit this test has not accounted for"
    );
    assert!(
        bills >= bails,
        "every `return None` in this function must be preceded by a bill. Found \
         {bails} bail-outs and {bills} calls to record_loop_cost — a loop that \
         explored for a dozen steps and produced nothing is the most expensive \\
         case, not the cheapest."
    );
}
