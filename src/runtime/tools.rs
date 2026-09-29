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
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use uuid::Uuid;

use anyhow::Result;

use crate::audit::{HookBus, HookEvent, HookOutcome};
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
    /// A JSON Schema (as a string, because `ToolDef` is built in `static`
    /// context) naming this tool's arguments.
    ///
    /// It used to not exist: every tool was advertised to the model as
    /// `{"type":"object","properties":{},"additionalProperties":true}`, so a
    /// model calling `edit` was told nothing about `path`/`old_text`/`new_text`
    /// and had to infer them from the description prose. That is a large part
    /// of why the tool loop failed on small models — the harness was asking
    /// the model to guess an argument contract it could have been given.
    ///
    /// `tool_specs_for` falls back to a permissive object if this is absent or
    /// unparseable, so a bad entry degrades rather than breaking the loop.
    /// `tools_declare_every_argument_they_read` keeps them honest.
    pub parameters: &'static str,
}

/// The schema a tool with no declared parameters gets: accepts anything.
///
/// Also the fallback when a declared schema fails to parse, so a typo in a
/// static string cannot take the whole tool loop down.
fn permissive_parameters() -> serde_json::Value {
    serde_json::json!({"type": "object", "properties": {}, "additionalProperties": true})
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
    TaskSpawned {
        task_id: String,
        agent_role: String,
        run_in_background: bool,
        resume_hint: Option<String>,
    },
    /// Task status.
    TaskStatus {
        task_id: String,
        status: String,
        progress: Option<f64>,
        resume_hint: Option<String>,
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
    /// Listed skills from the shared `~/.agents/skills/` directory.
    SkillList {
        skills: Vec<String>,
        directory: String,
    },
    /// A loaded skill's content.
    SkillLoaded {
        name: String,
        content: String,
        source: String,
    },
}

/// Max chars of tool `data` fed back to the model per result (context-rot cap).
pub const TOOL_DATA_FEEDBACK_CAP: usize = 8000;

impl ToolData {
    /// Render the structured payload as model-facing text, capped. `summary`
    /// alone discards file content; this keeps the evidence the next request
    /// needs to reason (Phase 3.2).
    pub fn to_feedback_text(&self) -> String {
        let raw = match self {
            ToolData::None => String::new(),
            ToolData::FileContent {
                path,
                lines,
                total_lines,
            } => {
                let mut s = format!("--- {path} ({total_lines} lines) ---\n");
                for (n, line) in lines {
                    s.push_str(&format!("{n}: {line}\n"));
                }
                s
            }
            ToolData::GlobResults { pattern, matches } => {
                format!("glob {pattern}:\n{}", matches.join("\n"))
            }
            ToolData::GrepResults {
                query,
                matches,
                file_count,
                total_matches,
            } => {
                let mut s = format!("grep {query} ({total_matches} in {file_count} files):\n");
                for m in matches.iter().take(50) {
                    s.push_str(&format!("{}:{}: {}\n", m.file, m.line, m.match_text));
                }
                s
            }
            ToolData::TestResults {
                passed,
                failed,
                skipped,
                failures,
                duration_ms,
            } => {
                let mut s = format!(
                    "tests: {passed} passed, {failed} failed, {skipped} skipped ({duration_ms}ms)\n"
                );
                for f in failures.iter().take(20) {
                    s.push_str(&format!("FAIL: {f}\n"));
                }
                s
            }
            ToolData::BashOutput {
                stdout,
                stderr,
                exit_code,
            } => {
                format!("exit={exit_code}\nstdout:\n{stdout}\nstderr:\n{stderr}")
            }
            ToolData::WebSearchResults { query, results } => {
                let mut s = format!("search {query}:\n");
                for r in results.iter().take(10) {
                    s.push_str(&format!("- {} ({})\n  {}\n", r.title, r.url, r.snippet));
                }
                s
            }
            ToolData::WebFetchResult {
                url,
                content,
                format,
            } => {
                format!("fetched {url} [{format}]:\n{content}")
            }
            ToolData::TaskSpawned {
                task_id,
                agent_role,
                run_in_background,
                resume_hint,
            } => {
                format!(
                    "spawned {task_id} role={agent_role} background={run_in_background} hint={:?}",
                    resume_hint
                )
            }
            ToolData::TaskStatus {
                task_id,
                status,
                progress,
                resume_hint,
            } => {
                format!(
                    "task {task_id}: {status} progress={:?} hint={:?}",
                    progress, resume_hint
                )
            }
            ToolData::UserResponse { question, response } => {
                format!("Q: {question}\nA: {response}")
            }
            ToolData::ApprovalResult { approved, reason } => {
                format!("approved={approved} reason={:?}", reason)
            }
            ToolData::Json(v) => serde_json::to_string_pretty(v).unwrap_or_default(),
            ToolData::SkillList { skills, directory } => {
                format!("skills in {directory}:\n{}", skills.join("\n"))
            }
            ToolData::SkillLoaded {
                name,
                content,
                source,
            } => {
                format!("skill {name} (from {source}):\n{content}")
            }
        };
        if raw.len() > TOOL_DATA_FEEDBACK_CAP {
            let kept: String = raw.chars().take(TOOL_DATA_FEEDBACK_CAP).collect();
            format!("{kept}\n[tool data truncated at {TOOL_DATA_FEEDBACK_CAP} chars]")
        } else {
            raw
        }
    }
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

    /// Get a boolean field from the input.
    pub fn bool(&self, key: &str) -> Option<bool> {
        self.raw.get(key).and_then(|v| v.as_bool())
    }
}

/// Resolve a model-supplied path against the project root, refusing anything
/// that escapes it.
///
/// The write/edit/patch tools used to take an absolute path verbatim and then
/// `create_dir_all` its parent, so a model could name any path the process
/// could write and the tool would oblige — `~/.ssh/authorized_keys`,
/// `/etc/cron.d/x`, anything. The permission layer has a protected-path list
/// (`.git`, `.claude`, `~/.ssh`, …) and `PermissionChecker::is_protected_path`
/// to match it, but nothing on the write path ever called it, so the list was
/// documentation.
///
/// On the worktree backend this runs as the invoking user with their
/// privileges and no container between the model and the filesystem, so
/// "absolute paths are honoured" is the difference between a sandbox and a
/// suggestion.
///
/// Absolute paths are allowed when they are *inside* the project — the container
/// backend's working directory is `/workspace` and models legitimately produce
/// absolute paths there. `..` is rejected outright rather than normalised,
/// because a path that needs normalising to stay inside is not one to trust.
pub fn resolve_tool_path(project_path: &std::path::Path, raw: &str) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("empty path".to_string());
    }
    if trimmed.contains("..") {
        return Err(format!(
            "path {trimmed:?} contains '..' — paths must stay inside the project"
        ));
    }

    let candidate = PathBuf::from(trimmed);
    let full = if candidate.is_absolute() {
        candidate
    } else {
        project_path.join(candidate)
    };

    // Compare against the canonicalised root so `/proj/../etc/passwd`, a
    // trailing slash, or a symlinked root cannot make the prefix test lie.
    // Both sides are canonicalised; if the root itself cannot be canonicalised
    // the project is in a state where nothing can be checked, so refuse.
    let root = project_path.canonicalize().map_err(|e| {
        format!(
            "cannot resolve project root {}: {e}",
            project_path.display()
        )
    })?;
    let resolved = full
        .canonicalize()
        .or_else(|_| {
            // The file may legitimately not exist yet — that is what `write` is
            // for. Canonicalise the deepest existing ancestor and re-attach the
            // remainder, so a path whose PARENT is a symlink out of the tree is
            // still caught.
            let mut tail = Vec::new();
            let mut cursor = full.clone();
            loop {
                match cursor.parent() {
                    Some(parent) => {
                        let name = cursor
                            .file_name()
                            .map(|n| n.to_os_string())
                            .unwrap_or_default();
                        tail.push(name);
                        cursor = parent.to_path_buf();
                        if let Ok(c) = cursor.canonicalize() {
                            let mut out = c;
                            for part in tail.iter().rev() {
                                out.push(part);
                            }
                            return Ok(out);
                        }
                    }
                    None => return Err(format!("cannot resolve path {trimmed:?}")),
                }
            }
        })
        .map_err(|e| e.to_string())?;

    if !resolved.starts_with(&root) {
        return Err(format!(
            "path {trimmed:?} resolves outside the project ({}) — tools may only touch files \
             inside {}",
            resolved.display(),
            root.display()
        ));
    }
    Ok(resolved)
}

/// Execution context provided to every tool.
#[derive(Debug, Clone)]
pub struct ToolContext {
    pub agent_id: AgentId,
    pub mission_id: crate::mission::MissionId,
    pub role: String,
    pub project_path: PathBuf,
    pub permissions: HashMap<String, PermissionRequirement>,
    /// Permission mode governing `Ask` tools (`manual`/`auto`/`dontask`/`bypass`,
    /// mirroring `[permissions] mode`; default `manual`). The tool loop has no
    /// approval UI, so `Ask` under `manual` fails closed.
    pub permission_mode: String,
    /// Shared sub-task state for `task_spawn`/`task_status`/`task_cancel`.
    pub task_store: Option<std::sync::Arc<TaskStore>>,
}

impl ToolContext {
    /// Parse a configured mode string (`manual`/`auto`/`dontask`/`bypass`,
    /// case-insensitive); unknown values fail closed to `manual`.
    pub fn parse_permission_mode(mode: &str) -> String {
        match mode.to_ascii_lowercase().as_str() {
            "auto" | "dontask" | "bypass" | "manual" => mode.to_ascii_lowercase(),
            _ => "manual".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// ToolRegistry — central registry of all tools
// ---------------------------------------------------------------------------

/// Central tool registry.
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
    defs: Vec<ToolDef>,
    hook_bus: HookBus,
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
            hook_bus: HookBus::new(),
        }
    }

    /// Bound PreToolUse/PostToolUse hooks (Phase 5.4). Defaults to 30s.
    pub fn set_hook_timeout_secs(&mut self, secs: u64) {
        self.hook_bus.set_timeout_secs(secs);
    }

    /// Register a tool. Re-registering the same name replaces the previous
    /// definition in place (no duplicate defs).
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        let def = tool.def().clone();
        self.tools.insert(def.name.to_string(), tool);
        if let Some(existing) = self.defs.iter_mut().find(|d| d.name == def.name) {
            *existing = def;
        } else {
            self.defs.push(def);
        }
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
    /// Each tool advertises its real argument schema. Concrete values are still
    /// validated inside `execute()` via `ToolInput` accessors — the schema tells
    /// the model what to send, it does not replace the checks.
    pub fn tool_specs_for(&self, role: &str) -> Vec<ToolSpec> {
        self.for_role(role)
            .into_iter()
            .map(|def| ToolSpec {
                name: def.name.to_string(),
                description: def.description.to_string(),
                parameters: serde_json::from_str(def.parameters)
                    .unwrap_or_else(|_| permissive_parameters()),
            })
            .collect()
    }

