//! One NIKI `Tool` per discovered MCP tool, so a model can actually call one.
//!
//! The manager living for the run (batch 7, B7-01) made the call path
//! possible. This is the other half: the tool loop dispatches by *name* out of
//! a `ToolRegistry`, and nothing put an MCP tool in one. A model could be told
//! a server offered `search_docs` and have no way to reach it — the same
//! shape of failure as a prompt that instructs a call the runtime cannot
//! honour, which is what `tools_for_prompt` used to do before batch 5.
//!
//! Three decisions worth stating:
//!
//! **The name is namespaced.** `mcp__{server}__{tool}`. An MCP server is free
//! to call a tool `read` or `bash`, and NIKI has both. A flat name would let
//! a server's `bash` shadow the real one, or be shadowed by it, and the model
//! would have no way to tell which it got.
//!
//! **The permission mirrors the governance.** A tool the server marked
//! read-only is `Allow`; one that did not is `Ask`, so it reaches the same
//! prompt the sandbox's own commands do. Read-only governance already denies
//! the mutating ones outright, so under the default posture this is belt and
//! braces — and a user who turns governance off still gets a prompt rather
//! than silent third-party writes.
//!
//! **Errors carry the chain.** `anyhow::Error::to_string()` prints only the
//! outermost context, so a tool result built from it would tell the model the
//! call failed and nothing about *why*, and the model would retry the same
//! call. B7-01 found that; this is where it would have bitten.

use std::sync::Arc;
use std::time::Duration;

use crate::mcp::{McpManager, McpTool};
use crate::runtime::ToolContext;
use crate::runtime::tools::{
    PermissionRequirement, RiskLevel, Tool, ToolCategory, ToolDef, ToolInput, ToolResult,
    ToolStatus,
};

/// Hand a computed string to `ToolDef`'s `&'static str` fields, once.
///
/// A process-wide table keyed by content, so the same MCP tool across a
/// hundred runs is allocated once rather than a hundred times. Not a general
/// string cache: only `McpToolAdapter` uses it, and the key set is small and
/// bounded by the servers a user actually configures.
fn intern(s: String) -> &'static str {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static TABLE: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let table = TABLE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = table.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(existing) = guard.get(&s) {
        return existing;
    }
    let leaked: &'static str = Box::leak(s.clone().into_boxed_str());
    guard.insert(s, leaked);
    leaked
}

/// The name a model sees for an MCP tool.
///
/// `mcp__{server}__{tool}`, with both parts reduced to characters a tool name
/// can carry. MCP servers are third parties: a server named `my server/v2`
/// would otherwise produce a name the model cannot reproduce and the router
/// cannot match, and the failure would read as "no such tool".
pub fn qualified_name(server: &str, tool: &str) -> String {
    format!("mcp__{}__{}", sanitise(server), sanitise(tool))
}

fn sanitise(part: &str) -> String {
    let mut out: String = part
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    // Collapse runs, and trim, so `a b` and `a__b` cannot both appear as the
    // same server with two different tools.
    let mut collapsed = String::with_capacity(out.len());
    let mut last_underscore = false;
    for c in out.drain(..) {
        if c == '_' {
            if last_underscore {
                continue;
            }
            last_underscore = true;
        } else {
            last_underscore = false;
        }
        collapsed.push(c);
    }
    let trimmed = collapsed.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed
    }
}

/// The NIKI tool wrapper around one MCP tool.
pub struct McpToolAdapter {
    manager: Arc<McpManager>,
    server: String,
    tool: McpTool,
    /// The schema, held as a `String` because `ToolDef` is built in `static`
    /// context for the baseline tools and this one is built per run.
    parameters: String,
    name: String,
    description: String,
    permission: PermissionRequirement,
    /// The `&'static` view, built once. See `def()`.
    def: std::sync::OnceLock<&'static ToolDef>,
}

impl McpToolAdapter {
    pub fn new(manager: Arc<McpManager>, tool: McpTool) -> Self {
        let name = qualified_name(&tool.server_name, &tool.name);
        // A server that describes nothing still needs a description the model
        // can act on. `get_` with a null schema is what a badly-behaved server
        // sends, and a tool advertised with an empty schema gets arguments the
        // model invented.
        let parameters = match &tool.input_schema {
            Some(schema) if schema.is_object() && schema.get("type").is_some() => {
                schema.to_string()
            }
            _ => r#"{"type":"object","properties":{},"additionalProperties":true}"#.to_string(),
        };
        let description = if tool.description.trim().is_empty() {
            format!(
                "The MCP server '{}' offers a tool called '{}', and described it \
                 with no documentation. Treat its arguments as unknown: read what \
                 you pass it, and do not assume what it returns.",
                tool.server_name, tool.name
            )
        } else {
            format!("[MCP: {}] {}", tool.server_name, tool.description.trim())
        };
        // Read-only is `Allow`; anything else is `Ask`.
        let permission = if tool.read_only {
            PermissionRequirement::Allow
        } else {
            PermissionRequirement::Ask
        };
        Self {
            manager,
            server: tool.server_name.clone(),
            tool,
            parameters,
            name,
            description,
            permission,
            def: std::sync::OnceLock::new(),
        }
    }

    // `server_tool_name` used to live here and returned `&self.tool.name` — the
    // **bare** name, `echo`, while the tool is registered as
    // `mcp__<server>__<tool>` (`qualified_name`, used at construction below).
    //
    // It was dead, and dead in the worst way: a method that reads like "what is
    // this tool called" and answers with a different name than the registry
    // holds. The first caller would have sent `echo` to `call_tool` for a tool
    // registered as `mcp__fixture__echo`, and failed with a not-found naming a
    // string the user never configured.
    //
    // Deleted rather than corrected: the qualified name is the only name this
    // type has in the product, and leaving a second one to be reached for is
    // what produced the bug.
}

