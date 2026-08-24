//! Integration tests for the ACP server (`niki acp`).
//!
//! Spawns the real binary and feeds it JSON-RPC 2.0 requests over stdin,
//! asserting on the responses written to stdout. Only the framing and the
//! request-validation paths are exercised here (initialize, empty-prompt
//! rejection, unknown-method, malformed JSON) - a full `prompt/send` run
//! needs a live LLM provider and is covered by the pipeline integration tests.

use assert_cmd::Command;
use serde_json::Value;

fn acp_response(stdout: &str, id: i64) -> Value {
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(line)
            && v.get("id") == Some(&Value::from(id))
        {
            return v;
        }
    }
    panic!("no response with id={} in stdout:\n{}", id, stdout);
}

fn run_acp(stdin: &str) -> String {
    let tmp = tempfile::tempdir().unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_niki"));
    cmd.arg("acp").arg("--project").arg(tmp.path());
    cmd.write_stdin(stdin).unwrap();
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "niki acp failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn acp_initialize_advertises_capabilities() {
    let stdout = run_acp(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#);
    let resp = acp_response(&stdout, 1);
    assert_eq!(resp["jsonrpc"], "2.0");
    assert!(resp["result"].is_object(), "expected a result object");
    assert!(
        resp["result"]["capabilities"].is_object(),
        "initialize must advertise capabilities"
    );
}

#[test]
fn acp_rejects_empty_prompt() {
    let stdout =
        run_acp(r#"{"jsonrpc":"2.0","id":2,"method":"prompt/send","params":{"prompt":""}}"#);
    let resp = acp_response(&stdout, 2);
    assert!(resp["error"].is_object(), "expected an error response");
    assert_eq!(resp["error"]["code"], -32602);
}

#[test]
fn acp_unknown_method_returns_method_not_found() {
    let stdout = run_acp(r#"{"jsonrpc":"2.0","id":3,"method":"does/not/exist","params":{}}"#);
    let resp = acp_response(&stdout, 3);
    assert_eq!(resp["error"]["code"], -32601);
}

#[test]
fn acp_malformed_json_returns_parse_error() {
    let stdout = run_acp("this is not json\n");
    let resp = stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l.trim()).ok())
        .find(|v| v["error"].is_object())
        .expect("expected a parse-error response");
    assert_eq!(resp["error"]["code"], -32700);
}

#[test]
fn acp_session_status_reports_idle() {
    let stdout = run_acp(r#"{"jsonrpc":"2.0","id":4,"method":"session/status","params":{}}"#);
    let resp = acp_response(&stdout, 4);
    assert_eq!(resp["result"]["status"], "idle");
}
