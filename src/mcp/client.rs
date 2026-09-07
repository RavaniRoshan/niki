use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

use super::{McpServerConfig, McpServerType, McpTool};

/// JSON-RPC 2.0 request
#[derive(Debug, Serialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 response
#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[serde(rename = "jsonrpc")]
    _jsonrpc: String,
    id: u64,
    #[serde(default)]
    result: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 error
#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

/// Transport behind an [`McpConnection`]: child-process STDIO or
/// Streamable-HTTP remote (plain JSON responses and SSE `data:` streams).
enum McpTransport {
    // Boxed: the child-process handles dwarf the HTTP fields, and clippy's
    // large_enum_variant lint is right that this would waste space inline.
    Stdio(Box<McpTransportStdio>),
    Http {
        client: reqwest::Client,
        url: String,
        headers: HashMap<String, String>,
        session_id: Option<String>,
    },
}

/// Child-process handles for STDIO transport (boxed inside [`McpTransport`]).
struct McpTransportStdio {
    child: Child,
    stdin: tokio::process::ChildStdin,
    stdout_lines: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
}

/// A connection to an MCP server (STDIO or Streamable-HTTP transport).
pub struct McpConnection {
    name: String,
    transport: McpTransport,
    request_id: u64,
    timeout_ms: u64,
}

impl McpConnection {
    /// Connect to an MCP server via STDIO transport.
    pub async fn connect_stdio(
        name: &str,
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
        timeout_ms: u64,
    ) -> Result<Self> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        for (k, v) in env {
            cmd.env(k, v);
        }

        let mut child = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn MCP server '{name}': {command}"))?;

        let stdin = child
            .stdin
            .take()
            .context("MCP server stdin not available")?;
        let stdout = child
            .stdout
            .take()
            .context("MCP server stdout not available")?;
        let stdout_lines = BufReader::new(stdout).lines();

        let mut conn = Self {
            name: name.to_string(),
            transport: McpTransport::Stdio(Box::new(McpTransportStdio {
                child,
                stdin,
                stdout_lines,
            })),
            request_id: 0,
            timeout_ms,
        };

        // Initialize the connection
        conn.initialize().await?;