#[async_trait::async_trait]
impl Tool for McpToolAdapter {
    fn def(&self) -> &ToolDef {
        // `ToolDef` holds `&'static str` because the 22 baseline tools are
        // `static DEF`s. An MCP tool's name, description and schema are
        // computed per run, so they cannot be.
        //
        // Leaking each `def()` call would leak on every invocation, so the
        // strings go through an intern table: the leak is bounded by the
        // number of *distinct* MCP tools the process has ever seen — a handful
        // — rather than by the number of calls. An IDE driving NIKI all day
        // would otherwise grow a few hundred bytes per run for ever.
        self.def.get_or_init(|| {
            Box::leak(Box::new(ToolDef {
                name: intern(self.name.clone()),
                description: intern(self.description.clone()),
                category: ToolCategory::Mcp,
                risk_level: if self.tool.read_only {
                    RiskLevel::Low
                } else {
                    RiskLevel::Medium
                },
                permission: self.permission,
                agent_access: &[],
                parameters: intern(self.parameters.clone()),
            }))
        })
    }

    async fn execute(&self, input: ToolInput, _ctx: &ToolContext) -> ToolResult {
        let started = std::time::Instant::now();
        let result = self
            .manager
            .call_tool(&self.server, &self.tool.name, input.raw.clone())
            .await;

        match result {
            Ok(value) => {
                // A server can report a tool-level failure *inside* a
                // successful JSON-RPC result: `isError: true`. Treating that
                // as success is how `web_search` shipped two batches' worth of
                // "Success with nothing in it".
                let is_error = value
                    .get("isError")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let rendered = render(&value);
                if is_error {
                    return ToolResult {
                        tool_id: crate::runtime::tools::ToolId::generate(),
                        tool_name: self.name.clone(),
                        status: ToolStatus::Failed,
                        summary: format!("{} failed: {}", self.tool.name, rendered),
                        data: crate::runtime::tools::ToolData::Text { text: rendered },
                        duration: started.elapsed(),
                        artifacts: Vec::new(),
                        diagnostics: vec!["the MCP server reported isError: true".into()],
                        metadata: std::collections::HashMap::new(),
                    };
                }
                ToolResult {
                    tool_id: crate::runtime::tools::ToolId::generate(),
                    tool_name: self.name.clone(),
                    status: ToolStatus::Success,
                    summary: format!("{} returned {}", self.tool.name, summarise(&value)),
                    data: crate::runtime::tools::ToolData::Text { text: rendered },
                    duration: started.elapsed(),
                    artifacts: Vec::new(),
                    diagnostics: Vec::new(),
                    metadata: std::collections::HashMap::new(),
                }
            }
            Err(e) => {
                // The **chain**, not `to_string()`. The server's own message
                // is a cause, several levels down; a tool result built from
                // `to_string()` would tell the model the call failed and
                // nothing about why, and the model would try the same thing
                // again.
                let detail = format!("{e:#}");
                ToolResult {
                    tool_id: crate::runtime::tools::ToolId::generate(),
                    tool_name: self.name.clone(),
                    status: ToolStatus::Failed,
                    summary: format!("{} could not be called: {}", self.tool.name, detail),
                    data: crate::runtime::tools::ToolData::Text {
                        text: detail.clone(),
                    },
                    duration: started.elapsed(),
                    artifacts: Vec::new(),
                    diagnostics: vec![detail],
                    metadata: std::collections::HashMap::new(),
                }
            }
        }
    }
}

/// Register every governed-allowed MCP tool with a registry.
///
/// Called with the run's manager. A server's tool is skipped — not silently:
/// `build_registry` reports the count so the notice can say what the model
/// will actually be able to call, which is the difference between a tool list
/// and a promise.
pub fn build_registry(
    registry: &mut crate::runtime::ToolRegistry,
    manager: Arc<McpManager>,
) -> usize {
    let allowed: Vec<McpTool> = manager.allowed_tools().into_iter().cloned().collect();
    let count = allowed.len();
    for tool in allowed {
        registry.register(Box::new(McpToolAdapter::new(manager.clone(), tool)));
    }
    count
}

/// Flatten an MCP result for a model: content blocks as text, plus the raw
/// value when there is no `content` at all.
///
/// A server that returns `{"result": 42}` must not reach the model as `{}`.
fn render(value: &serde_json::Value) -> String {
    if let Some(content) = value.get("content").and_then(|c| c.as_array()) {
        let mut out = String::new();
        for block in content {
            let text = block
                .get("text")
                .and_then(|t| t.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| block.to_string());
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&text);
        }
        if !out.trim().is_empty() {
            return out;
        }
    }
    value.to_string()
}

/// A one-line description for the transcript, where a whole JSON payload would
/// not fit and is not what a reader wants.
fn summarise(value: &serde_json::Value) -> String {
    let rendered = render(value);
    let first = rendered.lines().next().unwrap_or("").trim().to_string();
    if first.is_empty() {
        "an empty response".to_string()
    } else if first.chars().count() > 80 {
        let head: String = first.chars().take(77).collect();
        format!("{head}…")
    } else {
        first
    }
}

/// The deadline for one MCP call, from the server's own config.
pub fn call_timeout(config: &crate::mcp::McpServerConfig) -> Duration {
    Duration::from_millis(config.timeout_ms)
}
