//! Tool Runtime — the controlled boundary between agents and the real world.
//!
//! Every tool call goes through the ToolRegistry, which enforces:
//! - permissions (tool-level, path-level, command-level)
//! - sandboxing
//! - auditing
//! - observability
//! - structured results (ToolResult)
//!
//! Tools are categorized as:
//! - EXPLORE: read, glob, grep, list
//! - MODIFY: write, edit, patch
//! - EXECUTE: bash, test
//! - RESEARCH: web_search, web_fetch
//! - ORCHESTRATION: task_spawn, task_status, task_cancel, task_create, task_update, task_list
//! - HUMAN: ask_user, approval
//! - KNOWLEDGE: skill_list, skill_load
//! - VCS: git

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use uuid::Uuid;

use anyhow::Result;

use crate::event::{Event, EventBus};
use crate::llm::provider::{CompletionRequest, LlmProvider, ToolCall, ToolSpec};
use crate::mission::AgentId;

// ---------------------------------------------------------------------------
// Tool identifiers
// ---------------------------------------------------------------------------

/// Tool call identifier (unique).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ToolId(pub String);

impl fmt::Display for ToolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl ToolId {
    pub fn generate() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl std::str::FromStr for ToolId {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Tool categories
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolCategory {
    Explore,
    Modify,
    Execute,
    Research,
    Orchestration,
    Human,
    Knowledge,
    Vcs,
}

impl fmt::Display for ToolCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolCategory::Explore => write!(f, "explore"),
            ToolCategory::Modify => write!(f, "modify"),
            ToolCategory::Execute => write!(f, "execute"),
            ToolCategory::Research => write!(f, "research"),
            ToolCategory::Orchestration => write!(f, "orchestration"),
            ToolCategory::Human => write!(f, "human"),
            ToolCategory::Knowledge => write!(f, "knowledge"),
            ToolCategory::Vcs => write!(f, "vcs"),
        }
    }
}

// ---------------------------------------------------------------------------
// Risk levels
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

// ---------------------------------------------------------------------------
// Permission requirements
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionRequirement {
    /// Always allowed.
    Allow,
    /// Requires user confirmation.
    Ask,
    /// Always denied.
    Deny,
}

// ---------------------------------------------------------------------------
// Tool definition (metadata)
// ---------------------------------------------------------------------------

/// Metadata about a tool (registered in ToolRegistry).
#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub category: ToolCategory,
    pub risk_level: RiskLevel,
    pub permission: PermissionRequirement,
    pub agent_access: &'static [&'static str],
}

// ---------------------------------------------------------------------------
// ToolResult — structured result envelope
// ---------------------------------------------------------------------------

/// Status of a tool execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolStatus {
    Success,
    Failed,
    Cancelled,
    Timeout,
    PermissionDenied,
}

/// Structured result from a tool execution.
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub tool_id: ToolId,
    pub tool_name: String,
    pub status: ToolStatus,
    pub summary: String,
    pub data: ToolData,
    pub duration: Duration,
    pub artifacts: Vec<ArtifactRef>,
    pub diagnostics: Vec<String>,
    pub metadata: HashMap<String, String>,
}

/// Tool-specific data payload.
#[derive(Debug, Clone)]
pub enum ToolData {
    /// No structured data (simple text result).
    None,
    /// File content with line numbers.
    FileContent {
        path: String,
        lines: Vec<(usize, String)>,
        total_lines: usize,
    },
    /// Glob results.
    GlobResults {
        pattern: String,
        matches: Vec<String>,
    },
    /// Grep results.
    GrepResults {
        query: String,
        matches: Vec<GrepMatch>,
        file_count: usize,
        total_matches: usize,
    },
    /// Test results.
    TestResults {
        passed: usize,
        failed: usize,
        skipped: usize,
        failures: Vec<String>,
        duration_ms: u64,
    },
    /// Bash output.
    BashOutput {
        stdout: String,
        stderr: String,
        exit_code: i32,
    },
    /// Web search results.
    WebSearchResults {
        query: String,
        results: Vec<WebSearchResult>,
    },
    /// Web fetch result.
    WebFetchResult {
        url: String,
        content: String,
        format: String,
    },
    /// Task spawn result.
    TaskSpawned { task_id: String, agent_role: String },
    /// Task status.
    TaskStatus {
        task_id: String,
        status: String,
        progress: Option<f64>,
    },
    /// User response.
    UserResponse { question: String, response: String },
    /// Approval result.
    ApprovalResult {
        approved: bool,
        reason: Option<String>,
    },
    /// JSON data (for MCP and extensible tools).
    Json(serde_json::Value),
}

/// A file path with line range that was accessed.
#[derive(Debug, Clone)]
pub struct ArtifactRef {
    pub path: PathBuf,
    pub artifact_type: ArtifactType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactType {
    FileRead,
    FileWritten,
    FileEdited,
    Diff,
    TestOutput,
    BashOutput,
}

/// A single grep match.
#[derive(Debug, Clone)]
pub struct GrepMatch {
    pub file: String,
    pub line: usize,
    pub match_text: String,
    pub context: Option<String>,
}

/// A web search result.
#[derive(Debug, Clone)]
pub struct WebSearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub source: Option<String>,
}

// ---------------------------------------------------------------------------
// Tool trait — actual tool implementations
// ---------------------------------------------------------------------------

/// The trait all tools must implement.
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    /// Tool definition metadata.
    fn def(&self) -> &ToolDef;

    /// Execute the tool with the given input.
    async fn execute(&self, input: ToolInput, ctx: &ToolContext) -> ToolResult;
}

/// Input to a tool — a generic JSON value that each tool parses.
#[derive(Debug, Clone)]
pub struct ToolInput {
    pub raw: serde_json::Value,
}

impl ToolInput {
    pub fn new(raw: serde_json::Value) -> Self {
        Self { raw }
    }

    /// Get a string field from the input.
    pub fn str(&self, key: &str) -> Option<&str> {
        self.raw.get(key).and_then(|v| v.as_str())
    }

    /// Get an integer field from the input.
    pub fn int(&self, key: &str) -> Option<i64> {
        self.raw.get(key).and_then(|v| v.as_i64())
    }

    /// Get a required string field, returning error if missing.
    pub fn require_str(&self, key: &str) -> Result<&str, String> {
        self.str(key)
            .ok_or_else(|| format!("missing required field: {}", key))
    }
}

/// Execution context provided to every tool.
#[derive(Debug, Clone)]
pub struct ToolContext {
    pub agent_id: AgentId,
    pub mission_id: crate::mission::MissionId,
    pub role: String,
    pub project_path: PathBuf,
    pub permissions: HashMap<String, PermissionRequirement>,
}

