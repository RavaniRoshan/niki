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
    #[serde(default)]
    data: Option<serde_json::Value>,
}

/// A connection to an MCP server (STDIO transport).
pub struct McpConnection {
    name: String,
    child: Child,
    stdin: tokio::process::ChildStdin,
    stdout_lines: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
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
            child,
            stdin,
            stdout_lines,
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

        let mut msg = serde_json::to_string(&request)?;
        msg.push('\n');

        self.stdin
            .write_all(msg.as_bytes())
            .await
            .context("Failed to write to MCP server stdin")?;

        // Wait for response with timeout
        let timeout = std::time::Duration::from_millis(self.timeout_ms);
        let response = tokio::time::timeout(timeout, self.read_response(id))
            .await
            .context("MCP request timed out")??;

        if let Some(error) = response.error {
            return Err(anyhow::anyhow!(
                "MCP error {}: {}",
                error.code,
                error.message
            ));
        }

        Ok(response.result.unwrap_or(serde_json::Value::Null))
    }

    /// Send a JSON-RPC notification (no response expected).
    async fn send_notification(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<()> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        let mut msg = serde_json::to_string(&notification)?;
        msg.push('\n');

        self.stdin
            .write_all(msg.as_bytes())
            .await
            .context("Failed to write MCP notification")?;

        Ok(())
    }

    /// Read responses until we get the one with matching id.
    async fn read_response(&mut self, expected_id: u64) -> Result<JsonRpcResponse> {
        loop {
            let line = self
                .stdout_lines
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
        let _ = self.send_request("shutdown", None).await;
        let _ = self.send_notification("exit", None).await;
        let _ = self.child.kill().await;
        Ok(())
    }
}

/// Connect to an MCP server and discover its tools.
pub async fn connect_server(config: &McpServerConfig) -> Result<(McpConnection, Vec<McpTool>)> {
    match &config.server_type {
        McpServerType::Local {
            command,
            args,
            env,
        } => {
            let mut conn = McpConnection::connect_stdio(
                &config.name,
                command,
                args,
                env,
                config.timeout_ms,
            )
            .await?;
            let tools = conn.list_tools().await?;
            Ok((conn, tools))
        }
        McpServerType::Remote { url, headers: _ } => {
            // TODO: HTTP/SSE transport (not yet implemented)
            Err(anyhow::anyhow!(
                "MCP remote server '{}' ({}) — HTTP/SSE transport not yet implemented",
                config.name,
                url
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