        Ok(conn)
    }

    /// Connect to an MCP server via Streamable HTTP (remote URL).
    ///
    /// Sends JSON-RPC over HTTP POST with `Accept: application/json,
    /// text/event-stream` and understands both plain-JSON and SSE responses,
    /// per the Streamable HTTP transport. Session affinity follows the
    /// `mcp-session-id` response header when the server issues one.
    pub async fn connect_http(
        name: &str,
        url: &str,
        headers: &HashMap<String, String>,
        timeout_ms: u64,
    ) -> Result<Self> {
        let mut conn = Self {
            name: name.to_string(),
            transport: McpTransport::Http {
                client: reqwest::Client::new(),
                url: url.to_string(),
                headers: headers.clone(),
                session_id: None,
            },
            request_id: 0,
            timeout_ms,
        };

        // Initialize the connection
        conn.initialize().await?;

        Ok(conn)
    }

    /// Send an initialize request and wait for response.
    async fn initialize(&mut self) -> Result<()> {
        let params = serde_json::json!({
            "protocolVersion": "2026-07-28",
            "capabilities": {
                "tools": { "listChanged": false }
            },
            "clientInfo": {
                "name": "niki",
                "version": env!("CARGO_PKG_VERSION")
            }
        });

        let response: serde_json::Value = self
            .send_request("initialize", Some(params))
            .await
            .context("MCP initialize failed")?;

        tracing::info!(
            "MCP server '{}' initialized: {}",
            self.name,
            response
                .get("serverInfo")
                .and_then(|i| i.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
        );

        // Send initialized notification
        self.send_notification("notifications/initialized", None)
            .await?;

        Ok(())
    }

    /// List tools available on the server.
    pub async fn list_tools(&mut self) -> Result<Vec<McpTool>> {
        let response: serde_json::Value = self
            .send_request("tools/list", None)
            .await
            .context("MCP tools/list failed")?;

        let tools = response
            .get("tools")
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| {
                        Some(McpTool {
                            name: t.get("name")?.as_str()?.to_string(),
                            description: t
                                .get("description")
                                .and_then(|d| d.as_str())
                                .unwrap_or("")
                                .to_string(),
                            server_name: self.name.clone(),
                            input_schema: t.get("inputSchema").cloned(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(tools)
    }

    /// Call a tool on the server.
    pub async fn call_tool(
        &mut self,
        tool_name: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "name": tool_name,
            "arguments": arguments.unwrap_or(serde_json::json!({}))
        });

        self.send_request("tools/call", Some(params))
            .await
            .with_context(|| format!("MCP tools/call '{tool_name}' failed"))
    }

    /// Send a JSON-RPC request and wait for the response.
    async fn send_request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        self.request_id += 1;
        let id = self.request_id;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params,
        };

        let timeout = std::time::Duration::from_millis(self.timeout_ms);
        let response = match &mut self.transport {
            McpTransport::Stdio(stdio) => {
                let McpTransportStdio {
                    stdin,
                    stdout_lines,
                    ..
                } = stdio.as_mut();
                let mut msg = serde_json::to_string(&request)?;
                msg.push('\n');

                stdin
                    .write_all(msg.as_bytes())
                    .await
                    .context("Failed to write to MCP server stdin")?;

                // Wait for response with timeout
                tokio::time::timeout(timeout, Self::read_response_stdio(stdout_lines, id))
                    .await
                    .context("MCP request timed out")??
            }
            McpTransport::Http {
                client,
                url,
                headers,
                session_id,
            } => tokio::time::timeout(
                timeout,
                Self::post_json_rpc(client, url, headers, session_id, &request),
            )
            .await
            .context("MCP request timed out")??,
        };

        if let Some(error) = response.error {
            return Err(anyhow::anyhow!(
                "MCP error {}: {}",
                error.code,
                error.message
            ));
        }

        Ok(response.result.unwrap_or(serde_json::Value::Null))
    }

    /// POST one JSON-RPC message to a Streamable-HTTP server and parse the
    /// reply, accepting either a bare JSON body or an SSE `data:` stream.
    /// Updates `session_id` from the `mcp-session-id` response header.
    async fn post_json_rpc(
        client: &reqwest::Client,
        url: &str,
        headers: &HashMap<String, String>,
        session_id: &mut Option<String>,
        request: &JsonRpcRequest,
    ) -> Result<JsonRpcResponse> {
        let mut req = client
            .post(url)
            .header("Accept", "application/json, text/event-stream")
            .header("Content-Type", "application/json")
            .json(request);
        for (k, v) in headers {
            req = req.header(k.as_str(), v.as_str());
        }
        if let Some(sid) = session_id {
            req = req.header("mcp-session-id", sid.as_str());
        }

        let resp = req.send().await.context("MCP HTTP request failed")?;
        let status = resp.status();
        if let Some(sid) = resp
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
        {
            *session_id = Some(sid.to_string());
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "MCP HTTP error {}: {}",
                status,
                body.chars().take(300).collect::<String>()
            ));
        }
        let text = resp.text().await.context("MCP HTTP body unreadable")?;
        Self::parse_http_response(&text)
    }

    /// Parse a Streamable-HTTP reply: a bare JSON-RPC object, or the last
    /// parseable `data:` payload of an SSE stream.
    fn parse_http_response(text: &str) -> Result<JsonRpcResponse> {
        let trimmed = text.trim();
        if let Ok(resp) = serde_json::from_str::<JsonRpcResponse>(trimmed) {
            return Ok(resp);
        }
        let mut last: Option<JsonRpcResponse> = None;
        for line in trimmed.lines() {
            let line = line.trim();
            if let Some(data) = line.strip_prefix("data:") {
                let data = data.trim();
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }
                if let Ok(resp) = serde_json::from_str::<JsonRpcResponse>(data) {
                    last = Some(resp);
                }
            }
        }
        last.context(format!(
            "MCP HTTP reply was neither JSON-RPC nor an SSE data stream: {:?}",
            trimmed.chars().take(300).collect::<String>()
        ))
    }

    /// Send a JSON-RPC notification (no response expected).
    async fn send_notification(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<()> {
        // Claimed up front so the Http arm below holds no conflicting borrow.
        self.request_id += 1;
        let id = self.request_id;
        match &mut self.transport {
            McpTransport::Stdio(stdio) => {
                let McpTransportStdio { stdin, .. } = stdio.as_mut();
                let notification = serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": method,
                    "params": params
                });

                let mut msg = serde_json::to_string(&notification)?;
                msg.push('\n');

                stdin
                    .write_all(msg.as_bytes())
                    .await
                    .context("Failed to write MCP notification")?;

                Ok(())
            }
            // Streamable HTTP has no out-of-band channel: notifications are
            // POSTed like requests and the (empty) reply is discarded.
            McpTransport::Http {
                client,
                url,
                headers,
                session_id,
            } => {
                let request = JsonRpcRequest {
                    jsonrpc: "2.0".to_string(),
                    id,
                    method: method.to_string(),
                    params,
                };
                let timeout = std::time::Duration::from_millis(self.timeout_ms);
                let _ = tokio::time::timeout(
                    timeout,
                    Self::post_json_rpc(client, url, headers, session_id, &request),
                )
                .await;
                Ok(())
            }
        }
    }

    /// Read responses until we get the one with matching id.
    async fn read_response_stdio(
        stdout_lines: &mut tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
        expected_id: u64,
    ) -> Result<JsonRpcResponse> {
        loop {
            let line = stdout_lines
                .next_line()
                .await
                .context("MCP server stdout closed")?
                .context("Failed to read MCP response line")?;

            if line.is_empty() {
                continue;
            }

            let response: JsonRpcResponse = serde_json::from_str(&line)
                .with_context(|| format!("Failed to parse MCP response: {line}"))?;

            if response.id == expected_id {
                return Ok(response);
            }
            // Otherwise it's a notification or out-of-order response; skip it
        }
    }

    /// Shut down the server gracefully.
    pub async fn shutdown(&mut self) -> Result<()> {
        // Shutdown messages first (their borrows end before teardown).
        if matches!(self.transport, McpTransport::Stdio(_)) {
            let _ = self.send_request("shutdown", None).await;
            let _ = self.send_notification("exit", None).await;
        }
        match &mut self.transport {
            McpTransport::Stdio(stdio) => {
                let _ = stdio.child.kill().await;
            }
            // HTTP is stateless per request: best-effort session close, then
            // drop. Failures are ignored — shutdown must not fail a run.
            McpTransport::Http {
                client,
                url,
                session_id,
                ..
            } => {
                if let Some(sid) = session_id.clone() {
                    let _ = client
                        .delete(url.as_str())
                        .header("mcp-session-id", sid)
                        .send()
                        .await;
                }
            }
        }
        Ok(())
    }
}

