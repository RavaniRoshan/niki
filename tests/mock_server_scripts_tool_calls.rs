//! The mock LLM can be told which tool calls to make, in order.
//!
//! Until batch 7 the loop's script was two hardcoded calls: read a file, then
//! submit. So no test could drive any *other* tool — and `ask_user` and
//! `approval` were unreachable from an end-to-end leg. Their unit tests
//! exercise the adapter and the modal separately; nothing exercised the two
//! together through a real run, which is the only place a modal can be wrong
//! in a way no unit test sees. A key swallowed by the ladder. A question the
//! loop never gets to. A modal that opens behind another overlay.
//!
//! So the server takes an explicit `tool_calls` sequence, and this drives it
//! over a real socket rather than asserting on its source — a server that
//! *reads* a script and one that *honours* it are different claims, and only
//! the second is worth anything.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// A free-ish port. Chosen per run from the OS range rather than a constant,
/// so two of these can run at once — which `cargo test` does by default, and
/// which is how a fixed port turns a passing suite red on a busy machine.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("a port")
        .local_addr()
        .expect("addr")
        .port()
}

struct Mock {
    child: Child,
    port: u16,
    _script: tempfile::NamedTempFile,
}

impl Drop for Mock {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start(tool_calls: serde_json::Value) -> Mock {
    let script = serde_json::json!({ "tool_calls": tool_calls });
    let mut f = tempfile::NamedTempFile::new().expect("tempfile");
    serde_json::to_writer(&mut f, &script).expect("write script");
    f.flush().expect("flush");

    let port = free_port();
    let child = Command::new("python3")
        .arg(mock_path())
        .env("MOCK_LLM_SCRIPT", f.path())
        .env("MOCK_LLM_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect(
            "the mock server must start — it is the harness every \
                 end-to-end leg runs against",
        );

    // Own the child **before** waiting on it. The first version polled first
    // and only wrapped the child in `Mock` on success, so the timeout path —
    // the one that runs when something is actually wrong — leaked a python
    // process. `Mock`'s `Drop` kills it and a panic unwinds locals, so owning
    // it first is what makes the failure path clean. Clippy's
    // `zombie_processes` is right about this, and CI would have caught it
    // after the commit rather than before it.
    let mock = Mock {
        child,
        port,
        _script: f,
    };

    // Wait for the socket rather than sleeping a fixed amount: a machine under
    // load makes a sleep both slow and unreliable, in opposite directions.
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return mock;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("the mock server never listened on port {port}");
}

fn mock_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/integration/mock_llm.py")
}

/// One OpenAI-shaped turn. `results` is how many `tool` turns the conversation
/// already carries, which is how the server knows where in the sequence it is.
fn turn(results: usize) -> serde_json::Value {
    let mut messages = vec![serde_json::json!({"role": "system", "content": "s"})];
    for i in 0..results {
        messages.push(serde_json::json!({
            "role": "assistant", "content": null,
            "tool_calls": [{"id": format!("c{i}"), "type": "function",
                            "function": {"name": "prev", "arguments": "{}"}}],
        }));
        messages.push(serde_json::json!({"role": "tool", "content": format!("r{i}")}));
    }
    serde_json::json!({
        "model": "mock-model", "role": "coder", "messages": messages,
        "tools": [{"type": "function", "function": {"name": "ask_user", "parameters": {}}}],
    })
}

fn ask(port: u16, body: &serde_json::Value) -> serde_json::Value {
    let payload = serde_json::to_vec(body).expect("json");
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("timeout");
    let head = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nHost: 127.0.0.1\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n",
        payload.len()
    );
    stream.write_all(head.as_bytes()).expect("write head");
    stream.write_all(&payload).expect("write body");
    stream.flush().expect("flush");

    let mut raw = String::new();
    stream.read_to_string(&mut raw).expect("read");
    let body = raw.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or(&raw);
    serde_json::from_str(body.trim())
        .unwrap_or_else(|e| panic!("the server did not answer with JSON ({e}); it said: {raw}"))
}

/// The name of the call the server asked for on this turn.
fn called_name(reply: &serde_json::Value) -> Option<String> {
    reply["choices"][0]["message"]["tool_calls"][0]["function"]["name"]
        .as_str()
        .map(str::to_string)
}

fn called_arguments(reply: &serde_json::Value) -> serde_json::Value {
    let raw = reply["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"]
        .as_str()
        .unwrap_or("{}");
    serde_json::from_str(raw).unwrap_or(serde_json::Value::Null)
}

/// A scripted sequence is followed **in order**, and the arguments survive.
///
/// `ask_user` first with its question intact, then `submit_artifact`. The
/// arguments are the point: a server that called `ask_user` with `{}` would
/// open a modal asking the user nothing, and every test downstream would still
/// pass.
#[test]
fn a_scripted_tool_call_sequence_is_followed_in_order() {
    let mock = start(serde_json::json!([
        {"name": "ask_user",
         "arguments": {"question": "Which database?", "options": ["sqlite", "postgres"]}},
        {"name": "submit_artifact"},
    ]));

    let first = ask(mock.port, &turn(0));
    assert_eq!(
        called_name(&first).as_deref(),
        Some("ask_user"),
        "turn 0 must call the first scripted tool, got {first}"
    );
    let args = called_arguments(&first);
    assert_eq!(
        args["question"], "Which database?",
        "the question must survive the round trip, or the modal asks the user \
         nothing: {args}"
    );
    assert_eq!(
        args["options"][1], "postgres",
        "and so must the choice list: {args}"
    );

    let second = ask(mock.port, &turn(1));
    assert_eq!(
        called_name(&second).as_deref(),
        Some("submit_artifact"),
        "turn 1 must call the second scripted tool, not repeat the first. A \
         sequence that replays is worse than no sequence: the loop burns its \
         budget asking the same question. Got {second}"
    );
}