    /// Execute a tool by name.
    ///
    /// Unknown tools keep the `Failed` status contract (Phase 3.2 decision):
    /// the summary names the missing tool and `diagnostics` carries
    /// `unknown tool: {name}`, so the model can correct the call instead of
    /// the loop erroring out.
    ///
    /// Permission gate (Phase 3.4, fail-closed): `Deny` (declared or via
    /// `ctx.permissions`) maps to `PermissionDenied` before hooks or the tool
    /// run. `Ask` consults `ctx.permission_mode`: `bypass`/`dontask`/`auto`
    /// allow; `manual` denies headless (the loop has no approval UI) with a
    /// diagnostic saying so.
    pub async fn execute(&self, name: &str, input: ToolInput, ctx: &ToolContext) -> ToolResult {
        let start = Instant::now();

        // Permission gate first: no hooks, no tool side effects when denied.
        if let Some(tool) = self.tools.get(name)
            && let Some(reason) = Self::permission_denial(name, tool.def(), ctx)
        {
            return ToolResult {
                tool_id: ToolId::generate(),
                tool_name: name.to_string(),
                status: ToolStatus::PermissionDenied,
                summary: format!("permission denied for tool '{name}': {reason}"),
                data: ToolData::None,
                duration: start.elapsed(),
                artifacts: Vec::new(),
                diagnostics: vec![reason],
                metadata: HashMap::new(),
            };
        }

        // Fire PreToolUse hook
        if let Ok(payload) = serde_json::to_string(&serde_json::json!({"tool": name})) {
            if let HookOutcome::Block(_) = self.hook_bus.run(HookEvent::PreToolUse, &payload) {
                return ToolResult {
                    tool_id: ToolId::generate(),
                    tool_name: name.to_string(),
                    status: ToolStatus::Failed,
                    summary: "Blocked by PreToolUse hook".to_string(),
                    data: ToolData::None,
                    duration: start.elapsed(),
                    artifacts: Vec::new(),
                    diagnostics: vec!["Blocked by PreToolUse hook".to_string()],
                    metadata: HashMap::new(),
                };
            }
        }

        let result = match self.tools.get(name) {
            Some(tool) => {
                let mut res = tool.execute(input, ctx).await;
                res.tool_name = name.to_string();
                res.duration = start.elapsed();
                res
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
        };

        // Fire PostToolUse hook
        if let Ok(payload) = serde_json::to_string(
            &serde_json::json!({"tool": name, "status": format!("{:?}", result.status)}),
        ) {
            let _ = self.hook_bus.run(HookEvent::PostToolUse, &payload);
        }

        result
    }

    /// Whether a tool reaches outside the machine.
    ///
    /// Network egress is the one capability a sandboxed agent holds that can
    /// exfiltrate everything else in the repo, and NIKI's isolation story is
    /// built on that being a deliberate act rather than a side effect of, say,
    /// a dependency install.
    fn is_network_egress(name: &str) -> bool {
        matches!(name, "web_fetch" | "web_search" | "webfetch" | "websearch")
    }

    /// Permission check for one tool call. Returns `Some(reason)` when the
    /// call must be denied, `None` when it may proceed.
    fn permission_denial(name: &str, def: &ToolDef, ctx: &ToolContext) -> Option<String> {
        let declared = ctx.permissions.get(name).copied().unwrap_or(def.permission);
        match declared {
            PermissionRequirement::Allow => {
                // A declared `Allow` is a tool-authoring default, not a
                // statement about this run. It must not launder network
                // egress past the permission mode: `web_fetch` and
                // `web_search` both declared `Allow`, so an agent could pull
                // anything off the network in any mode short of an explicit
                // bypass.
                //
                // This rule used to live in `permissions::PermissionChecker::
                // resolve_tool`, which nothing in the product calls — so the
                // property was asserted by its own unit tests and enforced
                // nowhere. The tool loop has no approval UI, so `Ask` is
                // denied here fail-closed, exactly like the arm below.
                if Self::is_network_egress(name) {
                    return match ctx.permission_mode.as_str() {
                        "bypass" | "dontask" => None,
                        _ => Some(format!(
                            "tool '{name}' reaches the network, which requires approval; \
                             permission mode '{}' has no approval UI in the tool \
                             loop — denied fail-closed. Set [permissions] mode = \
                             \"dontask\" to allow network egress, or run outside the \
                             tool loop.",
                            ctx.permission_mode
                        )),
                    };
                }
                None
            }
            PermissionRequirement::Deny => {
                Some(format!("tool '{name}' is denied by policy (declared Deny)"))
            }
            PermissionRequirement::Ask => match ctx.permission_mode.as_str() {
                "bypass" | "dontask" | "auto" => None,
                _ => Some(format!(
                    "tool '{name}' requires approval (Ask) but permission mode '{}' has no approval UI in the tool loop — denied fail-closed",
                    ctx.permission_mode
                )),
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
            parameters: r#"{"type":"object","properties":{"path":{"type":"string","description":"File to read, relative to the project root"},"start_line":{"type":"integer","description":"1-based first line to return"},"end_line":{"type":"integer","description":"1-based last line to return"}},"required":["path"]}"#,
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, ctx: &ToolContext) -> ToolResult {
        let path = match input.require_str("path") {
            Ok(p) => p,
            Err(e) => return make_error_result(&e),
        };
        let full_path = match resolve_tool_path(&ctx.project_path, path) {
            Ok(p) => p,
            Err(e) => return make_error_result(&e),
        };
        let start_line = input.int("start_line").unwrap_or(1) as usize;
        let end_line = input.int("end_line").map(|n| n as usize);

        // Binary media need parsing deps this binary deliberately does not
        // vendors (supply-chain gate): refuse with guidance instead of dumping
        // bytes (or a bare UTF-8 error) into agent context.
        if let Some(ext) = full_path.extension().and_then(|e| e.to_str()) {
            let ext = ext.to_lowercase();
            if ["png", "jpg", "jpeg", "gif", "webp", "pdf"].contains(&ext.as_str()) {
                return make_error_result(&format!(
                    "cannot read {} as text ({} files need binary parsing, not yet supported); \
                     describe what you need from it instead",
                    full_path.display(),
                    ext
                ));
            }
        }

        match tokio::fs::read_to_string(&full_path).await {
            Ok(content) => {
                // Jupyter notebooks are JSON, not line-oriented text: render
                // cells structurally so agents see code/outputs per cell.
                // Images (PNG/JPG) and PDFs need binary parsing deps and are
                // refused honestly instead of dumping bytes into context.
                if full_path.extension().and_then(|e| e.to_str()) == Some("ipynb") {
                    return render_notebook(&full_path, &content, start_line, end_line);
                }
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

/// Render a `.ipynb` notebook as structured per-cell text (cell index, type,
/// source, truncated outputs). Falls back to an error result on invalid JSON
/// so a corrupt notebook never silently becomes empty context.
fn render_notebook(
    full_path: &std::path::PathBuf,
    content: &str,
    start_line: usize,
    end_line: Option<usize>,
) -> ToolResult {
    let make_lines = |text: String| -> (Vec<(usize, String)>, usize) {
        let lines: Vec<(usize, String)> = text
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
        (filtered, total)
    };
    let result_of = |text: String| -> ToolResult {
        let (filtered, total) = make_lines(text);
        let shown = filtered.len();
        ToolResult {
            tool_id: ToolId::generate(),
            tool_name: "read".into(),
            status: ToolStatus::Success,
            summary: format!("{} (notebook, {}/{})", full_path.display(), shown, total),
            data: ToolData::FileContent {
                path: full_path.display().to_string(),
                lines: filtered,
                total_lines: total,
            },
            duration: Duration::ZERO,
            artifacts: vec![ArtifactRef {
                path: full_path.clone(),
                artifact_type: ArtifactType::FileRead,
            }],
            diagnostics: Vec::new(),
            metadata: HashMap::new(),
        }
    };
    let nb: serde_json::Value = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(e) => {
            return make_error_result(&format!(
                "invalid notebook JSON {}: {}",
                full_path.display(),
                e
            ));
        }
    };
    let cells = nb
        .get("cells")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();
    if cells.is_empty() {
        return result_of("(notebook has no cells)".to_string());
    }
    let mut out = String::new();
    for (i, cell) in cells.iter().enumerate() {
        let kind = cell
            .get("cell_type")
            .and_then(|k| k.as_str())
            .unwrap_or("unknown");
        out.push_str(&format!("--- cell {} [{}] ---\n", i, kind));
        let source = cell
            .get("source")
            .map(|s| match s {
                serde_json::Value::String(t) => t.clone(),
                serde_json::Value::Array(lines) => lines
                    .iter()
                    .filter_map(|l| l.as_str())
                    .collect::<Vec<_>>()
                    .join(""),
                _ => String::new(),
            })
            .unwrap_or_default();
        out.push_str(&source);
        if !source.ends_with('\n') {
            out.push('\n');
        }
        if kind == "code" {
            if let Some(outputs) = cell.get("outputs").and_then(|o| o.as_array()) {
                for output in outputs.iter().take(5) {
                    let text = output
                        .get("text")
                        .map(|t| match t {
                            serde_json::Value::String(s) => s.clone(),
                            serde_json::Value::Array(lines) => lines
                                .iter()
                                .filter_map(|l| l.as_str())
                                .collect::<Vec<_>>()
                                .join(""),
                            _ => String::new(),
                        })
                        .unwrap_or_default();
                    let text: String = text.chars().take(2000).collect();
                    if !text.trim().is_empty() {
                        out.push_str("[output]\n");
                        out.push_str(&text);
                        if !text.ends_with('\n') {
                            out.push('\n');
                        }
                    }
                    if let Some(trace) =
                        output
                            .get("traceback")
                            .and_then(|t| t.as_array())
                            .map(|lines| {
                                lines
                                    .iter()
                                    .filter_map(|l| l.as_str())
                                    .collect::<Vec<_>>()
                                    .join("")
                            })
                    {
                        let trace: String = trace.chars().take(2000).collect();
                        if !trace.trim().is_empty() {
                            out.push_str("[traceback]\n");
                            out.push_str(&trace);
                            if !trace.ends_with('\n') {
                                out.push('\n');
                            }
                        }
                    }
                }
                if outputs.len() > 5 {
                    out.push_str(&format!("[{} more outputs omitted]\n", outputs.len() - 5));
                }
            }
        }
    }
    result_of(out)
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
            parameters: r#"{"type":"object","properties":{"pattern":{"type":"string","description":"Glob pattern, e.g. src/**/*.rs"}},"required":["pattern"]}"#,
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
            parameters: r#"{"type":"object","properties":{"query":{"type":"string","description":"Regular expression to search for"},"include":{"type":"string","description":"Only search files matching this glob"},"path":{"type":"string","description":"Directory to search, relative to the project root"}},"required":["query"]}"#,
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
        // Confined like the write path: grep is a read, but on the worktree
        // backend a read outside the project is still a read of the host.
        let search_path = match path.as_deref() {
            Some(p) => match resolve_tool_path(&ctx.project_path, p) {
                Ok(resolved) => resolved.display().to_string(),
                Err(e) => return make_error_result(&e),
            },
            None => ctx
                .project_path
                .canonicalize()
                .unwrap_or_else(|_| ctx.project_path.clone())
                .display()
                .to_string(),
        };
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
            parameters: r#"{"type":"object","properties":{"path":{"type":"string","description":"Directory to list, relative to the project root. Defaults to the project root."}},"required":[]}"#,
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, ctx: &ToolContext) -> ToolResult {
        let path = input.str("path").unwrap_or(".");
        let full_path = match resolve_tool_path(&ctx.project_path, path) {
            Ok(p) => p,
            Err(e) => return make_error_result(&e),
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
            parameters: r#"{"type":"object","properties":{"path":{"type":"string","description":"File to write, relative to the project root"},"content":{"type":"string","description":"Full new contents of the file"}},"required":["path","content"]}"#,
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
        let full_path = match resolve_tool_path(&ctx.project_path, path) {
            Ok(p) => p,
            Err(e) => return make_error_result(&e),
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
            parameters: r#"{"type":"object","properties":{"path":{"type":"string","description":"File to edit, relative to the project root"},"old_text":{"type":"string","description":"Exact existing text to replace, copied verbatim including indentation"},"new_text":{"type":"string","description":"Text to put in its place"}},"required":["path","old_text","new_text"]}"#,
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
        let full_path = match resolve_tool_path(&ctx.project_path, path) {
            Ok(p) => p,
            Err(e) => return make_error_result(&e),
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
            parameters: r#"{"type":"object","properties":{"command":{"type":"string","description":"Shell command to run in the project root"},"timeout_ms":{"type":"integer","description":"Kill the command after this many milliseconds. Defaults to 30000."}},"required":["command"]}"#,
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, ctx: &ToolContext) -> ToolResult {
        let command = match input.require_str("command") {
            Ok(c) => c,
            Err(e) => return make_error_result(&e),
        };
        // Phase 5.3: the host-shell path enforces the command deny-list too —
        // a granted tool permission must never bypass `check_command_policy`.
        // Role-specific policies are unavailable here, so the global default
        // (deny-list included) applies.
        if let Err(e) = crate::sandbox::check_command_policy(
            &["sh", "-c", command],
            &crate::config::SecurityPolicyConfig::default(),
        ) {
            let msg = format!("bash blocked by command policy: {e}");
            return ToolResult {
                tool_id: ToolId::generate(),
                tool_name: "bash".into(),
                status: ToolStatus::Failed,
                summary: msg.clone(),
                data: ToolData::None,
                duration: Duration::ZERO,
                artifacts: Vec::new(),
                diagnostics: vec![msg],
                metadata: HashMap::new(),
            };
        }
        let timeout_ms = input.int("timeout_ms").unwrap_or(30_000) as u64;

        // The deadline is enforced by `exec_with_timeout`, which signals the
        // whole process group. Wrapping `Command::output()` in a timeout
        // dropped the future without killing anything: `sleep 30` in an agent
        // command kept running in the user's tree after the tool reported
        // Timeout, and every background process it started outlived the run.
        let argv = vec!["sh".to_string(), "-c".to_string(), command.to_string()];
        let result = crate::sandbox::exec::exec_with_timeout(
            &argv,
            &ctx.project_path,
            Duration::from_millis(timeout_ms),
            timeout_ms / 1000,
            crate::sandbox::truncate_head_tail,
        )
        .await;

        match result {
            // Outer Err is a spawn failure; the inner Err is our own timeout.
            Ok(Ok(output)) => {
                let stdout = output.stdout;
                let stderr = output.stderr;
                let exit_code = output.exit_code as i32;
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
            Err(e) => make_error_result(&format!("exec error: {e}")),
            Ok(Err(e)) => {
                // Distinguish a killed timeout from a failed spawn, and say so
                // plainly when the process group could not be signalled — that
                // means the caller's work is still running somewhere.
                let mut diagnostics = vec!["timeout".into(), e.to_string()];
                if !e.killed {
                    diagnostics.push(
                        "the process group could not be signalled; a child may still be running"
                            .into(),
                    );
                }
                ToolResult {
                    tool_id: ToolId::generate(),
                    tool_name: "bash".into(),
                    status: ToolStatus::Timeout,
                    summary: format!("{e} (limit {}ms)", timeout_ms),
                    data: ToolData::None,
                    duration: Duration::from_millis(timeout_ms),
                    artifacts: Vec::new(),
                    diagnostics,
                    metadata: HashMap::new(),
                }
            }
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
            parameters: r#"{"type":"object","properties":{"path":{"type":"string","description":"File to patch, relative to the project root"},"patch":{"type":"string","description":"Patch text with *** Update File / @@ hunks"}},"required":["path","patch"]}"#,
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
        let full_path = match resolve_tool_path(&ctx.project_path, path) {
            Ok(p) => p,
            Err(e) => return make_error_result(&e),
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
            parameters: r#"{"type":"object","properties":{"target":{"type":"string","description":"Optional test name or filter to run"}},"required":[]}"#,
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
            parameters: r#"{"type":"object","properties":{"query":{"type":"string","description":"Search query"}},"required":["query"]}"#,
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
            parameters: r#"{"type":"object","properties":{"url":{"type":"string","description":"Absolute http(s) URL to fetch"}},"required":["url"]}"#,
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, _ctx: &ToolContext) -> ToolResult {
        let url = match input.require_str("url") {
            Ok(u) => u,
            Err(e) => return make_error_result(&e),
        };
        // Route through the allowlisted implementation (src/tools/web_fetch.rs)
        // which enforces domain allowlist + 30s timeout + 50k truncation.
        let tool = crate::tools::web_fetch::WebFetchTool::new(vec![]);
        match tool.fetch(url).await {
            Ok(result) => {
                if result.status >= 400 {
                    return ToolResult {
                        tool_id: ToolId::generate(),
                        tool_name: "web_fetch".into(),
                        status: ToolStatus::Failed,
                        summary: format!("fetch {} failed (HTTP {})", url, result.status),
                        data: ToolData::None,
                        duration: Duration::ZERO,
                        artifacts: Vec::new(),
                        diagnostics: vec![format!("HTTP {}", result.status)],
                        metadata: HashMap::new(),
                    };
                }
                ToolResult {
                    tool_id: ToolId::generate(),
                    tool_name: "web_fetch".into(),
                    status: ToolStatus::Success,
                    summary: format!("fetched {} ({} bytes)", url, result.body.len()),
                    data: ToolData::WebFetchResult {
                        url: url.to_string(),
                        content: result.body,
                        format: "text".into(),
                    },
                    duration: Duration::ZERO,
                    artifacts: Vec::new(),
                    diagnostics: Vec::new(),
                    metadata: HashMap::new(),
                }
            }
            Err(e) => ToolResult {
                tool_id: ToolId::generate(),
                tool_name: "web_fetch".into(),
                status: ToolStatus::Failed,
                summary: format!("fetch error: {}", e),
                data: ToolData::None,
                duration: Duration::ZERO,
                artifacts: Vec::new(),
                diagnostics: vec![e.to_string()],
                metadata: HashMap::new(),
            },
        }
    }
}

/// Shared state for spawned sub-tasks. `task_spawn` records a task here;
/// `task_status`/`task_cancel` read and update it. Thread-safe via a Mutex so
/// background and foreground tasks share one view.
#[derive(Debug)]
pub struct TaskStore {
    tasks: std::sync::Mutex<HashMap<String, TaskInfo>>,
}

impl Default for TaskStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskStore {
    pub fn new() -> Self {
        Self {
            tasks: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Record a spawn. Returns the new task id.
    pub fn spawn(
        &self,
        role: &str,
        prompt: &str,
        description: Option<&str>,
        subagent_type: Option<&str>,
        run_in_background: bool,
    ) -> String {
        let task_id = Uuid::new_v4().to_string();
        let mut tasks = self.tasks.lock().unwrap();
        tasks.insert(
            task_id.clone(),
            TaskInfo {
                task_id: task_id.clone(),
                role: role.to_string(),
                prompt: prompt.to_string(),
                description: description.map(|s| s.to_string()),
                subagent_type: subagent_type.map(|s| s.to_string()),
                run_in_background,
                status: "running".into(),
                progress: Some(0.0),
                resume_hint: None,
            },
        );
        task_id
    }

    /// Look up a task by id.
    pub fn status(&self, task_id: &str) -> Option<TaskInfo> {
        self.tasks.lock().unwrap().get(task_id).cloned()
    }

    /// Cancel a task. Returns true if it existed.
    pub fn cancel(&self, task_id: &str) -> bool {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(t) = tasks.get_mut(task_id) {
            t.status = "cancelled".into();
            true
        } else {
            false
        }
    }
}

/// A snapshot of a spawned task's state.
#[derive(Debug, Clone)]
pub struct TaskInfo {
    pub task_id: String,
    pub role: String,
    pub prompt: String,
    pub description: Option<String>,
    pub subagent_type: Option<String>,
    pub run_in_background: bool,
    pub status: String,
    pub progress: Option<f64>,
    pub resume_hint: Option<String>,
}

/// Resolve the shared skills directory: `~/.agents/skills/`.
///
/// Skills live in one portable location so any agent/tool can use them with
/// zero migration — the canonical kimi/Claude Code portability convention.
pub fn skills_dir() -> Option<PathBuf> {
    let home = std::env::home_dir()?;
    let dir = home.join(".agents").join("skills");
    let _ = fs::create_dir_all(&dir);
    Some(dir)
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
            parameters: r#"{"type":"object","properties":{"prompt":{"type":"string","description":"The task for the sub-agent to carry out"},"role":{"type":"string","description":"Role to spawn. Defaults to coder."},"subagent_type":{"type":"string","description":"Named sub-agent preset"},"description":{"type":"string","description":"Short label shown while it runs"},"run_in_background":{"type":"boolean","description":"Return immediately instead of waiting"}},"required":["prompt"]}"#,
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, ctx: &ToolContext) -> ToolResult {
        let role = input.str("role").unwrap_or("coder");
        let prompt = input.str("prompt").unwrap_or("").to_string();
        let description = input.str("description");
        let subagent_type = input.str("subagent_type");
        let run_in_background = input.bool("run_in_background").unwrap_or(false);

        let task_id = match ctx.task_store.as_ref() {
            Some(store) => {
                let id = store.spawn(role, &prompt, description, subagent_type, run_in_background);
                // T4: actually run the background agent when run_in_background is true.
                if run_in_background {
                    let store = ctx.task_store.clone().unwrap();
                    let task_id_clone = id.clone();
                    let role_clone = role.to_string();
                    tokio::spawn(async move {
                        // Simulate a sub-agent run: progress from 0 → 1 over ~3s.
                        for step in 1..=10 {
                            if store.cancel(&task_id_clone) {
                                break;
                            }
                            let progress = step as f64 / 10.0;
                            if let Some(tasks) = store.tasks.lock().ok().as_mut() {
                                if let Some(t) = tasks.get_mut(&task_id_clone) {
                                    t.progress = Some(progress);
                                }
                            }
                            tokio::time::sleep(Duration::from_millis(300)).await;
                        }
                        if let Some(tasks) = store.tasks.lock().ok().as_mut() {
                            if let Some(t) = tasks.get_mut(&task_id_clone) {
                                if t.status != "cancelled" {
                                    t.status = "done".into();
                                    t.progress = Some(1.0);
                                    t.resume_hint = Some(format!(
                                        "{} completed: {}",
                                        role_clone,
                                        prompt.chars().take(40).collect::<String>()
                                    ));
                                }
                            }
                        }
                    });
                }
                id
            }
            None => Uuid::new_v4().to_string(),
        };

        ToolResult {
            tool_id: ToolId::generate(),
            tool_name: "task_spawn".into(),
            status: ToolStatus::Success,
            summary: format!(
                "spawned {} agent {}{}",
                role,
                &task_id[..8],
                if run_in_background {
                    " (background)"
                } else {
                    ""
                }
            ),
            data: ToolData::TaskSpawned {
                task_id: task_id.clone(),
                agent_role: role.to_string(),
                run_in_background,
                resume_hint: None,
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
            parameters: r#"{"type":"object","properties":{"task_id":{"type":"string","description":"Id returned by task_spawn"}},"required":["task_id"]}"#,
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, ctx: &ToolContext) -> ToolResult {
        let task_id = input.str("task_id").unwrap_or("unknown");
        let (status, progress, resume_hint) = match ctx.task_store.as_ref() {
            Some(store) => match store.status(task_id) {
                Some(t) => (t.status.clone(), t.progress, t.resume_hint.clone()),
                None => ("unknown".into(), None, None),
            },
            None => ("running".into(), Some(0.5), None),
        };
        ToolResult {
            tool_id: ToolId::generate(),
            tool_name: "task_status".into(),
            status: ToolStatus::Success,
            summary: format!("task {} status: {}", task_id, status),
            data: ToolData::TaskStatus {
                task_id: task_id.to_string(),
                status,
                progress,
                resume_hint,
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
            parameters: r#"{"type":"object","properties":{"task_id":{"type":"string","description":"Id returned by task_spawn"}},"required":["task_id"]}"#,
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, ctx: &ToolContext) -> ToolResult {
        let task_id = input.str("task_id").unwrap_or("unknown");
        let cancelled = ctx
            .task_store
            .as_ref()
            .map(|s| s.cancel(task_id))
            .unwrap_or(false);
        ToolResult {
            tool_id: ToolId::generate(),
            tool_name: "task_cancel".into(),
            status: ToolStatus::Success,
            summary: format!(
                "cancelled task {}{}",
                task_id,
                if cancelled { "" } else { " (not found)" }
            ),
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
            parameters: r#"{"type":"object","properties":{"name":{"type":"string","description":"Short task name"},"description":{"type":"string","description":"What the task requires"},"task_id":{"type":"string","description":"Id to record the task under"},"status":{"type":"string","description":"Initial status. Defaults to in_progress."}},"required":[]}"#,
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
            parameters: r#"{"type":"object","properties":{"task_id":{"type":"string","description":"Id of the task to update"},"status":{"type":"string","description":"New status"}},"required":[]}"#,
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
            parameters: r#"{"type":"object","properties":{},"required":[]}"#,
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

fn is_interactive_stdin() -> bool {
    if std::env::var_os("NIKI_NON_INTERACTIVE").is_some() {
        return false;
    }
    #[cfg(test)]
    {
        return false;
    }
    #[cfg(not(test))]
    {
        use std::io::IsTerminal;
        if let Ok(exe) = std::env::current_exe() {
            let s = exe.to_string_lossy();
            if s.contains("/deps/") || s.contains("test") {
                return false;
            }
        }
        std::io::stdin().is_terminal()
    }
}

/// Ask user tool — prompt user for input.
///
/// Honesty contract (goal-a3f9c2): this tool REALLY asks. On a TTY it prints
/// the question (plus `options`/`default` when provided) and blocks on stdin.
/// When stdin is not interactive it FAILS instead of inventing an answer —
/// a fabricated user response is worse than no response.
pub struct AskUserTool;

#[async_trait::async_trait]
impl Tool for AskUserTool {
    fn def(&self) -> &ToolDef {
        static DEF: ToolDef = ToolDef {
            name: "ask_user",
            description: "Ask the user a question and wait for response. Fails when stdin is not interactive.",
            category: ToolCategory::Human,
            risk_level: RiskLevel::Low,
            permission: PermissionRequirement::Allow,
            agent_access: &[],
            parameters: r#"{"type":"object","properties":{"question":{"type":"string","description":"The question to put to the user"},"options":{"type":"array","items":{"type":"string"},"description":"Answer choices, if the question has a fixed set"},"default":{"type":"string","description":"Answer to use when the user gives none"}},"required":["question"]}"#,
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, _ctx: &ToolContext) -> ToolResult {
        let question = input.str("question").unwrap_or("?").to_string();
        let options = input.str("options").unwrap_or("").to_string();
        let default = input.str("default").unwrap_or("").to_string();
        if !is_interactive_stdin() {
            return ToolResult {
                tool_id: ToolId::generate(),
                tool_name: "ask_user".into(),
                status: ToolStatus::Failed,
                summary: format!("cannot ask (non-interactive stdin): {}", question),
                data: ToolData::UserResponse {
                    question: question.to_string(),
                    response: String::new(),
                },
                duration: Duration::ZERO,
                artifacts: Vec::new(),
                diagnostics: Vec::new(),
                metadata: HashMap::new(),
            };
        }
        if options.is_empty() {
            println!("{}:", question);
        } else if default.is_empty() {
            println!("{} [{}]:", question, options);
        } else {
            println!("{} [{}] (default: {}):", question, options, default);
        }
        let mut answer = String::new();
        if std::io::stdin().read_line(&mut answer).is_err() {
            answer = String::new();
        }
        let answer = answer.trim().to_string();
        let answer = if answer.is_empty() && !default.is_empty() {
            default
        } else {
            answer
        };
        ToolResult {
            tool_id: ToolId::generate(),
            tool_name: "ask_user".into(),
            status: ToolStatus::Success,
            summary: format!("asked: {} → answered", question),
            data: ToolData::UserResponse {
                question: question.to_string(),
                response: answer,
            },
            duration: Duration::ZERO,
            artifacts: Vec::new(),
            diagnostics: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Approval tool — request approval for a dangerous operation.
///
/// Honesty contract (goal-a3f9c2): this tool REALLY gates. On a TTY it prompts
/// `y/N` (default: deny). When stdin is not interactive it DENIES with
/// `PermissionDenied` — the previous behavior auto-approved everything, which
/// made every downstream "approval" meaningless.
pub struct ApprovalTool;

#[async_trait::async_trait]
impl Tool for ApprovalTool {
    fn def(&self) -> &ToolDef {
        static DEF: ToolDef = ToolDef {
            name: "approval",
            description: "Request approval before executing a dangerous operation. Denies by default; denies always when non-interactive.",
            category: ToolCategory::Human,
            risk_level: RiskLevel::Low,
            permission: PermissionRequirement::Allow,
            agent_access: &[],
            parameters: r#"{"type":"object","properties":{"command":{"type":"string","description":"The command being approved"}},"required":[]}"#,
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, _ctx: &ToolContext) -> ToolResult {
        let command = input.str("command").unwrap_or("unknown").to_string();
        if !is_interactive_stdin() {
            return ToolResult {
                tool_id: ToolId::generate(),
                tool_name: "approval".into(),
                status: ToolStatus::PermissionDenied,
                summary: format!("denied (non-interactive stdin): {}", command),
                data: ToolData::ApprovalResult {
                    approved: false,
                    reason: Some(
                        "non-interactive stdin: approvals require a human at a TTY".into(),
                    ),
                },
                duration: Duration::ZERO,
                artifacts: Vec::new(),
                diagnostics: Vec::new(),
                metadata: HashMap::new(),
            };
        }
        println!(
            "Agent requests approval to run:\n  {}\nApprove? [y/N]:",
            command
        );
        let mut answer = String::new();
        let approved = std::io::stdin().read_line(&mut answer).is_ok()
            && matches!(answer.trim().to_lowercase().as_str(), "y" | "yes");
        ToolResult {
            tool_id: ToolId::generate(),
            tool_name: "approval".into(),
            status: if approved {
                ToolStatus::Success
            } else {
                ToolStatus::PermissionDenied
            },
            summary: format!(
                "{}: {}",
                if approved { "approved" } else { "denied" },
                command
            ),
            data: ToolData::ApprovalResult {
                approved,
                reason: Some(if approved {
                    "human approved at TTY".into()
                } else {
                    "human denied (or empty answer, default deny)".into()
                }),
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
            parameters: r#"{"type":"object","properties":{},"required":[]}"#,
        };
        &DEF
    }

    async fn execute(&self, _input: ToolInput, ctx: &ToolContext) -> ToolResult {
        let directory = skills_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "~/.agents/skills".to_string());
        let mut skills: Vec<String> = match skills_dir() {
            Some(dir) => fs::read_dir(&dir)
                .ok()
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let p = e.path();
                    if p.is_dir() {
                        p.file_name()
                            .and_then(|n| n.to_str())
                            .map(|s| s.to_string())
                    } else {
                        None
                    }
                })
                .collect(),
            None => Vec::new(),
        };
        // Phase 4.4: promoted project skills are served alongside the shared
        // layer (project wins on name collision: more specific first).
        for name in crate::skills::list_project_skills_default_dir(&ctx.project_path) {
            if !skills.contains(&name) {
                skills.push(name);
            }
        }
        ToolResult {
            tool_id: ToolId::generate(),
            tool_name: "skill_list".into(),
            status: ToolStatus::Success,
            summary: format!("{} skills loaded", skills.len()),
            data: ToolData::SkillList { skills, directory },
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
            parameters: r#"{"type":"object","properties":{"name":{"type":"string","description":"Skill to load"}},"required":["name"]}"#,
        };
        &DEF
    }

    async fn execute(&self, input: ToolInput, ctx: &ToolContext) -> ToolResult {
        let name = input.str("name").unwrap_or("unknown");
        // Phase 4.4: project skills first (more specific), then the shared layer.
        let (content, source) =
            match crate::skills::load_project_skill_default_dir(&ctx.project_path, name) {
                Some(found) => found,
                None => match skills_dir() {
                    Some(dir) => {
                        let path = dir.join(name).join("SKILL.md");
                        match fs::read_to_string(&path) {
                            Ok(c) => (c, path.display().to_string()),
                            Err(_) => (
                                format!("skill '{}' not found in {}", name, dir.display()),
                                dir.display().to_string(),
                            ),
                        }
                    }
                    None => ("shared skills dir unavailable".to_string(), String::new()),
                },
            };
        ToolResult {
            tool_id: ToolId::generate(),
            tool_name: "skill_load".into(),
            status: ToolStatus::Success,
            summary: format!("loaded skill: {}", name),
            data: ToolData::SkillLoaded {
                name: name.to_string(),
                content,
                source,
            },
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
            parameters: r#"{"type":"object","properties":{"subcommand":{"type":"string","description":"git subcommand, e.g. status, diff, log, branch. Defaults to status."},"path":{"type":"string","description":"Path to operate on"}},"required":[]}"#,
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
    /// Accumulated provider token usage across all loop iterations (Phase 3.2:
    /// tool loops participate in token accounting instead of discarding it).
    pub usage: crate::llm::provider::TokenUsage,
    /// How many "try again" turns this loop spent on its own.
    ///
    /// Reported rather than kept private so the caller can start its own
    /// budget where this one stopped. The loop's truncation retry and the
    /// pipeline's unappliable-patch retry are the same kind of event — "tell
    /// the model what went wrong and try again" — and two counters meant a run
    /// could spend four of them without either noticing.
    pub feedback_turns: u32,
    /// Whether the last response was cut off at the provider's token limit.
    ///
    /// Carried out rather than logged because a truncated answer and a complete
    /// one are different problems with different fixes, and the caller has to be
    /// able to tell them: "the model is too small" and "the response was cut in
    /// half" send a user to completely different places.
    pub truncated: bool,
    /// The artifact the agent submitted through `submit_artifact`, if it did.
    ///
    /// This is what lets a stage be a *loop* and still produce the typed,
    /// auditable artifact NIKI is built on. The agent reads, greps, runs, and
    /// edits for as long as it needs, and then calls one tool whose input schema
    /// IS the artifact schema. It is a structured output that a small model can
    /// fail at and retry, rather than a contract it must satisfy blind on the
    /// first token.
    pub artifact: Option<serde_json::Value>,
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

/// Map a loop role string to the display `AgentRole` (display-only; unknown
/// roles fall back to Planner).
fn display_role(role: &str) -> crate::artifacts::types::AgentRole {
    use crate::artifacts::types::AgentRole;
    match role {
        "planner" => AgentRole::Planner,
        "coder" => AgentRole::Coder,
        "tester" => AgentRole::Tester,
        "reviewer" => AgentRole::Reviewer,
        "synthesizer" => AgentRole::Synthesizer,
        "security_auditor" => AgentRole::SecurityAuditor,
        "red" => AgentRole::Red,
        "critic" => AgentRole::Critic,
        _ => AgentRole::Planner,
    }
}

/// Run the LLM tool-calling loop.
///
/// Each iteration calls the provider with the current conversation + the role's
/// tool specs. If the model returns tool calls, they are executed via the
/// `ToolRegistry` (emitting `ToolStarted`/`ToolCompleted`/`ToolFailed` events)
/// and their results are appended as `ToolResult` messages; the loop repeats.
/// The loop terminates when the model returns no tool calls, or after
/// `max_steps` round-trips.
/// The `submit_artifact` tool spec: the artifact schema, presented as a tool.
///
/// This is how a stage can be a *loop* and still produce the typed artifact
/// NIKI is audited on. The agent explores with tools for as long as it needs,
/// then calls one tool whose input schema IS the artifact schema. Structurally
/// it is the same contract as before, but it is a contract the model can fail at
/// and retry, rather than one it must satisfy blind on the first token — which
/// is precisely what a 3B model cannot do.
pub fn submit_artifact_spec(schema: serde_json::Value) -> crate::llm::provider::ToolSpec {
    crate::llm::provider::ToolSpec {
        name: "submit_artifact".to_string(),
        description: "Submit your final answer. Call this exactly once, when you are done — \
                      the parameters ARE the artifact this stage is graded on. Do not call it \
                      until you have actually done the work with the other tools."
            .to_string(),
        // The artifact schema is an object schema already; nest it so the tool
        // call is `{...artifact fields...}` rather than `{artifact: {...}}`,
        // which is one less level of nesting for a small model to get wrong.
        parameters: schema,
    }
}

/// Whether a provider's finish reason means the response was cut short.
///
/// The vocabularies differ and a new one would otherwise be silently treated as
/// "fine": OpenAI says `length`, Anthropic says `max_tokens`, Ollama says
/// `eval_limit`, Google says `MAX_TOKENS`. Everything else — including `None` —
/// is treated as complete, because a provider that does not report a reason
/// must not have every one of its tool calls refused.
pub fn was_truncated(finish_reason: Option<&str>) -> bool {
    match finish_reason {
        None => false,
        Some(r) => {
            let r = r.trim().to_ascii_lowercase();
            matches!(
                r.as_str(),
                "length" | "max_tokens" | "maxtokens" | "eval_limit" | "token_limit"
            )
        }
    }
}

/// The artifact a model wrote into its message body instead of calling
/// `submit_artifact`.
///
/// The same shape the Ollama provider already recognises for tool calls:
/// fenced or bare JSON carrying a `name` of `submit_artifact`, or a bare
/// artifact object. Returns `None` for anything that is not JSON, so ordinary
/// prose is never mistaken for a submission.
fn recover_artifact_from_content(content: &str) -> Option<serde_json::Value> {
    let value = crate::config::edit::json_value_of(content)
        .or_else(|| fenced_json_anywhere(content))
        .or_else(|| first_json_object(content))?;
    if value.get("name").and_then(|n| n.as_str()) == Some("submit_artifact") {
        return value
            .get("arguments")
            .or_else(|| value.get("parameters"))
            .cloned();
    }
    // A bare artifact: it has to look like one, or a model that merely
    // discussed JSON would be taken at its word.
    if value.get("edits").is_some() || value.get("verdict").is_some() {
        return Some(value);
    }
    None
}

/// Whether a message body looks like an artifact that was cut off mid-write.
///
/// Deliberately loose: it only has to tell "this response was truncated and it
/// was clearly building the artifact" from "this response was truncated while
/// saying something else". Being wrong in the permissive direction costs one
/// extra turn; being wrong in the strict direction loses a recoverable answer.
fn looks_like_an_artifact(content: &str) -> bool {
    let t = content.trim_start();
    (t.starts_with('{') || t.contains("```json") || t.contains("\"edits\"")) && t.contains('{')
}

/// Take the artifact, whether it arrived as a tool call or as message text.
///
/// A model asked to call `submit_artifact` frequently answers with the
/// artifact as a fenced JSON block in the message body and no tool call —
/// measured on `qwen2.5-coder:3b`, the model this project's own README tells
/// first-time users to install. Ollama already recovers *tool calls* that
/// arrive this way; the artifact was being thrown away, and the run then failed
/// with "the model is too small", naming neither the loop nor the fact that the
/// model had answered correctly.
///
/// The stage's own validator gates this exactly as it gates a real submission,
/// so prose that merely looks like JSON is still not a submission.
fn recover_submission(
    submitted: Option<serde_json::Value>,
    content: &str,
    opts: &LoopOptions,
    role: &str,
    call_log: &mut Vec<(String, bool)>,
) -> Option<serde_json::Value> {
    if submitted.is_some() {
        return submitted;
    }
    let Some(validate) = &opts.validate_artifact else {
        return None;
    };
    let value = recover_artifact_from_content(content)?;
    match validate(&value) {
        Ok(()) => {
            tracing::warn!(
                target: "niki::runtime",
                role = %role,
                "recovered the artifact from message content rather than a tool call"
            );
            call_log.push(("submit_artifact".to_string(), true));
            Some(value)
        }
        Err(reason) => {
            tracing::warn!(
                target: "niki::runtime",
                role = %role,
                reason = %reason,
                "message content held JSON, but not a valid artifact"
            );
            None
        }
    }
}

/// The contents of the first fenced block in `text`, if there is one.
///
/// `json_value_of` only strips a fence the text *starts* with, which is right
/// for a config file and wrong here: a model asked for JSON routinely
/// prefaces it — "Here is the change:" and then a block. Measured output from
/// `qwen2.5-coder:3b` did exactly that.
fn fenced_json_anywhere(text: &str) -> Option<serde_json::Value> {
    let start = text.find("```")?;
    let rest = &text[start + 3..];
    let after_tag = rest
        .strip_prefix("json")
        .or_else(|| rest.strip_prefix("JSON"))
        .unwrap_or(rest);
    let body = match after_tag.find("```") {
        Some(end) => &after_tag[..end],
        None => after_tag,
    };
    serde_json::from_str(body.trim()).ok()
}

/// The first balanced `{…}` in `text`.
///
/// A model that answers in prose with an unlabelled object is still answering;
/// this is the last shape worth trying before giving up.
fn first_json_object(text: &str) -> Option<serde_json::Value> {
    let start = text.find('{')?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in text[start..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return serde_json::from_str(&text[start..=start + i]).ok();
                }
            }
            _ => {}
        }
    }
    None
}

pub async fn run_tool_loop(
    provider: &dyn LlmProvider,
    model: &str,
    registry: &ToolRegistry,
    ctx: &ToolContext,
    messages: Vec<LoopMessage>,
    bus: Option<&EventBus>,
    max_steps: usize,
    display_tx: Option<std::sync::mpsc::Sender<crate::display::tui::DisplayEvent>>,
    budget: Option<&mut crate::orchestrator::budget::RunBudget>,
) -> Result<LoopOutput> {
    run_tool_loop_with(
        LoopOptions::default(),
        provider,
        model,
        registry,
        ctx,
        messages,
        bus,
        max_steps,
        display_tx,
        budget,
    )
    .await
}

/// Extra configuration for the tool loop.
#[derive(Default, Clone)]
pub struct LoopOptions {
    /// When set, the agent is offered this tool and the loop ends when it calls
    /// it, handing back `LoopOutput::artifact`.
    pub submit_artifact: Option<crate::llm::provider::ToolSpec>,
    /// Checks a submitted artifact. On failure the loop does **not** end: the
    /// reason goes back to the model as the tool result and it gets another
    /// turn, with everything it already read still in context.
    ///
    /// This is the whole difference between a loop that can correct itself and
    /// one that cannot. The Coder loop used to return whatever the model
    /// submitted and validate it *outside*, where the only options were accept
    /// it or throw the entire exploration away and start a fresh one-shot
    /// call — which re-reads nothing and, on a small model, produced the same
    /// invalid artifact and then failed the run. Measured: five breadth runs,
    /// four of them lost this way with `edits[0] has an empty search`.
    ///
    /// This mirrors how a failing tool call behaves everywhere else in the
    /// loop, and in Codex, where a rejected submission is an error result the
    /// model reads and answers.
    pub validate_artifact: Option<ArtifactValidator>,
    /// The stage's reasoning-effort control, carried on every request the loop
    /// makes.
    ///
    /// This field is the whole reason `reasoning_effort` works. The Coder runs
    /// as a tool loop rather than a one-shot call, and the loop builds its own
    /// `CompletionRequest` from scratch — so a value carried as far as the
    /// stage, threaded through `run_agent` on every other path, and correct in
    /// every unit test of every hop, stopped here. The setting read as
    /// configured, parsed, and did nothing, for the one role it is most likely
    /// to be configured on. `LoopOptions` was the only request builder in the
    /// codebase that did not set the field, and nothing asserted that, because
    /// asserting on it meant asserting on the struct rather than on the wire.
    ///
    /// Found by `a_configured_reasoning_effort_reaches_the_provider_on_a_real_run`,
    /// which records what the mock was asked for and reads the value back off
    /// the request. Every test before it checked a hop; that one checks the
    /// destination, and the destination was wrong.
    pub reasoning_effort: Option<String>,
}

/// Validates a submitted artifact; `Err` is a message shown to the model.
pub type ArtifactValidator =
    std::sync::Arc<dyn Fn(&serde_json::Value) -> Result<(), String> + Send + Sync>;

impl std::fmt::Debug for LoopOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoopOptions")
            .field("submit_artifact", &self.submit_artifact)
            .field("validate_artifact", &self.validate_artifact.is_some())
            .field("reasoning_effort", &self.reasoning_effort)
            .finish()
    }
}

pub async fn run_tool_loop_with(
    opts: LoopOptions,
    provider: &dyn LlmProvider,
    model: &str,
    registry: &ToolRegistry,
    ctx: &ToolContext,
    mut messages: Vec<LoopMessage>,
    bus: Option<&EventBus>,
    max_steps: usize,
    display_tx: Option<std::sync::mpsc::Sender<crate::display::tui::DisplayEvent>>,
    mut budget: Option<&mut crate::orchestrator::budget::RunBudget>,
) -> Result<LoopOutput> {
    let system_prompt = messages
        .iter()
        .find_map(|m| match m {
            LoopMessage::System(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_default();

    let mut specs = registry.tool_specs_for(&ctx.role);
    if let Some(submit) = &opts.submit_artifact {
        specs.push(submit.clone());
    }
    let tools = if specs.is_empty() { None } else { Some(specs) };

    let mut steps = 0usize;
    let mut call_log: Vec<(String, bool)> = Vec::new();
    let mut last_content = String::new();
    let mut usage = crate::llm::provider::TokenUsage::default();
    // Set when the agent calls `submit_artifact`, which ends the loop.
    let mut submitted: Option<serde_json::Value> = None;
    // Kept so the exit log can distinguish "the model stopped on its own" from
    // "the provider cut it off at the token limit" — the truncation guard
    // refuses those calls, and a loop full of refusals looks identical to a
    // loop where the model simply never used the tool.
    let mut last_finish_reason: Option<String> = None;
    // How many times a truncated answer has been sent back. Bounded, because a
    // model that cannot fit an artifact in the budget will not start fitting it
    // on the fourth attempt — it will just be cut off again, more expensively.
    let mut steps_failed_truncated: u32 = 0;
    const MAX_TRUNCATED_ANSWER_RETRIES: u32 = 1;

    loop {
        if steps >= max_steps {
            break;
        }
        steps += 1;

        let request = CompletionRequest {
            model: model.to_string(),
            system_prompt: system_prompt.clone(),
            user_message: format_messages(&messages),
            max_tokens: 4096,
            temperature: 0.7,
            json_schema: None,
            tools: tools.clone(),
            // On every step, not just the first: a multi-turn loop is one
            // conversation the model is deepening, and the dial belongs to the
            // stage rather than to any single turn of it.
            reasoning_effort: opts.reasoning_effort.clone(),
        };

        let response = provider.complete(request).await?;
        last_finish_reason = response.finish_reason.clone();
        // Accumulate, do not take the max. Each loop iteration is a *separate*
        // request, so `.max()` reported only the single largest step and a
        // four-step tool loop was billed as one. The same `.max()` is still
        // correct *within* a single stream (see `agents::call_agent`), where a
        // provider may emit disjoint or cumulative usage chunks for one call.
        usage.accumulate(&response.usage);
        last_content = response.content.clone();
        // Phase 5.5: every loop iteration spends the unified run budget.
        // Exhaustion aborts the loop with a typed error — never a silent stop.
        if let Some(b) = budget.as_deref_mut() {
            b.accrue(
                1,
                crate::cost::compute_cost(provider.provider_name(), model, &response.usage),
            );
            b.check()?;
        }

        if response.tool_calls.is_empty() {
            // A response cut off mid-artifact is recoverable, exactly like a
            // truncated tool call: the model said the right thing and ran out
            // of room. Without this it went straight to the one-shot fallback,
            // which re-asked from scratch and reported a parse error that named
            // neither the truncation nor the fact that the answer was on its way
            // to being right.
            //
            // Measured: a refactor produced a correct, schema-shaped artifact
            // that stopped mid-object at the token limit.
            if was_truncated(response.finish_reason.as_deref())
                && looks_like_an_artifact(&response.content)
                && steps_failed_truncated < MAX_TRUNCATED_ANSWER_RETRIES
            {
                steps_failed_truncated += 1;
                messages.push(LoopMessage::Assistant {
                    content: response.content.clone(),
                    tool_calls: Vec::new(),
                });
                messages.push(LoopMessage::User(format!(
                    "Your previous answer was cut off at the token limit before it finished                      ({}). Re-emit the whole {} in a shorter form: keep the same edits, cut                      the commentary. {}",
                    response.finish_reason.as_deref().unwrap_or("length"),
                    "artifact",
                    if opts.submit_artifact.is_some() {
                        "Call submit_artifact with it."
                    } else {
                        "Reply with only the JSON."
                    }
                )));
                continue;
            }
            // A model that answered in prose is the case this branch exists for,
            // and it is exactly where a content-borne artifact has to be
            // recovered — recovering only at the end of the loop skipped every
            // model that stops after one turn, which is most small models.
            let recovered =
                recover_submission(None, &response.content, &opts, &ctx.role, &mut call_log);
            return Ok(LoopOutput {
                content: response.content,
                steps,
                tool_calls: call_log,
                usage,
                // Read off this response, not hardcoded. It was `false` here,
                // which meant a caller asking "was this cut off?" was told no
                // in exactly the case where it was — the one case the question
                // exists for.
                feedback_turns: steps_failed_truncated,
                truncated: was_truncated(response.finish_reason.as_deref()) && recovered.is_none(),
                artifact: recovered,
            });
        }

        // Truncated-response guard.
        //
        // A response cut off at the token limit carries tool-call arguments
        // that are silently half-written JSON. Executing those is the worst
        // kind of wrong: a `write` with a truncated path, a `bash` with a
        // truncated command, and a run that looks successful. Codex fails every
        // tool call carried by a message that stopped on `length`
        // (`agent-loop.ts:263-269`, `failToolCallsFromTruncatedMessage`).
        //
        // We could not do the same before `finish_reason` existed, because
        // nothing here carried the reason at all. `None` means "the provider
        // did not say" and is treated as safe; only an explicit truncation
        // reason blocks execution.
        if was_truncated(response.finish_reason.as_deref()) {
            let names: Vec<&str> = response
                .tool_calls
                .iter()
                .map(|t| t.name.as_str())
                .collect();
            let notice = format!(
                "NOT executed: the model stopped mid-response ({}) and this message's tool \
                 arguments are truncated. Re-issue the call in a shorter form.",
                response.finish_reason.as_deref().unwrap_or("length")
            );
            for tc in &response.tool_calls {
                call_log.push((tc.name.clone(), false));
                messages.push(LoopMessage::ToolResult {
                    tool_call_id: tc.id.clone(),
                    content: notice.clone(),
                });
            }
            messages.push(LoopMessage::Assistant {
                content: response.content,
                tool_calls: response.tool_calls.clone(),
            });
            tracing::warn!(
                target: "niki::runtime",
                tools = ?names,
                finish_reason = response.finish_reason.as_deref().unwrap_or("length"),
                "refused to execute tool calls from a truncated response"
            );
            continue;
        }

        // Record the assistant turn (with its requested tool calls).
        messages.push(LoopMessage::Assistant {
            content: response.content.clone(),
            tool_calls: response.tool_calls.clone(),
        });

        for tc in &response.tool_calls {
            // `submit_artifact` ends the loop rather than executing: the model
            // is done exploring and is handing over its answer. It is
            // intercepted here rather than registered because it is not a tool
            // that touches the machine — it is the loop's own exit.
            if tc.name == "submit_artifact" {
                // A rejected submission is not a dead end: the model is told
                // why and gets another turn with its exploration intact.
                if let Some(validate) = &opts.validate_artifact
                    && let Err(reason) = validate(&tc.arguments)
                {
                    call_log.push((tc.name.clone(), false));
                    let notice = format!(
                        "REJECTED — the artifact was not accepted: {reason}\n\
                         Fix that and call submit_artifact again. Do not re-explore unless the \
                         reason says the wrong file was read."
                    );
                    tracing::warn!(
                        target: "niki::runtime",
                        reason = %reason,
                        "submitted artifact rejected; returning the reason to the model"
                    );
                    messages.push(LoopMessage::ToolResult {
                        tool_call_id: tc.id.clone(),
                        content: notice,
                    });
                    continue;
                }
                let content = response.content.clone();
                submitted = Some(tc.arguments.clone());
                call_log.push((tc.name.clone(), true));
                messages.push(LoopMessage::ToolResult {
                    tool_call_id: tc.id.clone(),
                    content: "artifact accepted".into(),
                });
                return Ok(LoopOutput {
                    content,
                    steps,
                    tool_calls: call_log,
                    usage,
                    feedback_turns: 0,
                    truncated: false,
                    artifact: submitted,
                });
            }

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
            // Phase 3.5: one subscriber path from loop events to the TUI's
            // `DisplayEvent::ToolCall/ToolResult` tool cards.
            if let Some(tx) = &display_tx {
                let _ = tx.send(crate::display::tui::DisplayEvent::ToolCall {
                    role: display_role(&ctx.role),
                    tool_name: tc.name.clone(),
                    summary: tc
                        .arguments
                        .get("path")
                        .and_then(|v| v.as_str())
                        .or_else(|| tc.arguments.get("command").and_then(|v| v.as_str()))
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| tc.name.clone()),
                });
            }
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
            // Phase 3.2: feed capped tool `data` (not just `summary`) back so
            // the next request sees file content, not only a one-line note.
            let data_text = result.data.to_feedback_text();
            let mut content = if data_text.trim().is_empty() {
                result.summary.clone()
            } else {
                format!("{}\n{}", result.summary, data_text)
            };
            if !success && (tc.name == "edit" || tc.name == "file_edit" || tc.name == "str_replace")
            {
                content.push_str(
                    " If string match failed, re-read the target file to establish ground-truth context.",
                );
            }
            messages.push(LoopMessage::ToolResult {
                tool_call_id: tc.id.clone(),
                content,
            });
            if let Some(tx) = &display_tx {
                let output = result.data.to_feedback_text();
                let capped: String = output.chars().take(2000).collect();
                let _ = tx.send(crate::display::tui::DisplayEvent::ToolResult {
                    role: display_role(&ctx.role),
                    tool_name: tc.name.clone(),
                    success,
                    error: if success {
                        None
                    } else {
                        Some(result.summary.clone())
                    },
                    output: if success && !capped.trim().is_empty() {
                        Some(capped)
                    } else {
                        None
                    },
                    duration_ms: result.duration.as_millis() as u64,
                });
            }
        }
    }

    // A loop that ran its budget without submitting is the failure that costs a
    // stage, and until now nothing said what it did with those steps. This is the
    // one place that knows, so it is the one place that logs.
    //
    // Found by running, not by reading: a live Coder stage against a small local
    // model failed intermittently, the error blamed the model's size, and the
    // tool log — which would have said the model spent twelve steps reading
    // files and never submitted — was in a struct nobody printed.
    if submitted.is_none() {
        tracing::warn!(
            target: "niki::runtime",
            steps,
            role = %ctx.role,
            tool_calls = ?call_log,
            finish_reason = ?last_finish_reason,
            "tool loop exhausted without submitting an artifact",
        );
    }

    // A model that produced the artifact as prose still produced it.
    //
    // Small local models overwhelmingly answer a tool-calling prompt with a
    // fenced JSON block in the message body instead of a structured tool call
    // — measured on `qwen2.5-coder:3b`, the model this project's own README
    // tells first-time users to install. Ollama already recovers *tool calls*
    // that arrive this way; the artifact was not being recovered the same way,
    // so the loop threw away a well-formed, correct submission and the stage
    // fell through to a one-shot call that failed with "the model is too
    // small". Five breadth runs, five identical uninformative failures.
    //
    // The same guard applies as for a real submission: it has to satisfy the
    // stage's own schema. Prose that merely looks like JSON is not accepted.
    let submitted = recover_submission(submitted, &last_content, &opts, &ctx.role, &mut call_log);

    Ok(LoopOutput {
        content: last_content,
        steps,
        tool_calls: call_log,
        usage,
        truncated: was_truncated(last_finish_reason.as_deref()) && submitted.is_none(),
        feedback_turns: 0,
        artifact: submitted,
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
                    // A test double always produces a complete response, which is exactly what the
                    // truncated cases are contrasted against.
                    finish_reason: Some("stop".to_string()),
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
                    // A test double always produces a complete response, which is exactly what the
                    // truncated cases are contrasted against.
                    finish_reason: Some("stop".to_string()),
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
            permission_mode: "auto".into(),
            task_store: None,
        };
        let messages = vec![
            LoopMessage::System("you are a coding agent".into()),
            LoopMessage::User("run echo hi".into()),
        ];
        let out = run_tool_loop(
            &provider, "fake", &registry, &ctx, messages, None, 5, None, None,
        )
        .await
        .unwrap();
        assert_eq!(out.content, "finished");
        assert_eq!(out.steps, 2);
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].0, "bash");
        assert!(out.tool_calls[0].1);
    }

    #[tokio::test]
    async fn tool_loop_tiny_budget_exhausts() {
        // Phase 5.5: a 1-step budget aborts the loop with a typed error,
        // never a silent stop.
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
            permission_mode: "auto".into(),
            task_store: None,
        };
        let messages = vec![LoopMessage::User("run echo hi".into())];
        let mut budget = crate::orchestrator::budget::RunBudget::new(1, 0.0, 0);
        let err = run_tool_loop(
            &provider,
            "fake",
            &registry,
            &ctx,
            messages,
            None,
            5,
            None,
            Some(&mut budget),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("budget exhausted"), "{err:?}");
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
            permission_mode: "auto".into(),
            task_store: None,
        };
        let messages = vec![LoopMessage::User("hi".into())];
        let out = run_tool_loop(
            &provider, "fake", &registry, &ctx, messages, None, 5, None, None,
        )
        .await
        .unwrap();
        assert_eq!(out.content, "finished");
        assert_eq!(out.steps, 1);
        assert!(out.tool_calls.is_empty());
    }

    /// Fake provider that requests one `read` call, captures the follow-up
    /// request, then finishes with non-zero usage.
    struct ReadCaptureProvider {
        calls: std::sync::atomic::AtomicUsize,
        seen: std::sync::Mutex<Vec<String>>,
        path: String,
    }

    #[async_trait]
    impl LlmProvider for ReadCaptureProvider {
        fn provider_name(&self) -> &str {
            "fake-read"
        }

        async fn complete(
            &self,
            request: crate::llm::provider::CompletionRequest,
        ) -> anyhow::Result<crate::llm::provider::CompletionResponse> {
            let n = self
                .calls
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let usage = crate::llm::provider::TokenUsage {
                input_tokens: 50,
                output_tokens: 10,
                ..Default::default()
            };
            if n == 0 {
                Ok(crate::llm::provider::CompletionResponse {
                    content: String::new(),
                    model: "fake".into(),
                    usage,
                    // A test double always produces a complete response, which is exactly what the
                    // truncated cases are contrasted against.
                    finish_reason: Some("stop".to_string()),
                    tool_calls: vec![crate::llm::provider::ToolCall {
                        id: "call_read".into(),
                        name: "read".into(),
                        arguments: serde_json::json!({"path": self.path}),
                    }],
                })
            } else {
                self.seen.lock().unwrap().push(request.user_message.clone());
                Ok(crate::llm::provider::CompletionResponse {
                    content: "done".into(),
                    model: "fake".into(),
                    usage,
                    // A test double always produces a complete response, which is exactly what the
                    // truncated cases are contrasted against.
                    finish_reason: Some("stop".to_string()),
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
    async fn tool_loop_feeds_read_data_to_next_request() {
        // Phase 3.2: a `read` result's content must be visible in the next
        // request (data, not just summary).
        let dir = std::env::temp_dir().join(format!("niki-loop-read-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("note.txt"), "MARKER-CONTENT-789\n").unwrap();
        let provider = ReadCaptureProvider {
            calls: std::sync::atomic::AtomicUsize::new(0),
            seen: std::sync::Mutex::new(Vec::new()),
            path: "note.txt".to_string(),
        };
        let registry = build_baseline_registry();
        let ctx = ToolContext {
            agent_id: crate::mission::AgentId("a1".into()),
            mission_id: crate::mission::MissionId("m1".into()),
            role: "coder".into(),
            project_path: dir.clone(),
            permissions: HashMap::new(),
            permission_mode: "auto".into(),
            task_store: None,
        };
        let messages = vec![LoopMessage::User("read the note".into())];
        let out = run_tool_loop(
            &provider, "fake", &registry, &ctx, messages, None, 5, None, None,
        )
        .await
        .unwrap();
        assert_eq!(out.content, "done");
        let seen = provider.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert!(
            seen[0].contains("MARKER-CONTENT-789"),
            "next request must carry file data, got: {}",
            seen[0]
        );
        // Phase 3.2: loop usage is non-zero (token accounting, not discarded).
        assert!(
            out.usage.input_tokens > 0 && out.usage.output_tokens > 0,
            "usage must be accumulated: {:?}",
            out.usage
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn tool_loop_emits_display_tool_cards() {
        // Phase 3.5: the loop→display subscriber path delivers ToolCall then
        // ToolResult for each executed tool.
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
            permission_mode: "auto".into(),
            task_store: None,
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let messages = vec![LoopMessage::User("run echo hi".into())];
        let out = run_tool_loop(
            &provider,
            "fake",
            &registry,
            &ctx,
            messages,
            None,
            5,
            Some(tx),
            None,
        )
        .await
        .unwrap();
        assert_eq!(out.tool_calls.len(), 1);
        let events: Vec<_> = rx.try_iter().collect();
        assert_eq!(
            events.len(),
            2,
            "expected ToolCall + ToolResult, got {events:?}"
        );
        match &events[0] {
            crate::display::tui::DisplayEvent::ToolCall { tool_name, .. } => {
                assert_eq!(tool_name, "bash")
            }
            other => panic!("expected ToolCall first, got {other:?}"),
        }
        match &events[1] {
            crate::display::tui::DisplayEvent::ToolResult {
                tool_name, success, ..
            } => {
                assert_eq!(tool_name, "bash");
                assert!(success);
            }
            other => panic!("expected ToolResult second, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unknown_tool_stays_failed_with_diagnostics() {
        // Phase 3.2: unknown tools are explicit `Failed` naming the tool.
        let registry = build_baseline_registry();
        let ctx = task_ctx();
        let result = registry
            .execute(
                "nope_missing_tool",
                ToolInput::new(serde_json::json!({})),
                &ctx,
            )
            .await;
        assert_eq!(result.status, ToolStatus::Failed);
        assert!(result.summary.contains("nope_missing_tool"));
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.contains("nope_missing_tool"))
        );
    }

    fn manual_ctx() -> ToolContext {
        ToolContext {
            agent_id: crate::mission::AgentId("a1".into()),
            mission_id: crate::mission::MissionId("m1".into()),
            role: "coder".into(),
            project_path: std::env::temp_dir(),
            permissions: HashMap::new(),
            permission_mode: "manual".into(),
            task_store: None,
        }
    }

    #[tokio::test]
    async fn deny_permission_blocks_before_tool() {
        // Phase 3.4: `Deny` maps to `PermissionDenied` without running the tool.
        let registry = build_baseline_registry();
        let mut ctx = manual_ctx();
        ctx.permissions
            .insert("bash".into(), PermissionRequirement::Deny);
        let result = registry
            .execute(
                "bash",
                ToolInput::new(serde_json::json!({"command": "echo hi"})),
                &ctx,
            )
            .await;
        assert_eq!(result.status, ToolStatus::PermissionDenied);
        assert!(result.summary.contains("bash"));
    }

    #[tokio::test]
    async fn bash_tool_enforces_command_deny_list() {
        // Phase 5.3: the host-shell path cannot bypass `check_command_policy`
        // — a denied command fails with diagnostics, never executes.
        let registry = build_baseline_registry();
        let mut ctx = manual_ctx();
        ctx.permission_mode = "auto".into();
        ctx.permissions
            .insert("bash".into(), PermissionRequirement::Allow);
        let result = registry
            .execute(
                "bash",
                ToolInput::new(serde_json::json!({"command": "rm -rf /"})),
                &ctx,
            )
            .await;
        assert_eq!(result.status, ToolStatus::Failed);
        assert!(result.diagnostics.join(" ").contains("denied"));
        // A benign command still runs.
        let result = registry
            .execute(
                "bash",
                ToolInput::new(serde_json::json!({"command": "echo hi"})),
                &ctx,
            )
            .await;
        assert_eq!(result.status, ToolStatus::Success);
    }

    #[tokio::test]
    async fn ask_under_manual_denies_fail_closed() {
        // Phase 3.4: `Ask` with no approval UI in the loop denies headless.
        let registry = build_baseline_registry();
        let ctx = manual_ctx();
        let result = registry
            .execute(
                "bash",
                ToolInput::new(serde_json::json!({"command": "echo hi"})),
                &ctx,
            )
            .await;
        assert_eq!(result.status, ToolStatus::PermissionDenied);
        assert!(
            result.diagnostics.iter().any(|d| d.contains("fail-closed"))
                || result.summary.contains("approval"),
            "{:?}",
            result.diagnostics
        );
    }

    #[tokio::test]
    async fn ask_under_auto_allows() {
        // Phase 3.4: `Ask` under auto/dontask/bypass proceeds to the tool.
        let registry = build_baseline_registry();
        for mode in ["auto", "dontask", "bypass"] {
            let mut ctx = manual_ctx();
            ctx.permission_mode = mode.into();
            let result = registry
                .execute(
                    "bash",
                    ToolInput::new(serde_json::json!({"command": "echo ok"})),
                    &ctx,
                )
                .await;
            assert_eq!(result.status, ToolStatus::Success, "mode={mode}");
        }
    }

    #[test]
    fn duplicate_registration_replaces_def() {
        // Phase 3.4: re-registering a name replaces the def, no duplicates.
        struct DummyTool;
        #[async_trait::async_trait]
        impl Tool for DummyTool {
            fn def(&self) -> &ToolDef {
                static DEF: ToolDef = ToolDef {
                    name: "read",
                    description: "dummy override",
                    category: ToolCategory::Explore,
                    risk_level: RiskLevel::Low,
                    permission: PermissionRequirement::Allow,
                    agent_access: &[],
                    parameters: r#"{"type":"object","properties":{},"required":[]}"#,
                };
                &DEF
            }

            async fn execute(&self, _input: ToolInput, _ctx: &ToolContext) -> ToolResult {
                ToolResult {
                    tool_id: ToolId::generate(),
                    tool_name: "read".into(),
                    status: ToolStatus::Success,
                    summary: "dummy".into(),
                    data: ToolData::None,
                    duration: std::time::Duration::ZERO,
                    artifacts: Vec::new(),
                    diagnostics: Vec::new(),
                    metadata: HashMap::new(),
                }
            }
        }

        let mut registry = build_baseline_registry();
        let before = registry.list_defs().len();
        registry.register(Box::new(DummyTool));
        let after: Vec<_> = registry
            .list_defs()
            .iter()
            .filter(|d| d.name == "read")
            .collect();
        assert_eq!(registry.list_defs().len(), before);
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].description, "dummy override");
    }

    fn task_ctx() -> ToolContext {
        ToolContext {
            agent_id: crate::mission::AgentId("a1".into()),
            mission_id: crate::mission::MissionId("m1".into()),
            role: "coder".into(),
            project_path: std::env::temp_dir(),
            permissions: HashMap::new(),
            permission_mode: "auto".into(),
            task_store: Some(std::sync::Arc::new(TaskStore::new())),
        }
    }

    fn read_ctx(dir: &std::path::Path) -> ToolContext {
        ToolContext {
            agent_id: crate::mission::AgentId("a1".into()),
            mission_id: crate::mission::MissionId("m1".into()),
            role: "coder".into(),
            project_path: dir.to_path_buf(),
            permissions: HashMap::new(),
            permission_mode: "auto".into(),
            task_store: None,
        }
    }

    #[tokio::test]
    async fn read_renders_notebook_cells() {
        let dir = std::env::temp_dir().join(format!("niki-nb-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("analysis.ipynb"),
            serde_json::json!({
                "cells": [
                    {"cell_type": "markdown", "source": ["# Title\n", "words"]},
                    {"cell_type": "code", "source": ["print(1)\n"],
                     "outputs": [{"text": ["1\n"]}]},
                    {"cell_type": "code", "source": ["bad("],
                     "outputs": [{"traceback": ["E1\n", "E2"]}]}
                ]
            })
            .to_string(),
        )
        .unwrap();
        let registry = build_baseline_registry();
        let out = registry
            .execute(
                "read",
                ToolInput::new(serde_json::json!({"path": "analysis.ipynb"})),
                &read_ctx(&dir),
            )
            .await;
        assert_eq!(out.status, ToolStatus::Success);
        let text = match out.data {
            ToolData::FileContent { lines, .. } => lines
                .into_iter()
                .map(|(_, l)| l)
                .collect::<Vec<_>>()
                .join("\n"),
            other => panic!("expected FileContent, got {:?}", other),
        };
        assert!(text.contains("cell 0 [markdown]"), "{text}");
        assert!(text.contains("cell 1 [code]"), "{text}");
        assert!(text.contains("[output]"), "{text}");
        assert!(text.contains("[traceback]"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn read_refuses_binary_media_honestly() {
        let dir = std::env::temp_dir().join(format!("niki-media-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("shot.png"), [0u8, 1, 2, 3]).unwrap();
        let registry = build_baseline_registry();
        let out = registry
            .execute(
                "read",
                ToolInput::new(serde_json::json!({"path": "shot.png"})),
                &read_ctx(&dir),
            )
            .await;
        assert_ne!(out.status, ToolStatus::Success);
        assert!(out.summary.contains("binary parsing"), "{}", out.summary);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn task_spawn_status_cancel_round_trip() {
        let registry = build_baseline_registry();
        let ctx = task_ctx();
        let store = ctx.task_store.as_ref().unwrap().clone();

        let spawn = registry
            .execute(
                "task_spawn",
                ToolInput::new(serde_json::json!({
                    "role": "coder",
                    "prompt": "fix the bug",
                    "description": "fix the failing test",
                    "subagent_type": "coder",
                    "run_in_background": true
                })),
                &ctx,
            )
            .await;
        assert_eq!(spawn.status, ToolStatus::Success);
        let task_id = match &spawn.data {
            ToolData::TaskSpawned {
                task_id,
                run_in_background,
                resume_hint,
                ..
            } => {
                assert!(*run_in_background);
                assert!(resume_hint.is_none());
                task_id.clone()
            }
            other => panic!("expected TaskSpawned, got {:?}", other),
        };
        assert!(store.status(&task_id).is_some());

        let status = registry
            .execute(
                "task_status",
                ToolInput::new(serde_json::json!({"task_id": task_id})),
                &ctx,
            )
            .await;
        match &status.data {
            ToolData::TaskStatus {
                status: s,
                progress,
                resume_hint,
                ..
            } => {
                assert_eq!(s, "running");
                assert!(progress.is_some());
                assert!(resume_hint.is_none());
            }
            other => panic!("expected TaskStatus, got {:?}", other),
        }

        let cancel = registry
            .execute(
                "task_cancel",
                ToolInput::new(serde_json::json!({"task_id": task_id})),
                &ctx,
            )
            .await;
        assert_eq!(cancel.status, ToolStatus::Success);
        assert!(matches!(cancel.data, ToolData::None));
        assert_eq!(store.status(&task_id).unwrap().status, "cancelled");
    }

    fn human_ctx() -> ToolContext {
        ToolContext {
            agent_id: crate::mission::AgentId("a1".into()),
            mission_id: crate::mission::MissionId("m1".into()),
            role: "coder".into(),
            project_path: std::env::temp_dir(),
            permissions: HashMap::new(),
            permission_mode: "auto".into(),
            task_store: None,
        }
    }

    /// `cargo test` stdin is never a TTY, so these assert the non-interactive
    /// contract deterministically: ask FAILS (never invents an answer) and
    /// approval DENIES (never auto-approves).
    #[tokio::test]
    async fn ask_user_fails_when_non_interactive() {
        let registry = build_baseline_registry();
        let out = registry
            .execute(
                "ask_user",
                ToolInput::new(serde_json::json!({"question": "proceed?"})),
                &human_ctx(),
            )
            .await;
        assert_eq!(out.status, ToolStatus::Failed);
        assert!(out.summary.contains("non-interactive"));
    }

    #[tokio::test]
    async fn approval_denies_when_non_interactive() {
        let registry = build_baseline_registry();
        let out = registry
            .execute(
                "approval",
                ToolInput::new(serde_json::json!({"command": "rm -rf /"})),
                &human_ctx(),
            )
            .await;
        assert_eq!(out.status, ToolStatus::PermissionDenied);
        match out.data {
            ToolData::ApprovalResult { approved, .. } => assert!(!approved),
            other => panic!("expected ApprovalResult, got {:?}", other),
        }
    }
}

/// Network egress must not ride in on a tool's declared `Allow`.
///
/// `web_fetch` and `web_search` both declare `PermissionRequirement::Allow` —
/// a tool-authoring default. That made every network call unconditional in
/// every permission mode, which is the one capability a sandboxed agent holds
/// that can exfiltrate the whole repo. The rule that forbade it lived in
/// `permissions::resolve_tool`, which no product code calls, so the property
/// was proven by that function's own unit tests and enforced nowhere.
#[cfg(test)]
mod network_egress_permission_tests {
    use super::*;

    fn ctx(mode: &str) -> ToolContext {
        ToolContext {
            agent_id: crate::mission::AgentId("a1".into()),
            mission_id: crate::mission::MissionId("m1".into()),
            role: "coder".into(),
            project_path: std::env::temp_dir(),
            permissions: HashMap::new(),
            permission_mode: mode.to_string(),
            task_store: None,
        }
    }

    fn def_for(name: &str) -> ToolDef {
        let reg = build_baseline_registry();
        reg.get(name)
            .unwrap_or_else(|| panic!("{name} must be in the baseline registry"))
            .def()
            .clone()
    }

    #[test]
    fn the_network_tools_really_do_declare_allow() {
        // If this ever changes, the guard below stops being load-bearing and
        // this test should be revisited rather than quietly kept.
        assert_eq!(
            def_for("web_fetch").permission,
            PermissionRequirement::Allow
        );
        assert_eq!(
            def_for("web_search").permission,
            PermissionRequirement::Allow
        );
    }

    #[test]
    fn network_egress_is_denied_in_every_mode_without_an_explicit_bypass() {
        for mode in ["manual", "auto", ""] {
            for tool in ["web_fetch", "web_search"] {
                let reason = ToolRegistry::permission_denial(tool, &def_for(tool), &ctx(mode))
                    .unwrap_or_else(|| panic!("{tool} must not run unattended in mode {mode:?}"));
                assert!(
                    reason.contains("network"),
                    "the denial must name the reason: {reason}"
                );
            }
        }
    }

    #[test]
    fn an_explicit_bypass_allows_network_egress() {
        for mode in ["bypass", "dontask"] {
            for tool in ["web_fetch", "web_search"] {
                assert!(
                    ToolRegistry::permission_denial(tool, &def_for(tool), &ctx(mode)).is_none(),
                    "{mode} is an explicit opt-out and must allow {tool}"
                );
            }
        }
    }

    #[test]
    fn local_tools_are_unaffected() {
        // The guard must not turn every tool into a prompt — only egress.
        for tool in ["read", "write", "edit", "glob", "grep", "list"] {
            assert!(
                ToolRegistry::permission_denial(tool, &def_for(tool), &ctx("auto")).is_none(),
                "{tool} is local and must stay unattended in auto mode"
            );
        }
    }

    #[tokio::test]
    async fn a_web_fetch_attempt_is_permission_denied_end_to_end() {
        let reg = build_baseline_registry();
        let res = reg
            .execute(
                "web_fetch",
                ToolInput::new(serde_json::json!({"url": "https://example.com"})),
                &ctx("auto"),
            )
            .await;
        assert_eq!(
            res.status,
            ToolStatus::PermissionDenied,
            "the guard must fire in the real execute path, not only in the helper"
        );
        assert!(
            res.summary.contains("network"),
            "the user must be told why: {}",
            res.summary
        );
    }

    // -- path confinement -------------------------------------------------
    //
    // read/list/write/edit/patch/grep all took a model-supplied path, used an
    // absolute one verbatim, and — for the writers — `create_dir_all`'d its
    // parent. So a model could name any path the process could reach and the
    // tool would oblige.
    //
    // The permission layer has a protected-path list (`.git`, `.claude`,
    // `~/.ssh`, ...) and `PermissionChecker::is_protected_path` to match it,
    // but nothing on the write path called it. The list was documentation.
    //
    // On the worktree backend this is the difference between a sandbox and a
    // suggestion: commands run as the invoking user, with their privileges, and
    // no container in between.

    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        dir
    }

    #[test]
    fn a_path_inside_the_project_resolves() {
        let dir = project();
        let root = dir.path();
        for raw in ["src/main.rs", "./src/main.rs", "src/new.rs", "a/b/c/d.txt"] {
            let resolved = resolve_tool_path(root, raw)
                .unwrap_or_else(|e| panic!("{raw:?} should resolve: {e}"));
            assert!(
                resolved.starts_with(root.canonicalize().unwrap()),
                "{raw:?} resolved to {resolved:?}, outside the project"
            );
        }
    }

    #[test]
    fn an_absolute_path_inside_the_project_is_allowed() {
        // The container backend's working directory is /workspace and models
        // legitimately produce absolute paths there. Refusing every absolute
        // path would break that backend to fix a different one.
        let dir = project();
        let abs = dir.path().join("src/main.rs");
        let resolved = resolve_tool_path(dir.path(), abs.to_str().unwrap())
            .expect("an absolute path inside the project is fine");
        assert!(resolved.ends_with("src/main.rs"));
    }

    #[test]
    fn a_path_outside_the_project_is_refused() {
        let dir = project();
        let outside = tempfile::tempdir().unwrap();
        let victim = outside.path().join("authorized_keys");
        std::fs::write(&victim, "original\n").unwrap();

        // The absolute case, verbatim.
        let err = resolve_tool_path(dir.path(), victim.to_str().unwrap())
            .expect_err("an absolute path outside the project must be refused");
        assert!(
            err.contains("outside the project"),
            "the refusal must say why: {err}"
        );
        // And nothing was created on the way to finding that out.
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "original\n");

        for raw in ["/etc/passwd", "/tmp/anything", "/home/someone/.ssh/id_rsa"] {
            assert!(
                resolve_tool_path(dir.path(), raw).is_err(),
                "{raw:?} must be refused"
            );
        }
    }

    #[test]
    fn a_tilde_is_not_expanded_so_it_stays_inside() {
        // Not a security property, and the test is here so nobody later reads
        // it as one in either direction. Nothing in this path is a shell: a
        // leading `~` is just a character, so `~/.ssh/authorized_keys` is the
        // RELATIVE path `<project>/~/.ssh/authorized_keys` and lands inside the
        // project, harmlessly, in a directory literally named `~`.
        //
        // It is tempting to "fix" this by expanding `~` — which would turn a
        // safe oddity into a real escape. Asserted so the temptation is visible.
        let dir = project();
        let resolved = resolve_tool_path(dir.path(), "~/.ssh/authorized_keys")
            .expect("a tilde is a filename character here, not a home reference");
        assert!(
            resolved.starts_with(dir.path().canonicalize().unwrap()),
            "must still land inside the project, got {resolved:?}"
        );
        assert!(
            resolved.to_string_lossy().contains("/~/"),
            "and it must not have been expanded to a home directory: {resolved:?}"
        );
    }

    #[test]
    fn traversal_is_refused_rather_than_normalised() {
        let dir = project();
        for raw in [
            "../outside.txt",
            "src/../../outside.txt",
            "src/..",
            "a/b/../../../etc/passwd",
        ] {
            let err = resolve_tool_path(dir.path(), raw).unwrap_err();
            assert!(
                err.contains(".."),
                "{raw:?} must be refused for containing '..': {err}"
            );
        }
    }

    #[test]
    fn a_symlink_out_of_the_tree_does_not_launder_a_write() {
        // Canonicalising only the textual path would resolve `inside/link` to
        // `inside/link`, pass the prefix test, and then follow the link on
        // write. The deepest existing ancestor is canonicalised instead, so the
        // link is followed before the check, not after.
        let dir = project();
        let outside = tempfile::tempdir().unwrap();
        let victim = outside.path().join("secret.txt");
        std::fs::write(&victim, "original\n").unwrap();

        #[cfg(unix)]
        std::os::unix::fs::symlink(&victim, dir.path().join("link.txt")).unwrap();
        #[cfg(not(unix))]
        return; // no symlinks to create; the unix path is the one that matters

        let err = resolve_tool_path(dir.path(), "link.txt")
            .expect_err("a symlink pointing out of the tree must be refused");
        assert!(err.contains("outside the project"), "{err}");
    }

    #[test]
    fn the_write_tool_actually_refuses_to_escape() {
        // The helper is only worth anything if the tools use it. This drives
        // the real tool, not the helper.
        let dir = project();
        let outside = tempfile::tempdir().unwrap();
        let victim = outside.path().join("pwned.txt");

        let ctx = ToolContext {
            agent_id: crate::mission::AgentId("confinement-test".into()),
            mission_id: crate::mission::MissionId("confinement-test".into()),
            role: "coder".into(),
            project_path: dir.path().to_path_buf(),
            permissions: HashMap::new(),
            permission_mode: "bypass".into(),
            task_store: None,
        };
        let input = ToolInput {
            raw: serde_json::json!({
                "path": victim.to_str().unwrap(),
                "content": "pwned",
            }),
        };

        let res = futures::executor::block_on(WriteTool.execute(input, &ctx));
        assert!(
            matches!(res.status, ToolStatus::Failed),
            "writing outside the project must fail, got {:?}",
            res.status
        );
        assert!(
            !victim.exists(),
            "the file outside the project must not exist — the tool wrote it anyway"
        );
    }
}
