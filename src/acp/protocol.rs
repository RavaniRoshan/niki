//! Agent Client Protocol (ACP) - minimal JSON-RPC framing + typed messages.
//!
//! ACP is a JSON-RPC 2.0 protocol over stdio (or any byte stream) that lets an
//! IDE / editor drive an agent. NIKI exposes its pipeline through it so editors
//! like Zed and Claude Code can invoke NIKI directly.
//!
//! This module is intentionally minimal: it provides the wire framing
//! (request/response/notification envelopes) and the typed message shapes
//! NIKI cares about (`initialize`, `prompt/send`, `session/status`,
//! `session/cancel`). It does not implement the full ACP schema - only the
//! subset NIKI's CLI `acp` command needs.

use serde::Deserialize;

/// A JSON-RPC 2.0 request.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

/// A JSON-RPC 2.0 response.
#[derive(Debug, Clone, serde::Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

impl JsonRpcResponse {
    pub fn ok(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: serde_json::Value, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError { code, message }),
        }
    }
}

/// A JSON-RPC 2.0 notification (no id).
#[derive(Debug, Clone, serde::Serialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: &'static str,
    pub method: String,
    pub params: serde_json::Value,
}

impl JsonRpcNotification {
    pub fn new(method: &str, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0",
            method: method.to_string(),
            params,
        }
    }
}

/// `initialize` request params.
#[derive(Debug, Clone, Deserialize)]
pub struct InitializeParams {
    pub client_info: Option<serde_json::Value>,
    pub capabilities: Option<serde_json::Value>,
}

/// `prompt/send` request params - the core ACP method.
#[derive(Debug, Clone, Deserialize)]
pub struct PromptParams {
    pub prompt: String,
    #[serde(default)]
    pub session_id: Option<String>,
}

/// `session/status` request params.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SessionParams {
    pub session_id: Option<String>,
}

/// `session/cancel` request params.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct CancelParams {
    pub session_id: Option<String>,
}

/// Read one JSON-RPC message from a byte stream.
///
/// ACP frames messages as newline-delimited JSON. Returns Ok(None) at EOF.
pub fn read_message<R: std::io::BufRead>(reader: &mut R) -> std::io::Result<Option<String>> {
    let mut line = String::new();
    let n = reader.read_line(&mut line)?;
    if n == 0 {
        return Ok(None);
    }
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return read_message(reader);
    }
    Ok(Some(trimmed.to_string()))
}

/// Write a JSON-RPC message to a writer, newline-delimited.
pub fn write_message<W: std::io::Write>(
    writer: &mut W,
    value: &serde_json::Value,
) -> std::io::Result<()> {
    writeln!(writer, "{}", value)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_initialize_request() {
        let raw = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"zed","version":"0.140"}}}"#;
        let req: JsonRpcRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(req.method, "initialize");
        assert_eq!(req.id, Some(serde_json::json!(1)));
    }

    #[test]
    fn parse_prompt_send_request() {
        let raw = r#"{"jsonrpc":"2.0","id":2,"method":"prompt/send","params":{"prompt":"add a health endpoint"}}"#;
        let req: JsonRpcRequest = serde_json::from_str(raw).unwrap();
        let params: PromptParams = serde_json::from_value(req.params.unwrap()).unwrap();
        assert_eq!(params.prompt, "add a health endpoint");
    }

    #[test]
    fn response_serializes_without_error_when_ok() {
        let resp = JsonRpcResponse::ok(serde_json::json!(1), serde_json::json!({"ok": true}));
        let s = serde_json::to_string(&resp).unwrap();
        assert!(s.contains(r#""result":{"ok":true}"#));
        assert!(!s.contains(r#""error""#));
    }

    #[test]
    fn response_serializes_error_when_failed() {
        let resp =
            JsonRpcResponse::err(serde_json::json!(2), -32600, "method not found".to_string());
        let s = serde_json::to_string(&resp).unwrap();
        assert!(s.contains(r#""error":{"code":-32600,"message":"method not found"}"#));
    }
}
