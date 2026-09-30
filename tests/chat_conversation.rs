//! The chat surface must be a chat: the model has to see the conversation it
//! is in, the reply has to arrive while it is being written, and a turn that
//! fails or is cut off has to say so rather than looking like an answer.
//!
//! Every test here fails against the previous implementation, and each failure
//! mode was found by reading the code, not by guessing:
//!
//! * `CompletionRequest` had no history field, and all four providers
//!   hand-wrote a single-element `messages` array. Turn 3 was sent with turns
//!   1 and 2 erased while the transcript scrolled, persisted and resumed — so
//!   the screen showed a conversation the model had never seen.
//! * `reply_text` called `complete()`, a single non-streaming POST, on the one
//!   surface a person waits on most often.
//! * `has_running_stage()` is false in chat because `state.stages` is empty,
//!   so the spinner and the "esc cancel stage" hint never drew.
//! * Every error was rendered as `(offline) LLM error: …` inside an assistant
//!   bubble: a 401 told the user to check their network.
//! * `run_chat` never set `state.cancel`, so Esc painted "Stopping…" while the
//!   request ran to completion.

use std::path::PathBuf;

use niki::config::{NikiConfig, ProviderConfig};
use niki::display::state::AppState;
use niki::display::tui::DisplayEvent;
use niki::llm::provider::{ChatTurn, CompletionRequest, LlmProvider, message_chain};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn cfg(base_url: &str) -> ProviderConfig {
    ProviderConfig {
        api_key: Some("test-key".to_string()),
        base_url: Some(base_url.to_string()),
        default_model: "test-model".to_string(),
    }
}

/// A request that carries a two-turn conversation before the new one.
fn turn_three() -> CompletionRequest {
    CompletionRequest {
        model: "test-model".to_string(),
        system_prompt: "sys".to_string(),
        user_message: "and what is in the Auth module?".to_string(),
        max_tokens: 256,
        temperature: 0.0,
        json_schema: None,
        tools: None,
        reasoning_effort: None,
        history: vec![
            ChatTurn::user("what modules exist?"),
            ChatTurn::assistant("Billing, Auth and Storage."),
        ],
    }
}

#[test]
fn a_turn_carries_the_conversation_that_came_before_it() {
    let req = turn_three();
    let chain = message_chain(&req);

    assert_eq!(chain.len(), 3, "history plus the new turn");
    assert_eq!(chain[0].content, "what modules exist?");
    assert_eq!(chain[0].role, "user");
    assert_eq!(chain[1].content, "Billing, Auth and Storage.");
    assert_eq!(chain[1].role, "assistant");
    assert_eq!(
        chain[2].content, "and what is in the Auth module?",
        "the new turn must be last"
    );
}

#[test]
fn an_agent_stage_still_sends_exactly_one_message() {
    // The pipeline depends on this. A Planner that could see the Coder's later
    // output would not be an independent Planner, which is the property the
    // whole product rests on. `history: Vec::new()` is the single-turn case.
    let req = CompletionRequest {
        user_message: "plan this".to_string(),
        history: Vec::new(),
        ..Default::default()
    };
    let chain = message_chain(&req);
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].role, "user");
}

#[test]
fn a_chain_never_starts_with_an_assistant_turn() {
    // Anthropic rejects a conversation whose first message is not from the user,
    // and a resumed chat whose history was truncated mid-turn can begin with
    // one. Without this, the first turn after a resume is a 400.
    let req = CompletionRequest {
        user_message: "carry on".to_string(),
        history: vec![
            ChatTurn::assistant("orphaned reply from a truncated resume"),
            ChatTurn::user("real question"),
        ],
        ..Default::default()
    };
    let chain = message_chain(&req);
    assert_eq!(chain[0].role, "user", "a 400 here is silent and total");
    assert_eq!(chain[0].content, "real question");
}

#[test]
fn blank_turns_are_dropped_rather_than_sent_as_empty_content() {
    // An empty-content message is rejected by Anthropic, and an empty turn in
    // the transcript is a rendering accident, not something the user said.
    let req = CompletionRequest {
        user_message: "the real question".to_string(),
        history: vec![
            ChatTurn::user("first"),
            ChatTurn::assistant("   "),
            ChatTurn::user("second"),
        ],
        ..Default::default()
    };
    let chain = message_chain(&req);
    assert_eq!(chain.len(), 3, "the blank turn should not be sent");
    assert!(chain.iter().all(|t| !t.content.trim().is_empty()));
}