/// The same script, on the **Anthropic** wire format.
///
/// This is the half that was broken. `anthropic_json_response` referenced
/// `scripted` without ever computing it, so the first scripted call raised
/// `NameError` and the handler died — taking the whole server's connection
/// with it. The end-to-end leg saw `connection closed before message
/// completed`; the mock's own stderr had the traceback, and nothing in the
/// suite looked. Only the OpenAI path had a test.
///
/// So a feature scripted on one provider and run on the other is tested on
/// both, and the crash is the failure this pins: before the fix the second
/// turn raised and the assert never ran.
#[test]
fn a_scripted_sequence_works_on_the_anthropic_format_too() {
    let mock = start(serde_json::json!([
        {"name": "ask_user", "arguments": {"question": "Which greeting?"}},
        {"name": "submit_artifact"},
    ]));

    let first = ask_messages(mock.port, 0);
    assert_eq!(
        first["content"][0]["name"], "ask_user",
        "turn 0 must call the first scripted tool, got {first}"
    );
    assert_eq!(
        first["content"][0]["input"]["question"], "Which greeting?",
        "and the question must survive: {first}"
    );
    let second = ask_messages(mock.port, 1);
    assert_eq!(
        second["content"][0]["name"], "submit_artifact",
        "turn 1 must call the second scripted tool: {second}"
    );
}

/// One turn of the Anthropic conversation, with `results` `tool_result` blocks
/// already in it.
fn ask_messages(port: u16, results: usize) -> serde_json::Value {
    let mut messages = vec![serde_json::json!({"role": "user", "content": "go"})];
    for i in 0..results {
        messages.push(serde_json::json!({
            "role": "assistant",
            "content": [{"type": "tool_use", "id": format!("t{i}"),
                         "name": "ask_user", "input": {"question": "q"}}],
        }));
        messages.push(serde_json::json!({
            "role": "user",
            "content": [{"type": "tool_result", "tool_use_id": format!("t{i}"),
                         "content": "an answer"}],
        }));
    }
    let body = serde_json::json!({
        "model": "mock-model", "max_tokens": 100, "messages": messages,
        "tools": [{"name": "ask_user", "description": "d",
                   "input_schema": {"type": "object"}}],
    });
    let payload = serde_json::to_vec(&body).expect("json");
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("timeout");
    let head = format!(
        "POST /v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\n\
         Content-Type: application/json\r\nanthropic-version: 2023-06-01\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        payload.len()
    );
    stream.write_all(head.as_bytes()).expect("write head");
    stream.write_all(&payload).expect("write body");
    stream.flush().expect("flush");

    let mut raw = String::new();
    // A handler that raised mid-request closes the connection with no response
    // at all, which is what the `NameError` looked like from the client. The
    // read error *is* the finding, so it is reported as one.
    stream
        .read_to_string(&mut raw)
        .unwrap_or_else(|e| panic!("the Anthropic path closed without answering: {e}"));
    let body = raw.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or(&raw);
    serde_json::from_str(body.trim())
        .unwrap_or_else(|e| panic!("the server did not answer with JSON ({e}); it said: {raw}"))
}

/// The sequence is bounded. Past the end the server must stop calling tools,
/// or a loop with a step budget walks off the end of a list.
#[test]
fn the_script_runs_out_rather_than_repeating() {
    let mock = start(serde_json::json!([{"name": "ask_user", "arguments": {"question": "q"}}]));
    // Three turns, one scripted call. The second and third must not be the
    // question again.
    let names: Vec<Option<String>> = (0..3)
        .map(|i| called_name(&ask(mock.port, &turn(i))))
        .collect();
    assert_eq!(
        names[0].as_deref(),
        Some("ask_user"),
        "the first turn is the scripted call: {names:?}"
    );
    assert_eq!(
        names
            .iter()
            .filter(|n| n.as_deref() == Some("ask_user"))
            .count(),
        1,
        "a one-call script must produce exactly one call across three turns, \
         not repeat: {names:?}"
    );
}

/// No script, no override — the built-in story must be untouched.
///
/// The scripted path is an opt-in; a server that always consulted
/// `SCRIPTED_TOOL_CALLS` would change every existing consumer at once, on a
/// change whose subject is a new capability. That is the same trap the
/// existing `tool_loop` opt-in documents.
#[test]
fn with_no_script_the_built_in_story_is_untouched() {
    let script = tempfile::NamedTempFile::new().expect("tempfile");
    std::fs::write(script.path(), b"{}").expect("write");
    let port = free_port();
    let mut child = Command::new("python3")
        .arg(mock_path())
        .env("MOCK_LLM_SCRIPT", script.path())
        .env("MOCK_LLM_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline && TcpStream::connect(("127.0.0.1", port)).is_err() {
        std::thread::sleep(Duration::from_millis(50));
    }

    let reply = ask(port, &turn(0));
    // With no script and `tool_loop` unset, the server answers with its
    // long-standing probe. Asserting the *absence* of `ask_user` is the claim:
    // an empty `tool_calls` script must not be read as "call nothing ever".
    assert_ne!(
        called_name(&reply).as_deref(),
        Some("ask_user"),
        "an empty script must fall back to the built-in story, not invent a \
         question: {reply}"
    );
    let _ = child.kill();
    let _ = child.wait();
}
