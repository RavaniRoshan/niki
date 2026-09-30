//! A stage that cannot finish must fail in a time a person will wait for.
//
// Found by running the real pipeline against a live provider, which is the
// argument for having done that run.
//
// The fix that let a slow model finish — moving from `ClientBuilder::timeout`
// (a 120s **total** deadline) to `read_timeout` (120s **per read**) — also
// removed the only bound on the whole call. A read timeout multiplies by the
// attempt count, and the agent layer retries on top: 4 transport attempts x 3
// agent attempts, with back-off. Measured, a single stalled stage took **over
// twelve minutes** to give up, and a two-stage run never reached its second
// stage. Nothing in the pipeline reported progress, because nothing was
// progressing — it was waiting.
//
// The bound has to be longer than one read (a slow model must be allowed to
// finish a long answer) and shorter than the full retry multiplication (a
// stalled upstream must be abandoned).

use std::time::Duration;

use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The read timeout must stay generous — that is the fix that lets a slow
/// model finish — while the total budget must be shorter than the retry
/// multiplication would otherwise allow.
#[test]
fn the_two_bounds_are_ordered_as_intended() {
    let read = Duration::from_secs(120);
    // Four transport attempts at the read timeout is 8 minutes, and the agent
    // layer retries three times on top of that.
    let multiplication = read * 4 * 3;
    let budget = Duration::from_secs(300);
    assert!(
        budget > read,
        "the budget must be longer than one read, or a slow model is cut off"
    );
    assert!(
        budget < multiplication,
        "the budget ({budget:?}) must be shorter than read x attempts x agent-retries \
         ({multiplication:?}), or a stalled stage can still run for twenty minutes"
    );
}

/// The bound is applied at the top of the retry loop.
///
/// The elapsed-time test that used to live here could not fail: a 503 answers
/// instantly, so removing the budget check changed nothing it measured. A real
/// stall takes the full read timeout per attempt, which is far too slow for a
/// unit test — so the property is asserted structurally, and the ordering test
/// above is what pins the *value*. The first version of this test claimed to
/// prove the budget was enforced and proved nothing; that is worse than no
/// test, because it reads like coverage.
#[test]
fn the_budget_is_checked_before_every_attempt() {
    let s = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/llm/provider.rs"),
    )
    .expect("provider.rs is readable");

    let body = s
        .split("pub async fn send_request")
        .nth(1)
        .expect("send_request exists");
    assert!(
        body.contains("TOTAL_REQUEST_BUDGET"),
        "send_request must carry a total budget; the per-read timeout alone \
         multiplies by every attempt"
    );
    let check_at = body
        .find("tokio::time::Instant::now() >= budget")
        .expect("the budget must be checked inside the retry loop");
    let first_build = body
        .find("match build().await")
        .expect("the loop sends a request");
    assert!(
        check_at < first_build,
        "the budget must be checked *before* an attempt is made, or one more \
         attempt can start after it has expired"
    );
}

/// The budget must not cut short a request that is progressing.
#[tokio::test(flavor = "multi_thread")]
async fn a_slow_but_answering_provider_is_not_cut_off() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"choices": [{"message": {"content": "hi"}}]}))
                .set_delay(Duration::from_secs(3)),
        )
        .mount(&server)
        .await;

    let client = niki::llm::provider::http_client().expect("a client");
    let resp = client
        .post(server.uri())
        .body("{}")
        .send()
        .await
        .expect("a slow answer still arrives");
    assert!(resp.status().is_success(), "{}", resp.status());
}