/// Connect to an MCP server and discover its tools.
pub async fn connect_server(config: &McpServerConfig) -> Result<(McpConnection, Vec<McpTool>)> {
    match &config.server_type {
        McpServerType::Local { command, args, env } => {
            let mut conn =
                McpConnection::connect_stdio(&config.name, command, args, env, config.timeout_ms)
                    .await?;
            let tools = conn.list_tools().await?;
            Ok((conn, tools))
        }
        McpServerType::Remote { url, headers } => {
            let mut conn =
                McpConnection::connect_http(&config.name, url, headers, config.timeout_ms).await?;
            let tools = conn.list_tools().await?;
            Ok((conn, tools))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mcp_connection_stdio() {
        // Use a simple echo server for testing
        let config = McpServerConfig {
            name: "test".to_string(),
            server_type: McpServerType::Local {
                command: "cat".to_string(),
                args: vec![],
                env: HashMap::new(),
            },
            enabled: true,
            timeout_ms: 5000,
        };

        // This will fail because cat doesn't speak MCP, but it tests the spawn
        let result = connect_server(&config).await;
        assert!(result.is_err()); // Expected — cat doesn't speak JSON-RPC
    }

    #[tokio::test]
    async fn test_http_transport_list_and_call_json() {
        use wiremock::matchers::{body_string_contains, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/mcp"))
            .and(body_string_contains("initialize"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"jsonrpc": "2.0", "id": 1,
                    "result": {"serverInfo": {"name": "mock-http"}}})),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/mcp"))
            .and(body_string_contains("notifications/initialized"))
            .respond_with(ResponseTemplate::new(202).set_body_string("Accepted"))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/mcp"))
            .and(body_string_contains("tools/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"jsonrpc": "2.0", "id": 3, "result": {"tools": [
                    {"name": "search", "description": "Search docs",
                     "inputSchema": {"type": "object"}}
                ]}}),
            ))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/mcp"))
            .and(body_string_contains("tools/call"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"jsonrpc": "2.0", "id": 4,
                    "result": {"content": [{"type": "text", "text": "found it"}]}})),
            )
            .mount(&server)
            .await;

        let config = McpServerConfig {
            name: "mock-http".to_string(),
            server_type: McpServerType::Remote {
                url: format!("{}/mcp", server.uri()),
                headers: HashMap::new(),
            },
            enabled: true,
            timeout_ms: 5000,
        };
        let (mut conn, tools) = connect_server(&config).await.expect("http connect");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "search");
        let out = conn
            .call_tool("search", Some(serde_json::json!({"q": "x"})))
            .await
            .expect("http call");
        assert_eq!(
            out.get("content")
                .and_then(|c| c.as_array())
                .map(|a| a.len()),
            Some(1)
        );
        conn.shutdown().await.expect("http shutdown");
    }

    #[tokio::test]
    async fn test_http_transport_parses_sse_stream() {
        use wiremock::matchers::{body_string_contains, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/mcp"))
            .and(body_string_contains("initialize"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"jsonrpc": "2.0", "id": 1, "result": {}})),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/mcp"))
            .and(body_string_contains("notifications/initialized"))
            .respond_with(ResponseTemplate::new(202).set_body_string("Accepted"))
            .mount(&server)
            .await;
        // SSE envelope around the tools/list reply.
        let sse = "event: message\ndata: {\"jsonrpc\": \"2.0\", \"id\": 3,                    \"result\": {\"tools\": [{\"name\": \"fetch\",                    \"description\": \"Fetch a URL\"}]}}\n\n";
        Mock::given(method("POST"))
            .and(path("/mcp"))
            .and(body_string_contains("tools/list"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse),
            )
            .mount(&server)
            .await;

        let config = McpServerConfig {
            name: "mock-sse".to_string(),
            server_type: McpServerType::Remote {
                url: format!("{}/mcp", server.uri()),
                headers: HashMap::new(),
            },
            enabled: true,
            timeout_ms: 5000,
        };
        let (_conn, tools) = connect_server(&config).await.expect("sse connect");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "fetch");
    }

    #[test]
    fn test_json_rpc_request_serialization() {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "initialize".to_string(),
            params: Some(serde_json::json!({ "test": true })),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"method\":\"initialize\""));
    }
}
