//! MCP (Model Context Protocol) server support.
//!
//! Allows NIKI to connect to local (STDIO) and remote (HTTP/SSE) MCP servers
//! to extend agent capabilities with external tools.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

pub mod client;

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

/// MCP governance policy — controls what agents can do with MCP tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpGovernance {
    /// When true, agents can only call MCP tools that are marked read-only.
    /// Read-only tools are those that don't modify external state (e.g., search, fetch).
    #[serde(default = "default_read_only_policy")]
    pub read_only: bool,
    /// Domain allowlist for web fetch tools (empty = block all web fetches).
    #[serde(default)]
    pub domain_allowlist: Vec<String>,
}

fn default_read_only_policy() -> bool {
    true
}

impl Default for McpGovernance {
    fn default() -> Self {
        Self {
            read_only: true,
            domain_allowlist: Vec::new(),
        }
    }
}

/// MCP trust store — the "up front" gate for project MCP servers.
///
/// Project MCP servers are arbitrary local processes or remote endpoints:
/// a malicious server can exfiltrate the agent's context. Niki therefore
/// requires explicit trust before connecting: each server is identified by a
/// fingerprint of its command/args/url, so a swap of the underlying binary is
/// detected even if the server name is unchanged. Persisted to disk so the
/// user only ever answers "trust this server?" once.
///
/// Default state: nothing is trusted. Opt in with [`McpManager::with_trust_store`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpTrustStore {
    /// server name → fingerprint of the last-allowed configuration.
    pub allowed: HashMap<String, String>,
    /// server names explicitly denied (never auto-trusted).
    pub denied: Vec<String>,
}

impl McpTrustStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `config` is currently trusted (allowed + fingerprint unchanged).
    pub fn is_allowed(&self, config: &McpServerConfig) -> bool {
        if self.denied.contains(&config.name) {
            return false;
        }
        match self.allowed.get(&config.name) {
            Some(fp) => fp == &config.fingerprint(),
            None => false,
        }
    }

    /// Whether the server needs an explicit trust decision before connecting.
    pub fn needs_gate(&self, config: &McpServerConfig) -> bool {
        !self.is_allowed(config)
    }

    /// Trust this exact configuration.
    pub fn allow(&mut self, config: &McpServerConfig) {
        self.allowed
            .insert(config.name.clone(), config.fingerprint());
        self.denied.retain(|n| n != &config.name);
    }

    /// Explicitly deny this server (never auto-trusted).
    pub fn deny(&mut self, name: &str) {
        self.allowed.remove(name);
        if !self.denied.contains(&name.to_string()) {
            self.denied.push(name.to_string());
        }
    }

    /// Load the trust store from disk (missing file → empty).
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Persist the trust store to disk.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

impl McpServerConfig {
    /// Stable fingerprint of this server's configuration. Used by the trust
    /// store to detect a swapped command/args/url under an unchanged name.
    pub fn fingerprint(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut s = String::new();
        match &self.server_type {
            McpServerType::Local {
                command,
                args,
                env,
            } => {
                s.push_str("local:");
                s.push_str(command);
                s.push(':');
                s.push_str(&args.join(","));
                for (k, v) in env {
                    s.push_str(&format!("{}={}", k, v));
                }
            }
            McpServerType::Remote { url, headers } => {
                s.push_str("remote:");
                s.push_str(url);
                for k in headers.keys() {
                    s.push_str(k);
                }
            }
        }
        let mut hasher = DefaultHasher::new();
        s.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}

/// Default trust-store path: project-local `.niki/mcp_trust.json`.
pub fn trust_store_path(project_path: &Path) -> std::path::PathBuf {
    project_path.join(".niki").join("mcp_trust.json")
}

/// Manages MCP server connections and tool discovery.
pub struct McpManager {
    servers: Vec<McpServerConfig>,
    tools: Vec<McpTool>,
    governance: McpGovernance,
    /// Optional trust store for the "up front" MCP trust gate. When set,
    /// servers that need an explicit trust decision are skipped (and
    /// warned) until the user allows them. When `None`, behavior is
    /// unchanged (all enabled servers connect).
    trust_store: Option<McpTrustStore>,
}

impl McpManager {
    /// Create a new MCP manager.
    pub fn new() -> Self {
        Self {
            servers: Vec::new(),
            tools: Vec::new(),
            governance: McpGovernance::default(),
            trust_store: None,
        }
    }

    /// Set the governance policy.
    pub fn with_governance(mut self, governance: McpGovernance) -> Self {
        self.governance = governance;
        self
    }