#[tokio::test]
async fn anthropic_puts_the_whole_conversation_on_the_wire() {
    let server = MockServer::start().await;
    // Match on the opening of turn 1: if history is dropped somewhere between
    // the request struct and the JSON body, this returns 404 and the call fails.
    Mock::given(method("POST"))
        .and(wiremock::matchers::body_string_contains(
            "what modules exist?",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{"type": "text", "text": "Auth holds the login flow."}],
            "usage": {"input_tokens": 40, "output_tokens": 8}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let provider = niki::llm::anthropic::AnthropicProvider::new(&cfg(&server.uri())).unwrap();
    let resp = provider.complete(turn_three()).await.unwrap();
    assert!(resp.content.contains("Auth holds the login flow."));
}

#[tokio::test]
async fn openai_puts_the_whole_conversation_on_the_wire() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(wiremock::matchers::body_string_contains(
            "Billing, Auth and Storage.",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{"message": {"role": "assistant", "content": "Auth holds login."},
                         "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 40, "completion_tokens": 8}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let provider =
        niki::llm::openai::OpenAiProvider::new_named(&cfg(&server.uri()), "test").unwrap();
    let resp = provider.complete(turn_three()).await.unwrap();
    assert!(resp.content.contains("Auth holds login."));
}

#[tokio::test]
async fn google_names_the_assistant_turn_the_way_google_does() {
    // Google's API calls the assistant role "model". Sending "assistant" is
    // accepted-but-wrong on some endpoints and rejected on others, and the
    // failure names nothing useful.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(wiremock::matchers::body_string_contains("\"model\""))
        .and(wiremock::matchers::body_string_contains(
            "what modules exist?",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "candidates": [{"content": {"parts": [{"text": "Auth holds login."}]},
                            "finishReason": "STOP"}]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let provider = niki::llm::google::GoogleProvider::new(&cfg(&server.uri())).unwrap();
    let resp = provider.complete(turn_three()).await.unwrap();
    assert!(resp.content.contains("Auth holds login."));
}

// ── The state machine a streaming turn depends on ─────────────────────

fn state() -> AppState {
    AppState::new(
        "chat session".to_string(),
        NikiConfig::default(),
        PathBuf::from("."),
    )
}

#[test]
fn streamed_fragments_become_exactly_one_committed_turn() {
    let mut s = state();
    s.apply_display_event(DisplayEvent::ChatMessage {
        role: "user".to_string(),
        text: "hello".to_string(),
    });
    s.apply_display_event(DisplayEvent::ChatPending);
    assert!(s.chat_pending, "the surface must show it is working");

    for frag in ["Hi", ", ", "how can I", " help?"] {
        s.apply_display_event(DisplayEvent::ChatDelta {
            text: frag.to_string(),
        });
    }
    // Nothing is in the transcript yet: a partial reply must never be
    // mistakable for a finished one.
    assert_eq!(s.chat_log.len(), 1, "only the user turn so far");
    assert_eq!(s.chat_stream, "Hi, how can I help?");

    s.apply_display_event(DisplayEvent::ChatFinished {
        finish_reason: Some("end_turn".to_string()),
    });
    assert_eq!(s.chat_log.len(), 2, "the stream commits as one turn");
    assert_eq!(s.chat_log[1].0, "assistant");
    assert_eq!(s.chat_log[1].1, "Hi, how can I help?");
    assert!(!s.chat_pending);
    assert!(!s.chat_truncated);
    assert!(s.chat_stream.is_empty());
}

#[test]
fn a_reply_cut_off_at_the_token_limit_is_flagged_as_incomplete() {
    // This is the misdiagnosis `StreamChunk::Finish` exists to prevent on the
    // pipeline path. Chat parsed the provider's stop reason and threw it away,
    // so a cut-off reply was presented as a finished answer.
    let mut s = state();
    s.apply_display_event(DisplayEvent::ChatDelta {
        text: "Here is the first half".to_string(),
    });
    s.apply_display_event(DisplayEvent::ChatFinished {
        finish_reason: Some("max_tokens".to_string()),
    });
    assert!(
        s.chat_truncated,
        "a reply stopped by the token limit is not a finished answer"
    );

    for reason in ["length", "MAX_TOKENS", "max_tokens"] {
        let mut s = state();
        s.apply_display_event(DisplayEvent::ChatFinished {
            finish_reason: Some(reason.to_string()),
        });
        assert!(s.chat_truncated, "{reason} is a truncation, not a stop");
    }
}

#[test]
fn a_failed_turn_is_not_recorded_as_something_the_model_said() {
    // The old code pushed `(offline) LLM error: HTTP 401 …` into the transcript
    // as an assistant turn, so a wrong API key was displayed in the same
    // bubble style as a real answer, labelled as a network problem.
    let mut s = state();
    s.apply_display_event(DisplayEvent::ChatPending);
    s.apply_display_event(DisplayEvent::ChatError {
        message: "Authentication failed. Your API key was rejected.".to_string(),
        cancelled: false,
    });
    assert_eq!(s.chat_log.len(), 1);
    assert_eq!(
        s.chat_log[0].0, "error",
        "a failure is not a turn the assistant took"
    );
    assert!(s.chat_log[0].1.contains("Authentication failed"));
    assert!(!s.chat_pending, "the spinner must stop on failure");
}

#[test]
fn a_cancelled_turn_says_so_rather_than_reporting_a_failure() {
    let mut s = state();
    s.apply_display_event(DisplayEvent::ChatError {
        message: "Cancelled — the request was stopped before it finished.".to_string(),
        cancelled: true,
    });
    assert_eq!(s.chat_log[0].0, "cancelled");
}

#[test]
fn an_empty_reply_is_labelled_instead_of_rendering_nothing() {
    // `"".lines()` yields no lines, so the renderer emitted no header at all:
    // a blank gap the user could not tell from "the model has not replied yet".
    let mut s = state();
    s.apply_display_event(DisplayEvent::ChatMessage {
        role: "assistant".to_string(),
        text: String::new(),
    });
    assert_eq!(s.chat_log.len(), 1);
    assert!(
        s.chat_log[0].1.contains("empty response"),
        "an empty turn must be visible as a turn"
    );
}

// ── The transcript renders markdown ───────────────────────────────────

fn lines_for(state: &AppState) -> Vec<String> {
    niki::display::pages::chat::build_chat_lines(state, 80, false)
        .into_iter()
        .map(|l| l.text)
        .collect()
}

#[test]
fn an_assistant_reply_renders_markdown_rather_than_its_source() {
    // `src/display/chat/` is 1,367 lines of tested pulldown-cmark rendering
    // that was unreachable from the conversation: `build_chat_lines` used
    // `text.lines()`, so a fenced block appeared literally, with its backticks,
    // on the one surface where a coding assistant's output is read.
    let mut s = state();
    s.apply_display_event(DisplayEvent::ChatMessage {
        role: "user".to_string(),
        text: "show me the fix".to_string(),
    });
    s.apply_display_event(DisplayEvent::ChatDelta {
        text: "Here it is:\n\n```rust\nfn main() {}\n```\n\nThat is the whole change.".to_string(),
    });
    s.apply_display_event(DisplayEvent::ChatFinished {
        finish_reason: None,
    });

    let lines = lines_for(&s);
    let joined = lines.join("\n");
    assert!(
        !joined.contains("```"),
        "a fenced block must not show its fences:\n{joined}"
    );
    assert!(
        joined.contains("fn main()"),
        "the code must survive:\n{joined}"
    );
    assert!(
        lines.iter().any(|l| l.contains("assistant")),
        "the turn must still be attributable:\n{joined}"
    );
}

#[test]
fn an_error_turn_is_left_verbatim_rather_than_rendered_as_markdown() {
    // Error text is a diagnostic, not prose. It has to survive into a copy and
    // a bug report exactly as written, so it is not run through the markdown
    // engine the way an assistant turn is.
    let mut s = state();
    s.apply_display_event(DisplayEvent::ChatError {
        message: "Set ANTHROPIC_API_KEY, or run `niki auth login`.".to_string(),
        cancelled: false,
    });
    let joined = lines_for(&s).join("\n");
    assert!(
        joined.contains("`niki auth login`"),
        "backticks must survive verbatim in a diagnostic:\n{joined}"
    );
}

#[test]
fn the_rendered_line_map_is_the_map_the_pointer_acts_on() {
    // `build_chat_lines` is the single source for both what is drawn and what
    // `state.chat_lines` holds for the click/drag/copy hit-test. Rendering the
    // transcript through the markdown engine changes the row count, so the
    // property that has to hold is that the drawn rows and the stored rows are
    // produced by the same call — not that any particular row exists.
    let mut s = state();
    for i in 0..4 {
        s.apply_display_event(DisplayEvent::ChatMessage {
            role: if i % 2 == 0 { "user" } else { "assistant" }.to_string(),
            text: format!("turn {i}\n\n```rust\nlet x = {i};\n```"),
        });
    }
    let first = lines_for(&s);
    let second = lines_for(&s);
    assert_eq!(first, second, "the line map must be stable across calls");

    // Every addressable row points at a real message.
    for l in niki::display::pages::chat::build_chat_lines(&s, 80, false) {
        if l.msg_index != usize::MAX {
            assert!(
                l.msg_index < s.chat_log.len(),
                "row addresses message {} but only {} exist",
                l.msg_index,
                s.chat_log.len()
            );
        }
    }
}