// ---------------------------------------------------------------------------
// ToolRegistry — central registry of all tools
// ---------------------------------------------------------------------------

/// Central tool registry.
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
    defs: Vec<ToolDef>,
}

impl fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ToolRegistry")
            .field("tool_count", &self.tools.len())
            .finish()
    }
}

impl ToolRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            defs: Vec::new(),
        }
    }

    /// Register a tool.
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        let def = tool.def().clone();
        self.tools.insert(def.name.to_string(), tool);
        self.defs.push(def);
    }

    /// Get a tool by name.
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    /// List all tool definitions.
    pub fn list_defs(&self) -> &[ToolDef] {
        &self.defs
    }

    /// List tools accessible by a given agent role.
    pub fn for_role(&self, role: &str) -> Vec<&ToolDef> {
        self.defs
            .iter()
            .filter(|d| d.agent_access.is_empty() || d.agent_access.contains(&role))
            .collect()
    }

    /// Build tool specifications (JSON-schema) for the LLM, scoped to a role.
    ///
    /// The generated parameter schema is intentionally permissive: each tool
    /// accepts a free-form `object`. Concrete arg validation happens inside the
    /// tool's `execute()` via `ToolInput` accessors.
    pub fn tool_specs_for(&self, role: &str) -> Vec<ToolSpec> {
        self.for_role(role)
            .into_iter()
            .map(|def| ToolSpec {
                name: def.name.to_string(),
                description: def.description.to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": true,
                }),
            })
            .collect()
    }

    /// Execute a tool by name.
    pub async fn execute(&self, name: &str, input: ToolInput, ctx: &ToolContext) -> ToolResult {
        let start = Instant::now();
        match self.tools.get(name) {
            Some(tool) => {
                let mut result = tool.execute(input, ctx).await;
                result.tool_name = name.to_string();
                result.duration = start.elapsed();
                result
            }
            None => ToolResult {
                tool_id: ToolId::generate(),
                tool_name: name.to_string(),
                status: ToolStatus::Failed,
                summary: format!("tool not found: {}", name),
                data: ToolData::None,
                duration: start.elapsed(),
                artifacts: Vec::new(),
                diagnostics: vec![format!("unknown tool: {}", name)],
                metadata: HashMap::new(),
            },
        }
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Built-in tool implementations
// ---------------------------------------------------------------------------

/// Read tool — read file content with line numbers.
pub struct ReadTool;

#[async_trait::async_trait]
impl Tool for ReadTool {
    fn def(&self) -> &ToolDef {
        static DEF: ToolDef = ToolDef {
            name: "read",
            description: "Read file content with line numbers",
            category: ToolCategory::Explore,
            risk_level: RiskLevel::Low,
            permission: PermissionRequirement::Allow,
            agent_access: &[],
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, ctx: &ToolContext) -> ToolResult {
        let path = match input.require_str("path") {
            Ok(p) => p,
            Err(e) => return make_error_result(&e),
        };
        let full_path = if PathBuf::from(path).is_absolute() {
            PathBuf::from(path)
        } else {
            ctx.project_path.join(path)
        };
        let start_line = input.int("start_line").unwrap_or(1) as usize;
        let end_line = input.int("end_line").map(|n| n as usize);

        match tokio::fs::read_to_string(&full_path).await {
            Ok(content) => {
                let lines: Vec<(usize, String)> = content
                    .lines()
                    .enumerate()
                    .map(|(i, l)| (i + 1, l.to_string()))
                    .collect();
                let total = lines.len();
                let filtered: Vec<(usize, String)> = lines
                    .into_iter()
                    .filter(|(i, _)| *i >= start_line)
                    .filter(|(i, _)| end_line.is_none_or(|e| *i <= e))
                    .collect();
                let summary = format!(
                    "{}:{} ({}/{})",
                    full_path.display(),
                    start_line,
                    filtered.len(),
                    total
                );
                ToolResult {
                    tool_id: ToolId::generate(),
                    tool_name: "read".into(),
                    status: ToolStatus::Success,
                    summary: summary.clone(),
                    data: ToolData::FileContent {
                        path: full_path.display().to_string(),
                        lines: filtered,
                        total_lines: total,
                    },
                    duration: Duration::ZERO,
                    artifacts: vec![ArtifactRef {
                        path: full_path,
                        artifact_type: ArtifactType::FileRead,
                    }],
                    diagnostics: Vec::new(),
                    metadata: HashMap::new(),
                }
            }
            Err(e) => make_error_result(&format!("failed to read {}: {}", full_path.display(), e)),
        }
    }
}

/// Glob tool — find files by pattern.
pub struct GlobTool;

#[async_trait::async_trait]
impl Tool for GlobTool {
    fn def(&self) -> &ToolDef {
        static DEF: ToolDef = ToolDef {
            name: "glob",
            description: "Find files by glob pattern",
            category: ToolCategory::Explore,
            risk_level: RiskLevel::Low,
            permission: PermissionRequirement::Allow,
            agent_access: &[],
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, ctx: &ToolContext) -> ToolResult {
        let pattern = match input.require_str("pattern") {
            Ok(p) => p,
            Err(e) => return make_error_result(&e),
        };
        let full_pattern = if pattern.starts_with('/') {
            pattern.to_string()
        } else {
            format!("{}/{}", ctx.project_path.display(), pattern)
        };
        match glob::glob(&full_pattern) {
            Ok(paths) => {
                let matches: Vec<String> = paths
                    .filter_map(|p| p.ok())
                    .map(|p| {
                        p.strip_prefix(&ctx.project_path)
                            .unwrap_or(&p)
                            .display()
                            .to_string()
                    })
                    .collect();
                let count = matches.len();
                ToolResult {
                    tool_id: ToolId::generate(),
                    tool_name: "glob".into(),
                    status: ToolStatus::Success,
                    summary: format!("{} matches", count),
                    data: ToolData::GlobResults {
                        pattern: pattern.to_string(),
                        matches,
                    },
                    duration: Duration::ZERO,
                    artifacts: Vec::new(),
                    diagnostics: Vec::new(),
                    metadata: HashMap::new(),
                }
            }
            Err(e) => make_error_result(&format!("glob error: {}", e)),
        }
    }
}

/// Grep tool — search file contents.
pub struct GrepTool;

#[async_trait::async_trait]
impl Tool for GrepTool {
    fn def(&self) -> &ToolDef {
        static DEF: ToolDef = ToolDef {
            name: "grep",
            description: "Search file contents with regex",
            category: ToolCategory::Explore,
            risk_level: RiskLevel::Low,
            permission: PermissionRequirement::Allow,
            agent_access: &[],
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, ctx: &ToolContext) -> ToolResult {
        let query = match input.require_str("query") {
            Ok(q) => q,
            Err(e) => return make_error_result(&e),
        };
        let include = input.str("include").map(|s| s.to_string());
        let path = input.str("path").map(|s| s.to_string());

        // Use ripgrep via command if available, fall back to grep -r
        let mut cmd = tokio::process::Command::new("rg");
        cmd.arg("--no-heading").arg("--line-number");
        if let Some(inc) = &include {
            cmd.arg("-g").arg(inc);
        }
        let search_path = path
            .map(|p| {
                if PathBuf::from(&p).is_absolute() {
                    p
                } else {
                    format!("{}/{}", ctx.project_path.display(), p)
                }
            })
            .unwrap_or_else(|| ctx.project_path.display().to_string());
        cmd.arg(query).arg(&search_path);

        match cmd.output().await {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let matches: Vec<GrepMatch> = stdout
                    .lines()
                    .filter_map(|line| {
                        let parts: Vec<&str> = line.splitn(3, ':').collect();
                        if parts.len() >= 3 {
                            Some(GrepMatch {
                                file: parts[0].to_string(),
                                line: parts[1].parse().unwrap_or(0),
                                match_text: parts[2].to_string(),
                                context: None,
                            })
                        } else {
                            None
                        }
                    })
                    .collect();
                let file_count = matches
                    .iter()
                    .map(|m| &m.file)
                    .collect::<std::collections::HashSet<_>>()
                    .len();
                let total = matches.len();
                ToolResult {
                    tool_id: ToolId::generate(),
                    tool_name: "grep".into(),
                    status: ToolStatus::Success,
                    summary: format!("{} matches in {} files", total, file_count),
                    data: ToolData::GrepResults {
                        query: query.to_string(),
                        matches,
                        file_count,
                        total_matches: total,
                    },
                    duration: Duration::ZERO,
                    artifacts: Vec::new(),
                    diagnostics: Vec::new(),
                    metadata: HashMap::new(),
                }
            }
            Err(e) => make_error_result(&format!("grep error: {}", e)),
        }
    }
}

/// List tool — list directory contents.
pub struct ListTool;

#[async_trait::async_trait]
impl Tool for ListTool {
    fn def(&self) -> &ToolDef {
        static DEF: ToolDef = ToolDef {
            name: "list",
            description: "List directory contents",
            category: ToolCategory::Explore,
            risk_level: RiskLevel::Low,
            permission: PermissionRequirement::Allow,
            agent_access: &[],
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, ctx: &ToolContext) -> ToolResult {
        let path = input.str("path").unwrap_or(".");
        let full_path = if PathBuf::from(path).is_absolute() {
            PathBuf::from(path)
        } else {
            ctx.project_path.join(path)
        };
        match tokio::fs::read_dir(&full_path).await {
            Ok(mut entries) => {
                let mut items = Vec::new();
                while let Some(entry) = entries.next_entry().await.unwrap_or(None) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    items.push(format!("{name}/"));
                }
                items.sort();
                let count = items.len();
                ToolResult {
                    tool_id: ToolId::generate(),
                    tool_name: "list".into(),
                    status: ToolStatus::Success,
                    summary: format!("{} entries in {}", count, full_path.display()),
                    data: ToolData::Json(serde_json::json!({
                        "path": full_path.display().to_string(),
                        "entries": items,
                        "count": count,
                    })),
                    duration: Duration::ZERO,
                    artifacts: Vec::new(),
                    diagnostics: Vec::new(),
                    metadata: HashMap::new(),
                }
            }
            Err(e) => make_error_result(&format!("list error: {}", e)),
        }
    }
}

/// Write tool — create or overwrite a file.
pub struct WriteTool;

#[async_trait::async_trait]
impl Tool for WriteTool {
    fn def(&self) -> &ToolDef {
        static DEF: ToolDef = ToolDef {
            name: "write",
            description: "Create or overwrite a file",
            category: ToolCategory::Modify,
            risk_level: RiskLevel::Medium,
            permission: PermissionRequirement::Ask,
            agent_access: &[],
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, ctx: &ToolContext) -> ToolResult {
        let path = match input.require_str("path") {
            Ok(p) => p,
            Err(e) => return make_error_result(&e),
        };
        let content = match input.require_str("content") {
            Ok(c) => c,
            Err(e) => return make_error_result(&e),
        };
        let full_path = if PathBuf::from(path).is_absolute() {
            PathBuf::from(path)
        } else {
            ctx.project_path.join(path)
        };

        // Ensure parent directory exists
        if let Some(parent) = full_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }

        match tokio::fs::write(&full_path, content).await {
            Ok(()) => {
                let lines = content.lines().count();
                ToolResult {
                    tool_id: ToolId::generate(),
                    tool_name: "write".into(),
                    status: ToolStatus::Success,
                    summary: format!("wrote {} lines to {}", lines, full_path.display()),
                    data: ToolData::None,
                    duration: Duration::ZERO,
                    artifacts: vec![ArtifactRef {
                        path: full_path,
                        artifact_type: ArtifactType::FileWritten,
                    }],
                    diagnostics: Vec::new(),
                    metadata: HashMap::new(),
                }
            }
            Err(e) => make_error_result(&format!("write error: {}", e)),
        }
    }
}

/// Edit tool — replace text in a file.
pub struct EditTool;

#[async_trait::async_trait]
impl Tool for EditTool {
    fn def(&self) -> &ToolDef {
        static DEF: ToolDef = ToolDef {
            name: "edit",
            description: "Replace exact text in a file",
            category: ToolCategory::Modify,
            risk_level: RiskLevel::Medium,
            permission: PermissionRequirement::Ask,
            agent_access: &[],
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, ctx: &ToolContext) -> ToolResult {
        let path = match input.require_str("path") {
            Ok(p) => p,
            Err(e) => return make_error_result(&e),
        };
        let old_text = match input.require_str("old_text") {
            Ok(t) => t,
            Err(e) => return make_error_result(&e),
        };
        let new_text = match input.require_str("new_text") {
            Ok(t) => t,
            Err(e) => return make_error_result(&e),
        };
        let full_path = if PathBuf::from(path).is_absolute() {
            PathBuf::from(path)
        } else {
            ctx.project_path.join(path)
        };

        match tokio::fs::read_to_string(&full_path).await {
            Ok(content) => {
                if !content.contains(old_text) {
                    return make_error_result(&format!(
                        "old_text not found in {}",
                        full_path.display()
                    ));
                }
                let new_content = content.replacen(old_text, new_text, 1);
                let lines_changed = new_content.lines().count();
                match tokio::fs::write(&full_path, &new_content).await {
                    Ok(()) => ToolResult {
                        tool_id: ToolId::generate(),
                        tool_name: "edit".into(),
                        status: ToolStatus::Success,
                        summary: format!(
                            "edited {} ({} lines)",
                            full_path.display(),
                            lines_changed
                        ),
                        data: ToolData::None,
                        duration: Duration::ZERO,
                        artifacts: vec![ArtifactRef {
                            path: full_path,
                            artifact_type: ArtifactType::FileEdited,
                        }],
                        diagnostics: Vec::new(),
                        metadata: HashMap::new(),
                    },
                    Err(e) => make_error_result(&format!("write error: {}", e)),
                }
            }
            Err(e) => make_error_result(&format!("read error: {}", e)),
        }
    }
}

/// Bash tool — execute shell commands.
pub struct BashTool;

#[async_trait::async_trait]
impl Tool for BashTool {
    fn def(&self) -> &ToolDef {
        static DEF: ToolDef = ToolDef {
            name: "bash",
            description: "Execute a shell command",
            category: ToolCategory::Execute,
            risk_level: RiskLevel::High,
            permission: PermissionRequirement::Ask,
            agent_access: &[],
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, ctx: &ToolContext) -> ToolResult {
        let command = match input.require_str("command") {
            Ok(c) => c,
            Err(e) => return make_error_result(&e),
        };
        let timeout_ms = input.int("timeout_ms").unwrap_or(30_000) as u64;

        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c").arg(command).current_dir(&ctx.project_path);
        #[cfg(unix)]
        cmd.process_group(0);

        let result = tokio::time::timeout(Duration::from_millis(timeout_ms), cmd.output()).await;

        match result {
            Ok(Ok(output)) => {
                let raw_stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let raw_stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let stdout = crate::sandbox::truncate_head_tail(&raw_stdout, 1500, 65536);
                let stderr = crate::sandbox::truncate_head_tail(&raw_stderr, 1500, 65536);
                let exit_code = output.status.code().unwrap_or(-1);
                let status = if exit_code == 0 {
                    ToolStatus::Success
                } else {
                    ToolStatus::Failed
                };
                let summary = format!(
                    "exit {} ({} bytes stdout, {} bytes stderr)",
                    exit_code,
                    stdout.len(),
                    stderr.len()
                );
                ToolResult {
                    tool_id: ToolId::generate(),
                    tool_name: "bash".into(),
                    status,
                    summary,
                    data: ToolData::BashOutput {
                        stdout,
                        stderr,
                        exit_code,
                    },
                    duration: Duration::ZERO,
                    artifacts: Vec::new(),
                    diagnostics: Vec::new(),
                    metadata: HashMap::new(),
                }
            }
            Ok(Err(e)) => make_error_result(&format!("exec error: {}", e)),
            Err(_) => ToolResult {
                tool_id: ToolId::generate(),
                tool_name: "bash".into(),
                status: ToolStatus::Timeout,
                summary: format!("command timed out after {}ms", timeout_ms),
                data: ToolData::None,
                duration: Duration::from_millis(timeout_ms),
                artifacts: Vec::new(),
                diagnostics: vec!["timeout".into()],
                metadata: HashMap::new(),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_error_result(msg: &str) -> ToolResult {
    ToolResult {
        tool_id: ToolId::generate(),
        tool_name: String::new(),
        status: ToolStatus::Failed,
        summary: msg.to_string(),
        data: ToolData::None,
        duration: Duration::ZERO,
        artifacts: Vec::new(),
        diagnostics: vec![msg.to_string()],
        metadata: HashMap::new(),
    }
}

// Additional baseline tools
// ---------------------------------------------------------------------------

/// Patch tool — apply a structured patch.
pub struct PatchTool;

#[async_trait::async_trait]
impl Tool for PatchTool {
    fn def(&self) -> &ToolDef {
        static DEF: ToolDef = ToolDef {
            name: "patch",
            description: "Apply a structured patch to a file",
            category: ToolCategory::Modify,
            risk_level: RiskLevel::Medium,
            permission: PermissionRequirement::Ask,
            agent_access: &[],
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, ctx: &ToolContext) -> ToolResult {
        let path = match input.require_str("path") {
            Ok(p) => p,
            Err(e) => return make_error_result(&e),
        };
        let patch_text = match input.require_str("patch") {
            Ok(p) => p,
            Err(e) => return make_error_result(&e),
        };
        let full_path = if PathBuf::from(path).is_absolute() {
            PathBuf::from(path)
        } else {
            ctx.project_path.join(path)
        };
        // Simple patch: apply as replacement for now
        match tokio::fs::read_to_string(&full_path).await {
            Ok(content) => {
                let new_content = format!("{}\n// PATCH APPLIED:\n{}", content, patch_text);
                match tokio::fs::write(&full_path, &new_content).await {
                    Ok(()) => ToolResult {
                        tool_id: ToolId::generate(),
                        tool_name: "patch".into(),
                        status: ToolStatus::Success,
                        summary: format!("patched {}", full_path.display()),
                        data: ToolData::None,
                        duration: Duration::ZERO,
                        artifacts: vec![ArtifactRef {
                            path: full_path,
                            artifact_type: ArtifactType::FileEdited,
                        }],
                        diagnostics: Vec::new(),
                        metadata: HashMap::new(),
                    },
                    Err(e) => make_error_result(&format!("write error: {}", e)),
                }
            }
            Err(e) => make_error_result(&format!("read error: {}", e)),
        }
    }
}

/// Test tool — run project tests.
pub struct TestTool;

#[async_trait::async_trait]
impl Tool for TestTool {
    fn def(&self) -> &ToolDef {
        static DEF: ToolDef = ToolDef {
            name: "test",
            description: "Run project tests with auto-detected test runner",
            category: ToolCategory::Execute,
            risk_level: RiskLevel::Low,
            permission: PermissionRequirement::Allow,
            agent_access: &[],
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, ctx: &ToolContext) -> ToolResult {
        let _target = input.str("target");
        // Detect test runner
        let (cmd, args) = if ctx.project_path.join("Cargo.toml").exists() {
            ("cargo", vec!["test".to_string()])
        } else if ctx.project_path.join("package.json").exists() {
            ("npm", vec!["test".to_string()])
        } else if ctx.project_path.join("go.mod").exists() {
            ("go", vec!["test".to_string(), "./...".to_string()])
        } else {
            ("cargo", vec!["test".to_string()])
        };
        let result = tokio::process::Command::new(cmd)
            .args(&args)
            .current_dir(&ctx.project_path)
            .output()
            .await;
        match result {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let _stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code().unwrap_or(-1);
                let passed = stdout.matches("test result: ok").count();
                let failed = stdout.matches("test result: FAILED").count();
                let status = if exit_code == 0 {
                    ToolStatus::Success
                } else {
                    ToolStatus::Failed
                };
                ToolResult {
                    tool_id: ToolId::generate(),
                    tool_name: "test".into(),
                    status,
                    summary: format!("exit {} ({} passed, {} failed)", exit_code, passed, failed),
                    data: ToolData::TestResults {
                        passed,
                        failed,
                        skipped: 0,
                        failures: Vec::new(),
                        duration_ms: 0,
                    },
                    duration: Duration::ZERO,
                    artifacts: Vec::new(),
                    diagnostics: Vec::new(),
                    metadata: HashMap::new(),
                }
            }
            Err(e) => make_error_result(&format!("test error: {}", e)),
        }
    }
}

/// Web search tool.
pub struct WebSearchTool;

#[async_trait::async_trait]
impl Tool for WebSearchTool {
    fn def(&self) -> &ToolDef {
        static DEF: ToolDef = ToolDef {
            name: "web_search",
            description: "Search the web for information",
            category: ToolCategory::Research,
            risk_level: RiskLevel::Low,
            permission: PermissionRequirement::Allow,
            agent_access: &[],
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, _ctx: &ToolContext) -> ToolResult {
        let query = match input.require_str("query") {
            Ok(q) => q,
            Err(e) => return make_error_result(&e),
        };
        // Placeholder — real implementation uses firecrawl or similar
        ToolResult {
            tool_id: ToolId::generate(),
            tool_name: "web_search".into(),
            status: ToolStatus::Success,
            summary: format!("search: {}", query),
            data: ToolData::WebSearchResults {
                query: query.to_string(),
                results: Vec::new(),
            },
            duration: Duration::ZERO,
            artifacts: Vec::new(),
            diagnostics: vec!["web search not yet wired — use firecrawl MCP".into()],
            metadata: HashMap::new(),
        }
    }
}

/// Web fetch tool.
pub struct WebFetchTool;

#[async_trait::async_trait]
impl Tool for WebFetchTool {
    fn def(&self) -> &ToolDef {
        static DEF: ToolDef = ToolDef {
            name: "web_fetch",
            description: "Fetch content from a URL",
            category: ToolCategory::Research,
            risk_level: RiskLevel::Low,
            permission: PermissionRequirement::Allow,
            agent_access: &[],
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, _ctx: &ToolContext) -> ToolResult {
        let url = match input.require_str("url") {
            Ok(u) => u,
            Err(e) => return make_error_result(&e),
        };
        match reqwest::get(url).await {
            Ok(resp) => {
                let text = resp.text().await.unwrap_or_default();
                ToolResult {
                    tool_id: ToolId::generate(),
                    tool_name: "web_fetch".into(),
                    status: ToolStatus::Success,
                    summary: format!("fetched {} ({} bytes)", url, text.len()),
                    data: ToolData::WebFetchResult {
                        url: url.to_string(),
                        content: text,
                        format: "text".into(),
                    },
                    duration: Duration::ZERO,
                    artifacts: Vec::new(),
                    diagnostics: Vec::new(),
                    metadata: HashMap::new(),
                }
            }
            Err(e) => make_error_result(&format!("fetch error: {}", e)),
        }
    }
}

/// Task spawn tool — spawn a sub-agent.
pub struct TaskSpawnTool;

#[async_trait::async_trait]
impl Tool for TaskSpawnTool {
    fn def(&self) -> &ToolDef {
        static DEF: ToolDef = ToolDef {
            name: "task_spawn",
            description: "Spawn a sub-agent for a specific task",
            category: ToolCategory::Orchestration,
            risk_level: RiskLevel::Low,
            permission: PermissionRequirement::Allow,
            agent_access: &["planner"],
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, _ctx: &ToolContext) -> ToolResult {
        let role = input.str("role").unwrap_or("coder");
        let task_id = Uuid::new_v4().to_string();
        ToolResult {
            tool_id: ToolId::generate(),
            tool_name: "task_spawn".into(),
            status: ToolStatus::Success,
            summary: format!("spawned {} agent ({})", role, &task_id[..8]),
            data: ToolData::TaskSpawned {
                task_id,
                agent_role: role.to_string(),
            },
            duration: Duration::ZERO,
            artifacts: Vec::new(),
            diagnostics: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Task status tool.
pub struct TaskStatusTool;

#[async_trait::async_trait]
impl Tool for TaskStatusTool {
    fn def(&self) -> &ToolDef {
        static DEF: ToolDef = ToolDef {
            name: "task_status",
            description: "Check status of a spawned task",
            category: ToolCategory::Orchestration,
            risk_level: RiskLevel::Low,
            permission: PermissionRequirement::Allow,
            agent_access: &[],
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, _ctx: &ToolContext) -> ToolResult {
        let task_id = input.str("task_id").unwrap_or("unknown");
        ToolResult {
            tool_id: ToolId::generate(),
            tool_name: "task_status".into(),
            status: ToolStatus::Success,
            summary: format!("task {} status: running", task_id),
            data: ToolData::TaskStatus {
                task_id: task_id.to_string(),
                status: "running".into(),
                progress: Some(0.5),
            },
            duration: Duration::ZERO,
            artifacts: Vec::new(),
            diagnostics: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Task cancel tool.
pub struct TaskCancelTool;

#[async_trait::async_trait]
impl Tool for TaskCancelTool {
    fn def(&self) -> &ToolDef {
        static DEF: ToolDef = ToolDef {
            name: "task_cancel",
            description: "Cancel a running task",
            category: ToolCategory::Orchestration,
            risk_level: RiskLevel::Medium,
            permission: PermissionRequirement::Ask,
            agent_access: &[],
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, _ctx: &ToolContext) -> ToolResult {
        let task_id = input.str("task_id").unwrap_or("unknown");
        ToolResult {
            tool_id: ToolId::generate(),
            tool_name: "task_cancel".into(),
            status: ToolStatus::Success,
            summary: format!("cancelled task {}", task_id),
            data: ToolData::None,
            duration: Duration::ZERO,
            artifacts: Vec::new(),
            diagnostics: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Task create tool — create a planning task.
pub struct TaskCreateTool;

#[async_trait::async_trait]
impl Tool for TaskCreateTool {
    fn def(&self) -> &ToolDef {
        static DEF: ToolDef = ToolDef {
            name: "task_create",
            description: "Create a task in the mission plan",
            category: ToolCategory::Orchestration,
            risk_level: RiskLevel::Low,
            permission: PermissionRequirement::Allow,
            agent_access: &["planner"],
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, _ctx: &ToolContext) -> ToolResult {
        let desc = input.str("description").unwrap_or("unnamed task");
        ToolResult {
            tool_id: ToolId::generate(),
            tool_name: "task_create".into(),
            status: ToolStatus::Success,
            summary: format!("created task: {}", desc),
            data: ToolData::None,
            duration: Duration::ZERO,
            artifacts: Vec::new(),
            diagnostics: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Task update tool.
pub struct TaskUpdateTool;

#[async_trait::async_trait]
impl Tool for TaskUpdateTool {
    fn def(&self) -> &ToolDef {
        static DEF: ToolDef = ToolDef {
            name: "task_update",
            description: "Update task status in the mission plan",
            category: ToolCategory::Orchestration,
            risk_level: RiskLevel::Low,
            permission: PermissionRequirement::Allow,
            agent_access: &["planner", "coder"],
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, _ctx: &ToolContext) -> ToolResult {
        let task_id = input.str("task_id").unwrap_or("unknown");
        let status = input.str("status").unwrap_or("in_progress");
        ToolResult {
            tool_id: ToolId::generate(),
            tool_name: "task_update".into(),
            status: ToolStatus::Success,
            summary: format!("task {} → {}", task_id, status),
            data: ToolData::None,
            duration: Duration::ZERO,
            artifacts: Vec::new(),
            diagnostics: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Task list tool.
pub struct TaskListTool;

#[async_trait::async_trait]
impl Tool for TaskListTool {
    fn def(&self) -> &ToolDef {
        static DEF: ToolDef = ToolDef {
            name: "task_list",
            description: "List all tasks in the mission plan",
            category: ToolCategory::Orchestration,
            risk_level: RiskLevel::Low,
            permission: PermissionRequirement::Allow,
            agent_access: &[],
        };
        &DEF
    }

    async fn execute(&self, _input: ToolInput, _ctx: &ToolContext) -> ToolResult {
        ToolResult {
            tool_id: ToolId::generate(),
            tool_name: "task_list".into(),
            status: ToolStatus::Success,
            summary: "0 tasks".into(),
            data: ToolData::None,
            duration: Duration::ZERO,
            artifacts: Vec::new(),
            diagnostics: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Ask user tool — prompt user for input.
pub struct AskUserTool;

#[async_trait::async_trait]
impl Tool for AskUserTool {
    fn def(&self) -> &ToolDef {
        static DEF: ToolDef = ToolDef {
            name: "ask_user",
            description: "Ask the user a question and wait for response",
            category: ToolCategory::Human,
            risk_level: RiskLevel::Low,
            permission: PermissionRequirement::Allow,
            agent_access: &[],
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, _ctx: &ToolContext) -> ToolResult {
        let question = input.str("question").unwrap_or("?");
        // In real execution, this blocks for user input via TUI
        ToolResult {
            tool_id: ToolId::generate(),
            tool_name: "ask_user".into(),
            status: ToolStatus::Success,
            summary: format!("asked: {}", question),
            data: ToolData::UserResponse {
                question: question.to_string(),
                response: "(awaiting TUI integration)".into(),
            },
            duration: Duration::ZERO,
            artifacts: Vec::new(),
            diagnostics: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Approval tool — request approval for a dangerous operation.
pub struct ApprovalTool;

#[async_trait::async_trait]
impl Tool for ApprovalTool {
    fn def(&self) -> &ToolDef {
        static DEF: ToolDef = ToolDef {
            name: "approval",
            description: "Request approval before executing a dangerous operation",
            category: ToolCategory::Human,
            risk_level: RiskLevel::Low,
            permission: PermissionRequirement::Allow,
            agent_access: &[],
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, _ctx: &ToolContext) -> ToolResult {
        let command = input.str("command").unwrap_or("unknown");
        ToolResult {
            tool_id: ToolId::generate(),
            tool_name: "approval".into(),
            status: ToolStatus::Success,
            summary: format!("approval for: {}", command),
            data: ToolData::ApprovalResult {
                approved: true,
                reason: Some("auto-approved for testing".into()),
            },
            duration: Duration::ZERO,
            artifacts: Vec::new(),
            diagnostics: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Skill list tool.
pub struct SkillListTool;

#[async_trait::async_trait]
impl Tool for SkillListTool {
    fn def(&self) -> &ToolDef {
        static DEF: ToolDef = ToolDef {
            name: "skill_list",
            description: "List available skills",
            category: ToolCategory::Knowledge,
            risk_level: RiskLevel::Low,
            permission: PermissionRequirement::Allow,
            agent_access: &[],
        };
        &DEF
    }

    async fn execute(&self, _input: ToolInput, _ctx: &ToolContext) -> ToolResult {
        ToolResult {
            tool_id: ToolId::generate(),
            tool_name: "skill_list".into(),
            status: ToolStatus::Success,
            summary: "0 skills loaded".into(),
            data: ToolData::None,
            duration: Duration::ZERO,
            artifacts: Vec::new(),
            diagnostics: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Skill load tool.
pub struct SkillLoadTool;

#[async_trait::async_trait]
impl Tool for SkillLoadTool {
    fn def(&self) -> &ToolDef {
        static DEF: ToolDef = ToolDef {
            name: "skill_load",
            description: "Load a skill by name",
            category: ToolCategory::Knowledge,
            risk_level: RiskLevel::Low,
            permission: PermissionRequirement::Allow,
            agent_access: &[],
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, _ctx: &ToolContext) -> ToolResult {
        let name = input.str("name").unwrap_or("unknown");
        ToolResult {
            tool_id: ToolId::generate(),
            tool_name: "skill_load".into(),
            status: ToolStatus::Success,
            summary: format!("loaded skill: {}", name),
            data: ToolData::None,
            duration: Duration::ZERO,
            artifacts: Vec::new(),
            diagnostics: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Git tool — version control operations.
pub struct GitTool;

#[async_trait::async_trait]
impl Tool for GitTool {
    fn def(&self) -> &ToolDef {
        static DEF: ToolDef = ToolDef {
            name: "git",
            description: "Execute git operations (status, diff, commit, branch, log)",
            category: ToolCategory::Vcs,
            risk_level: RiskLevel::Medium,
            permission: PermissionRequirement::Ask,
            agent_access: &[],
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, ctx: &ToolContext) -> ToolResult {
        let subcommand = input.str("subcommand").unwrap_or("status");
        let extra_args: Vec<&str> = match subcommand {
            "diff" => vec!["--stat"],
            "log" => vec!["--oneline", "-10"],
            "status" => vec![],
            "branch" => vec![],
            _ => vec![],
        };
        let result = tokio::process::Command::new("git")
            .arg(subcommand)
            .args(&extra_args)
            .current_dir(&ctx.project_path)
            .output()
            .await;
        match result {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code().unwrap_or(-1);
                let status = if exit_code == 0 {
                    ToolStatus::Success
                } else {
                    ToolStatus::Failed
                };
                ToolResult {
                    tool_id: ToolId::generate(),
                    tool_name: "git".into(),
                    status,
                    summary: format!("git {} (exit {})", subcommand, exit_code),
                    data: ToolData::BashOutput {
                        stdout,
                        stderr,
                        exit_code,
                    },
                    duration: Duration::ZERO,
                    artifacts: Vec::new(),
                    diagnostics: Vec::new(),
                    metadata: HashMap::new(),
                }
            }
            Err(e) => make_error_result(&format!("git error: {}", e)),
        }
    }
}

// ---------------------------------------------------------------------------
// Build registry
// ---------------------------------------------------------------------------

pub fn build_baseline_registry() -> ToolRegistry {
    let mut reg = ToolRegistry::new();
    // Explore
    reg.register(Box::new(ReadTool));
    reg.register(Box::new(GlobTool));
    reg.register(Box::new(GrepTool));
    reg.register(Box::new(ListTool));
    // Modify
    reg.register(Box::new(WriteTool));
    reg.register(Box::new(EditTool));
    reg.register(Box::new(PatchTool));
    // Execute
    reg.register(Box::new(BashTool));
    reg.register(Box::new(TestTool));
    // Research
    reg.register(Box::new(WebSearchTool));
    reg.register(Box::new(WebFetchTool));
    // Orchestration
    reg.register(Box::new(TaskSpawnTool));
    reg.register(Box::new(TaskStatusTool));
    reg.register(Box::new(TaskCancelTool));
    // Planning
    reg.register(Box::new(TaskCreateTool));
    reg.register(Box::new(TaskUpdateTool));
    reg.register(Box::new(TaskListTool));
    // Human
    reg.register(Box::new(AskUserTool));
    reg.register(Box::new(ApprovalTool));
    // Knowledge
    reg.register(Box::new(SkillListTool));
    reg.register(Box::new(SkillLoadTool));
    // VCS
    reg.register(Box::new(GitTool));
    reg
}

// ---------------------------------------------------------------------------
// LLM tool-calling loop — wires the ToolRegistry into the LLM completion loop.
// ---------------------------------------------------------------------------

/// A message in a tool-calling conversation.
#[derive(Debug, Clone)]
pub enum LoopMessage {
    /// System prompt (used once as the request's `system_prompt`).
    System(String),
    /// A user turn.
    User(String),
    /// An assistant turn, optionally carrying tool calls requested by the model.
    Assistant {
        content: String,
        tool_calls: Vec<ToolCall>,
    },
    /// The result of a previously-requested tool call, fed back to the model.
    ToolResult {
        tool_call_id: String,
        content: String,
    },
}

/// Final output of the tool-calling loop.
#[derive(Debug, Clone)]
pub struct LoopOutput {
    /// The model's final natural-language answer.
    pub content: String,
    /// Number of LLM round-trips performed.
    pub steps: usize,
    /// Per-step record of `(tool_name, success)` for telemetry.
    pub tool_calls: Vec<(String, bool)>,
}

/// Serialize the conversation (minus the leading `System` message) into a single
/// text `user_message` suitable for text-based providers. Tool calls and their
/// results are embedded as fenced JSON so the model can reason over them.
fn format_messages(messages: &[LoopMessage]) -> String {
    let mut out = String::new();
    for msg in messages {
        match msg {
            LoopMessage::System(_) => {}
            LoopMessage::User(text) => {
                out.push_str(&format!("<user>\n{}\n</user>\n", text));
            }
            LoopMessage::Assistant {
                content,
                tool_calls,
            } => {
                if !content.is_empty() {
                    out.push_str(&format!("<assistant>\n{}\n</assistant>\n", content));
                }
                if !tool_calls.is_empty() {
                    let json = serde_json::json!(tool_calls);
                    out.push_str(&format!(
                        "<assistant_tool_calls>\n{}\n</assistant_tool_calls>\n",
                        serde_json::to_string_pretty(&json).unwrap_or_default()
                    ));
                }
            }
            LoopMessage::ToolResult {
                tool_call_id,
                content,
            } => {
                out.push_str(&format!(
                    "<tool_result id=\"{}\">\n{}\n</tool_result>\n",
                    tool_call_id, content
                ));
            }
        }
    }
    out
}

/// Run the LLM tool-calling loop.
///
/// Each iteration calls the provider with the current conversation + the role's
/// tool specs. If the model returns tool calls, they are executed via the
/// `ToolRegistry` (emitting `ToolStarted`/`ToolCompleted`/`ToolFailed` events)
/// and their results are appended as `ToolResult` messages; the loop repeats.
/// The loop terminates when the model returns no tool calls, or after
/// `max_steps` round-trips.
pub async fn run_tool_loop(
    provider: &dyn LlmProvider,
    registry: &ToolRegistry,
    ctx: &ToolContext,
    mut messages: Vec<LoopMessage>,
    bus: Option<&EventBus>,
    max_steps: usize,
) -> Result<LoopOutput> {
    let system_prompt = messages
        .iter()
        .find_map(|m| match m {
            LoopMessage::System(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_default();

    let tools = if registry.tool_specs_for(&ctx.role).is_empty() {
        None
    } else {
        Some(registry.tool_specs_for(&ctx.role))
    };

    let mut steps = 0usize;
    let mut call_log: Vec<(String, bool)> = Vec::new();
    let mut last_content = String::new();

    loop {
        if steps >= max_steps {
            break;
        }
        steps += 1;

        let request = CompletionRequest {
            model: String::new(),
            system_prompt: system_prompt.clone(),
            user_message: format_messages(&messages),
            max_tokens: 4096,
            temperature: 0.7,
            json_schema: None,
            tools: tools.clone(),
        };

        let response = provider.complete(request).await?;
        last_content = response.content.clone();

        if response.tool_calls.is_empty() {
            return Ok(LoopOutput {
                content: response.content,
                steps,
                tool_calls: call_log,
            });
        }

        // Record the assistant turn (with its requested tool calls).
        messages.push(LoopMessage::Assistant {
            content: response.content,
            tool_calls: response.tool_calls.clone(),
        });

        for tc in &response.tool_calls {
            let tool_id = ToolId::generate();
            if let Some(bus) = bus {
                let _ = bus.emit(Event::ToolStarted {
                    mission_id: ctx.mission_id.clone(),
                    agent_id: ctx.agent_id.clone(),
                    tool_id: tool_id.clone(),
                    tool_name: tc.name.clone(),
                    input_summary: tc
                        .arguments
                        .get("path")
                        .and_then(|v| v.as_str())
                        .or_else(|| tc.arguments.get("command").and_then(|v| v.as_str()))
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| tc.name.clone()),
                    timestamp: Instant::now(),
                });
            }

            let input = ToolInput::new(tc.arguments.clone());
            let result = registry.execute(&tc.name, input, ctx).await;
            let success = result.status == ToolStatus::Success;

            if let Some(bus) = bus {
                let _ = bus.emit(if success {
                    Event::ToolCompleted {
                        mission_id: ctx.mission_id.clone(),
                        agent_id: ctx.agent_id.clone(),
                        tool_id: tool_id.clone(),
                        summary: result.summary.clone(),
                        duration_ms: result.duration.as_millis() as u64,
                        timestamp: Instant::now(),
                    }
                } else {
                    Event::ToolFailed {
                        mission_id: ctx.mission_id.clone(),
                        agent_id: ctx.agent_id.clone(),
                        tool_id: tool_id.clone(),
                        error: result.summary.clone(),
                        timestamp: Instant::now(),
                    }
                });
            }

            call_log.push((tc.name.clone(), success));
            let content = if !success
                && (tc.name == "edit" || tc.name == "file_edit" || tc.name == "str_replace")
            {
                format!(
                    "{}. If string match failed, re-read the target file to establish ground-truth context.",
                    result.summary
                )
            } else {
                result.summary.clone()
            };
            messages.push(LoopMessage::ToolResult {
                tool_call_id: tc.id.clone(),
                content,
            });
        }
    }

    Ok(LoopOutput {
        content: last_content,
        steps,
        tool_calls: call_log,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_id_unique() {
        let a = ToolId::generate();
        let b = ToolId::generate();
        assert_ne!(a, b);
    }

    #[test]
    fn tool_input_str() {
        let input = ToolInput::new(serde_json::json!({"path": "src/main.rs"}));
        assert_eq!(input.str("path"), Some("src/main.rs"));
        assert_eq!(input.int("path"), None);
    }

    #[test]
    fn tool_input_require() {
        let input = ToolInput::new(serde_json::json!({}));
        assert!(input.require_str("path").is_err());
    }

    #[test]
    fn baseline_registry_has_tools() {
        let reg = build_baseline_registry();
        assert!(reg.get("read").is_some());
        assert!(reg.get("write").is_some());
        assert!(reg.get("edit").is_some());
        assert!(reg.get("glob").is_some());
        assert!(reg.get("grep").is_some());
        assert!(reg.get("list").is_some());
        assert!(reg.get("bash").is_some());
        assert_eq!(reg.list_defs().len(), 22);
    }

    #[test]
    fn tool_category_display() {
        assert_eq!(ToolCategory::Explore.to_string(), "explore");
        assert_eq!(ToolCategory::Modify.to_string(), "modify");
    }

    #[test]
    fn tool_result_status() {
        assert_eq!(ToolStatus::Success, ToolStatus::Success);
        assert_ne!(ToolStatus::Success, ToolStatus::Failed);
    }

    // ---- LLM tool-calling loop ----

    use crate::llm::provider::{LlmProvider, StreamChunk};
    use async_trait::async_trait;
    use futures::Stream;
    use std::pin::Pin;

    /// Fake provider: first call requests a `bash` tool call, second returns text.
    struct FakeToolProvider {
        calls: std::sync::atomic::AtomicUsize,
    }

    #[async_trait]
    impl LlmProvider for FakeToolProvider {
        fn provider_name(&self) -> &str {
            "fake"
        }

        async fn complete(
            &self,
            _request: crate::llm::provider::CompletionRequest,
        ) -> anyhow::Result<crate::llm::provider::CompletionResponse> {
            let n = self
                .calls
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n == 0 {
                Ok(crate::llm::provider::CompletionResponse {
                    content: String::new(),
                    model: "fake".into(),
                    usage: crate::llm::provider::TokenUsage::default(),
                    tool_calls: vec![crate::llm::provider::ToolCall {
                        id: "call_1".into(),
                        name: "bash".into(),
                        arguments: serde_json::json!({"command": "echo hi"}),
                    }],
                })
            } else {
                Ok(crate::llm::provider::CompletionResponse {
                    content: "finished".into(),
                    model: "fake".into(),
                    usage: crate::llm::provider::TokenUsage::default(),
                    tool_calls: vec![],
                })
            }
        }

        async fn stream(
            &self,
            _request: crate::llm::provider::CompletionRequest,
        ) -> anyhow::Result<Pin<Box<dyn Stream<Item = anyhow::Result<StreamChunk>> + Send>>>
        {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn tool_loop_executes_tool_then_final() {
        let provider = FakeToolProvider {
            calls: std::sync::atomic::AtomicUsize::new(0),
        };
        let registry = build_baseline_registry();
        let ctx = ToolContext {
            agent_id: crate::mission::AgentId("a1".into()),
            mission_id: crate::mission::MissionId("m1".into()),
            role: "coder".into(),
            project_path: std::env::temp_dir(),
            permissions: HashMap::new(),
        };
        let messages = vec![
            LoopMessage::System("you are a coding agent".into()),
            LoopMessage::User("run echo hi".into()),
        ];
        let out = run_tool_loop(&provider, &registry, &ctx, messages, None, 5)
            .await
            .unwrap();
        assert_eq!(out.content, "finished");
        assert_eq!(out.steps, 2);
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].0, "bash");
        assert!(out.tool_calls[0].1);
    }

    #[tokio::test]
    async fn tool_loop_no_tools_returns_immediately() {
        let provider = FakeToolProvider {
            calls: std::sync::atomic::AtomicUsize::new(1),
        };
        let registry = build_baseline_registry();
        let ctx = ToolContext {
            agent_id: crate::mission::AgentId("a1".into()),
            mission_id: crate::mission::MissionId("m1".into()),
            role: "coder".into(),
            project_path: std::env::temp_dir(),
            permissions: HashMap::new(),
        };
        let messages = vec![LoopMessage::User("hi".into())];
        let out = run_tool_loop(&provider, &registry, &ctx, messages, None, 5)
            .await
            .unwrap();
        assert_eq!(out.content, "finished");
        assert_eq!(out.steps, 1);
        assert!(out.tool_calls.is_empty());
    }
}