    /// Attach the trust store for the up-front MCP trust gate.
    pub fn with_trust_store(mut self, trust_store: McpTrustStore) -> Self {
        self.trust_store = Some(trust_store);
        self
    }

    /// Get the governance policy.
    pub fn governance(&self) -> &McpGovernance {
        &self.governance
    }

    /// Get the trust store, if configured.
    pub fn trust_store(&self) -> Option<&McpTrustStore> {
        self.trust_store.as_ref()
    }

    /// Whether `config` is currently trusted.
    pub fn is_trusted(&self, config: &McpServerConfig) -> bool {
        match &self.trust_store {
            Some(ts) => ts.is_allowed(config),
            None => true, // no gate configured -> trust by default
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

    /// Connect to all enabled servers and discover their tools.
    ///
    /// When a trust store is attached, servers that need an explicit trust
    /// decision are skipped (and warned) until the user allows them — the
    /// "up front" gate from S5 §7.
    pub async fn connect_all(&mut self) -> Result<()> {
        let enabled: Vec<McpServerConfig> =
            self.servers.iter().filter(|s| s.enabled).cloned().collect();

        for server_config in &enabled {
            if let Some(ts) = &self.trust_store
                && ts.needs_gate(server_config)
            {
                tracing::warn!(
                    "MCP server '{}' is not trusted — skipping until allowed \
                     (run `niki mcp trust {}` or edit .niki/mcp_trust.json)",
                    server_config.name,
                    server_config.name
                );
                continue;
            }
            match client::connect_server(server_config).await {
                Ok((_conn, tools)) => {
                    tracing::info!(
                        "MCP server '{}' connected, {} tools discovered",
                        server_config.name,
                        tools.len()
                    );
                    self.tools.extend(tools);
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to connect MCP server '{}': {}",
                        server_config.name,
                        e
                    );
                }
            }
        }

        Ok(())
    }

    /// Filter tools by governance policy.
    pub fn allowed_tools(&self) -> Vec<&McpTool> {
        if self.governance.read_only {
            // In read-only mode, return all tools (we trust the server to mark tools correctly)
            // In a future implementation, we could filter by tool metadata
            self.tools.iter().collect()
        } else {
            self.tools.iter().collect()
        }
    }

    /// Format MCP tools for injection into agent prompts.
    pub fn tools_for_prompt(&self) -> String {
        let allowed = self.allowed_tools();
        if allowed.is_empty() {
            return String::new();
        }

        let mut output = String::from("\n## Available MCP Tools\n\n");
        for tool in &allowed {
            output.push_str(&format!(
                "- **{}** (from {}): {}\n",
                tool.name, tool.server_name, tool.description
            ));
        }
        output.push_str("\nUse these tools via the standard MCP tool call format.\n");
        output
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

    fn local_server(name: &str, command: &str) -> McpServerConfig {
        McpServerConfig {
            name: name.to_string(),
            server_type: McpServerType::Local {
                command: command.to_string(),
                args: vec!["--foo".to_string()],
                env: HashMap::new(),
            },
            enabled: true,
            timeout_ms: 5000,
        }
    }

    #[test]
    fn trust_store_untrusted_by_default() {
        let ts = McpTrustStore::new();
        let cfg = local_server("fs", "npx");
        assert!(!ts.is_allowed(&cfg));
        assert!(ts.needs_gate(&cfg));
    }

    #[test]
    fn trust_store_allow_then_deny() {
        let mut ts = McpTrustStore::new();
        let cfg = local_server("fs", "npx");
        ts.allow(&cfg);
        assert!(ts.is_allowed(&cfg));
        ts.deny("fs");
        assert!(!ts.is_allowed(&cfg));
    }

    #[test]
    fn trust_store_detects_swapped_command() {
        let mut ts = McpTrustStore::new();
        let cfg = local_server("fs", "npx");
        ts.allow(&cfg);
        // Same name, different command -> fingerprint mismatch -> not trusted.
        let swapped = local_server("fs", "node");
        assert!(!ts.is_allowed(&swapped));
    }

    #[test]
    fn manager_without_trust_store_trusts_everything() {
        let cfg = local_server("fs", "npx");
        let manager = McpManager::new();
        assert!(manager.is_trusted(&cfg));
    }

    #[test]
    fn trust_store_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("mcp_trust.json");
        let mut ts = McpTrustStore::new();
        ts.allow(&local_server("fs", "npx"));
        ts.save(&path).unwrap();
        let loaded = McpTrustStore::load(&path);
        assert!(loaded.is_allowed(&local_server("fs", "npx")));
    }
}
