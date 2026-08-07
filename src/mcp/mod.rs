//! MCP (Model Context Protocol) server support.
//!
//! Allows NIKI to connect to local (STDIO) and remote (HTTP/SSE) MCP servers
//! to extend agent capabilities with external tools.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// MCP server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub server_type: McpServerType,
    pub enabled: bool,
    pub timeout_ms: u64,
}

/// Type of MCP server connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum McpServerType {
    /// Local process communicating via STDIO.
    Local {
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
    },
    /// Remote server communicating via HTTP/SSE.
    Remote {
        url: String,
        headers: HashMap<String, String>,
    },
}

/// A discovered MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub server_name: String,
    pub input_schema: Option<serde_json::Value>,
}

/// Manages MCP server connections and tool discovery.
pub struct McpManager {
    servers: Vec<McpServerConfig>,
    tools: Vec<McpTool>,
}

impl McpManager {
    /// Create a new MCP manager.
    pub fn new() -> Self {
        Self {
            servers: Vec::new(),
            tools: Vec::new(),
        }
    }

    /// Load MCP server configurations from a config file.
    pub fn load_config(&mut self, config_path: &Path) -> Result<()> {
        if !config_path.exists() {
            return Ok(());
        }
        let content = std::fs::read_to_string(config_path)?;
        let servers: Vec<McpServerConfig> = serde_json::from_str(&content)?;
        self.servers = servers;
        Ok(())
    }

    /// Add a server configuration.
    pub fn add_server(&mut self, config: McpServerConfig) {
        self.servers.push(config);
    }

    /// Get all configured servers.
    pub fn servers(&self) -> &[McpServerConfig] {
        &self.servers
    }

    /// Get enabled servers only.
    pub fn enabled_servers(&self) -> Vec<&McpServerConfig> {
        self.servers.iter().filter(|s| s.enabled).collect()
    }

    /// Get all discovered tools.
    pub fn tools(&self) -> &[McpTool] {
        &self.tools
    }
}

impl Default for McpManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_manager_new() {
        let manager = McpManager::new();
        assert_eq!(manager.servers().len(), 0);
    }

    #[test]
    fn mcp_add_server() {
        let mut manager = McpManager::new();
        manager.add_server(McpServerConfig {
            name: "filesystem".to_string(),
            server_type: McpServerType::Local {
                command: "npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-filesystem".to_string(),
                ],
                env: HashMap::new(),
            },
            enabled: true,
            timeout_ms: 5000,
        });
        assert_eq!(manager.servers().len(), 1);
        assert_eq!(manager.enabled_servers().len(), 1);
    }

    #[test]
    fn mcp_load_config_missing_file() {
        let mut manager = McpManager::new();
        let result = manager.load_config(Path::new("/nonexistent/path.json"));
        assert!(result.is_ok());
    }
}
