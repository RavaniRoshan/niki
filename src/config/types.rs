use crate::artifacts::types::{AgentRole, Complexity};
use crate::sandbox::SandboxBackend;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use toml;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NikiConfig {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
    #[serde(default)]
    pub agents: AgentsConfig,
    #[serde(default)]
    pub docker: DockerConfig,
    /// Optional data-driven pipeline topology. When `stages` is empty the
    /// pipeline falls back to the classic Planner → Coder → Tester → Reviewer
    /// wiring derived from `[agents]`.
    #[serde(default)]
    pub pipeline: PipelineConfig,
    /// Optional extra context ingestion: project doc files and external URLs.
    #[serde(default)]
    pub knowledge: KnowledgeConfig,
    /// Optional independent security audit pass (#4). When enabled, a
    /// SecurityAuditor stage is injected after the Reviewer.
    #[serde(default)]
    pub security: SecurityConfig,
    /// Optional parallel-coder mode (#3). When enabled with `coder_count > 1`,
    /// N coder agents run concurrently (each isolated in its own git worktree),
    /// then a Synthesizer reconciles their diffs into one change.
    #[serde(default)]
    pub parallel: ParallelConfig,
    /// Adversarial "Red/Blue" verification (#1.2). When enabled, an independent
    /// Red agent probes the Coder's diff before the Reviewer runs; the Reviewer
    /// must reconcile each Red challenge (uphold or refute). This is what makes
    /// "independent review" real instead of a rubber stamp.
    #[serde(default)]
    pub red_blue: RedBlueConfig,
    /// Goal runner configuration.
    #[serde(default)]
    pub goal: GoalConfig,
    /// TUI display configuration.
    #[serde(default)]
    pub ui: UiConfig,
    /// Session management configuration.
    #[serde(default)]
    pub session: SessionConfig,
    /// Context compaction configuration.
    #[serde(default)]
    pub compaction: CompactionConfig,
    /// MCP server configurations.
    #[serde(default)]
    pub mcp: McpConfig,
    /// Permission system configuration.
    #[serde(default)]
    pub permissions: PermissionsConfig,
    /// AGENTS.md / project instructions configuration.
    #[serde(default)]
    pub instructions: InstructionsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KnowledgeConfig {
    /// Glob patterns (relative to the project root) of extra doc files to
    /// include as agent context (e.g. `["docs/**/*.md", "README.md"]`).
    #[serde(default)]
    pub doc_globs: Vec<String>,
    /// External URLs (READMEs, linked docs, wikis, issues) fetched and included
    /// as agent context. Fetched best-effort; a failed fetch is skipped.
    #[serde(default)]
    pub urls: Vec<String>,
    /// Max characters ingested per external source, bounding context size.
    #[serde(default = "default_max_source_chars")]
    pub max_source_chars: usize,
}

/// Per-role security policy controlling which commands an agent may execute.
///
/// Each role can have its own policy keyed by role name in
/// `SecurityConfig::policies`. When a command is attempted through the sandbox,
/// it is checked against the deny-list first; denied commands are rejected with
/// a clear error. An empty allow-list means "allow everything not denied".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicyConfig {
    /// Exact command prefixes that are always allowed (bypasses deny-check).
    #[serde(default)]
    pub allowed_commands: Vec<String>,
    /// Command prefixes that are always rejected. Checked against the first
    /// token(s) of the command string (space-separated).
    #[serde(default)]
    pub denied_commands: Vec<String>,
    /// Maximum seconds a single `exec` call may run before being killed.
    #[serde(default = "default_max_exec_seconds")]
    pub max_exec_seconds: u64,
}

impl Default for SecurityPolicyConfig {
    fn default() -> Self {
        Self {
            allowed_commands: Vec::new(),
            denied_commands: default_global_deny_list(),
            max_exec_seconds: default_max_exec_seconds(),
        }
    }
}

fn default_max_exec_seconds() -> u64 {
    300
}

/// Global deny-list applied to every role unless overridden.
fn default_global_deny_list() -> Vec<String> {
    vec![
        "git push --force".to_string(),
        "git push -f".to_string(),
        "rm -rf /".to_string(),
        "rm -rf /*".to_string(),
        "mkfs".to_string(),
        "dd".to_string(),
        "curl | sh".to_string(),
        "curl | bash".to_string(),
        "wget | sh".to_string(),
        "wget | bash".to_string(),
        "--no-verify".to_string(),
    ]
}

/// Per-role defaults for the built-in agent roles.
pub fn default_tester_policy() -> SecurityPolicyConfig {
    SecurityPolicyConfig {
        allowed_commands: vec![
            "cargo test".to_string(),
            "npm test".to_string(),
            "npx vitest".to_string(),
            "npx jest".to_string(),
            "python3 -m pytest".to_string(),
            "go test".to_string(),
            "cargo check".to_string(),
            "npx tsc".to_string(),
            "pyflakes".to_string(),
            "python3 -m py_compile".to_string(),
            "go build".to_string(),
            "git diff".to_string(),
            "git log".to_string(),
            "git status".to_string(),
            "ls".to_string(),
            "cat".to_string(),
            "head".to_string(),
            "tail".to_string(),
            "grep".to_string(),
            "find".to_string(),
            "wc".to_string(),
        ],
        denied_commands: vec![
            "git push".to_string(),
            "git commit".to_string(),
            "git merge".to_string(),
            "git rebase".to_string(),
            "git checkout".to_string(),
            "git reset".to_string(),
            "git branch -D".to_string(),
            "rm".to_string(),
            "mv".to_string(),
            "cp".to_string(),
            "mkdir".to_string(),
            "touch".to_string(),
            "chmod".to_string(),
            "chown".to_string(),
        ],
        max_exec_seconds: default_max_exec_seconds(),
    }
}

pub fn default_coder_policy() -> SecurityPolicyConfig {
    SecurityPolicyConfig {
        allowed_commands: vec![
            "cargo test".to_string(),
            "cargo check".to_string(),
            "cargo build".to_string(),
            "npm test".to_string(),
            "npm install".to_string(),
            "npx vitest".to_string(),
            "npx jest".to_string(),
            "npx tsc".to_string(),
            "python3 -m pytest".to_string(),
            "python3 -m py_compile".to_string(),
            "go test".to_string(),
            "go build".to_string(),
            "git diff".to_string(),
            "git log".to_string(),
            "git status".to_string(),
            "git add".to_string(),
            "git commit".to_string(),
            "git branch".to_string(),
            "ls".to_string(),
            "cat".to_string(),
            "head".to_string(),
            "tail".to_string(),
            "grep".to_string(),
            "find".to_string(),
            "wc".to_string(),
            "mkdir".to_string(),
            "touch".to_string(),
            "mv".to_string(),
            "cp".to_string(),
            "rm".to_string(),
        ],
        denied_commands: vec![
            "git push".to_string(),
            "git push --force".to_string(),
            "git push -f".to_string(),
        ],
        max_exec_seconds: default_max_exec_seconds(),
    }
}

pub fn default_reviewer_policy() -> SecurityPolicyConfig {
    SecurityPolicyConfig {
        allowed_commands: vec![
            "git diff".to_string(),
            "git log".to_string(),
            "git status".to_string(),
            "git show".to_string(),
            "git blame".to_string(),
            "git annotate".to_string(),
            "ls".to_string(),
            "cat".to_string(),
            "head".to_string(),
            "tail".to_string(),
            "grep".to_string(),
            "find".to_string(),
            "wc".to_string(),
            "cargo check".to_string(),
            "npx tsc".to_string(),
            "python3 -m py_compile".to_string(),
            "go build".to_string(),
        ],
        denied_commands: vec![
            "git push".to_string(),
            "git commit".to_string(),
            "git merge".to_string(),
            "git rebase".to_string(),
            "git checkout".to_string(),
            "git reset".to_string(),
            "git branch -D".to_string(),
            "rm".to_string(),
            "mv".to_string(),
            "cp".to_string(),
            "mkdir".to_string(),
            "touch".to_string(),
            "chmod".to_string(),
            "chown".to_string(),
        ],
        max_exec_seconds: default_max_exec_seconds(),
    }
}

/// Configuration for the optional independent security audit pass (#4).
///
/// When `enabled`, the pipeline injects a `SecurityAuditor` stage (driven by
/// `provider`/`model`, defaulting to the configured `security_auditor` agent)
/// after the Reviewer. The audit verdict is recorded as an artifact but does not
/// gate the revision loop by default.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Optional provider override; defaults to `[agents] security_auditor.provider`.
    #[serde(default)]
    pub provider: Option<String>,
    /// Optional model override; defaults to `[agents] security_auditor.model`.
    #[serde(default)]
    pub model: Option<String>,
    /// Per-role security policies keyed by role name (e.g. "tester", "coder").
    /// When a role has a policy, commands executed in its sandbox are checked
    /// against the deny-list before running.
    #[serde(default = "default_policies")]
    pub policies: HashMap<String, SecurityPolicyConfig>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: None,
            model: None,
            policies: default_policies(),
        }
    }
}

fn default_policies() -> HashMap<String, SecurityPolicyConfig> {
    let mut m = HashMap::new();
    m.insert("tester".to_string(), default_tester_policy());
    m.insert("coder".to_string(), default_coder_policy());
    m.insert("reviewer".to_string(), default_reviewer_policy());
    m
}

/// Configuration for the optional parallel-coder mode (#3).
///
/// When `enabled` with `coder_count > 1`, the pipeline runs that many Coder
/// agents concurrently — each isolated in its own git worktree so their changes
/// never collide — then a `Synthesizer` stage reconciles the diffs into one
/// change the rest of the pipeline consumes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_coder_count")]
    pub coder_count: u32,
}

impl Default for ParallelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            coder_count: default_coder_count(),
        }
    }
}

/// Configuration for the adversarial "Red/Blue" verification pass (#1.2).
///
/// When `enabled`, the pipeline injects a `Red` stage immediately before the
/// Reviewer. The Red agent independently attacks the Coder's diff; the Reviewer
/// must then reconcile each Red challenge (uphold → request revision, or refute
/// → justify). This is what prevents the Reviewer from silently ratifying the
/// Coder and is enabled by default because it is the product's core thesis:
/// *isolated* agents that genuinely challenge each other.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedBlueConfig {
    #[serde(default = "default_red_blue_enabled")]
    pub enabled: bool,
    /// Optional provider override; defaults to `[agents] red.provider`.
    #[serde(default)]
    pub provider: Option<String>,
    /// Optional model override; defaults to `[agents] red.model`.
    #[serde(default)]
    pub model: Option<String>,
}

impl Default for RedBlueConfig {
    fn default() -> Self {
        // Red/Blue is on by default — it is the product's core thesis (isolated
        // agents that genuinely challenge each other, not a rubber stamp).
        Self {
            enabled: default_red_blue_enabled(),
            provider: None,
            model: None,
        }
    }
}

fn default_red_blue_enabled() -> bool {
    true
}

/// Goal runner configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalConfig {
    #[serde(default = "default_goal_max_iterations")]
    pub max_iterations: u32,
    /// Prefix for goal git branches (default: "goal").
    #[serde(default)]
    pub branch_prefix: String,
    /// Directory for goal state files (relative to project root).
    #[serde(default)]
    pub state_dir: String,
    /// Fail fast when parallel stages error (default: true).
    #[serde(default = "default_goal_fail_fast")]
    pub fail_fast: bool,
    /// Number of retry attempts for transient LLM errors (default: 3).
    #[serde(default)]
    pub retry_attempts: u32,
    /// Delay between retries in milliseconds (default: 1000).
    #[serde(default)]
    pub retry_delay_ms: u64,
}

impl Default for GoalConfig {
    fn default() -> Self {
        Self {
            max_iterations: default_goal_max_iterations(),
            branch_prefix: String::new(),
            state_dir: String::new(),
            fail_fast: default_goal_fail_fast(),
            retry_attempts: 3,
            retry_delay_ms: 1000,
        }
    }
}

fn default_goal_max_iterations() -> u32 {
    30
}

fn default_goal_fail_fast() -> bool {
    true
}

/// Theme preference for the TUI (light/dark/auto).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ThemePreference {
    #[default]
    Auto,
    Dark,
    Light,
}

impl ThemePreference {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s {
            "dark" => ThemePreference::Dark,
            "light" => ThemePreference::Light,
            _ => ThemePreference::Auto,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ThemePreference::Auto => "auto",
            ThemePreference::Dark => "dark",
            ThemePreference::Light => "light",
        }
    }
}

/// TUI display configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UiConfig {
    #[serde(default)]
    pub tips: TipsConfig,
    #[serde(default)]
    pub theme: ThemePreference,
}

/// Tips banner configuration for the TUI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TipsConfig {
    #[serde(default = "default_tips_enabled")]
    pub enabled: bool,
    #[serde(default = "default_tips_rotation_seconds")]
    pub rotation_seconds: u64,
}

impl Default for TipsConfig {
    fn default() -> Self {
        Self {
            enabled: default_tips_enabled(),
            rotation_seconds: default_tips_rotation_seconds(),
        }
    }
}

fn default_tips_enabled() -> bool {
    true
}

fn default_tips_rotation_seconds() -> u64 {
    30
}

/// Session management configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    #[serde(default = "default_session_enabled")]
    pub enabled: bool,
    #[serde(default = "default_max_sessions")]
    pub max_sessions: usize,
    #[serde(default)]
    pub auto_save: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            enabled: default_session_enabled(),
            max_sessions: default_max_sessions(),
            auto_save: true,
        }
    }
}

fn default_session_enabled() -> bool {
    true
}

fn default_max_sessions() -> usize {
    50
}

/// Context compaction configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    #[serde(default = "default_compaction_enabled")]
    pub enabled: bool,
    #[serde(default = "default_compaction_threshold")]
    pub threshold_pct: u8,
    #[serde(default = "default_compaction_reserved_tokens")]
    pub reserved_tokens: u32,
    #[serde(default)]
    pub auto_compact: bool,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            enabled: default_compaction_enabled(),
            threshold_pct: default_compaction_threshold(),
            reserved_tokens: default_compaction_reserved_tokens(),
            auto_compact: true,
        }
    }
}

fn default_compaction_enabled() -> bool {
    true
}

fn default_compaction_threshold() -> u8 {
    80
}

fn default_compaction_reserved_tokens() -> u32 {
    4096
}

/// MCP server configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: Vec<McpServerConfigEntry>,
    #[serde(default = "default_mcp_enabled")]
    pub enabled: bool,
    #[serde(default = "default_mcp_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_mcp_enabled() -> bool {
    true
}

fn default_mcp_timeout_ms() -> u64 {
    5000
}

/// A single MCP server configuration entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfigEntry {
    pub name: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    #[serde(default = "default_mcp_server_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

fn default_mcp_server_enabled() -> bool {
    true
}

/// Permission system configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PermissionsConfig {
    #[serde(default)]
    pub auto_approve: bool,
    #[serde(default)]
    pub rules: Vec<PermissionRuleConfig>,
}

/// A single permission rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRuleConfig {
    pub action: String,
    pub permission: String,
    pub pattern: Option<String>,
}

/// AGENTS.md / project instructions configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstructionsConfig {
    #[serde(default = "default_instructions_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default = "default_auto_detect_agents_md")]
    pub auto_detect_agents_md: bool,
}

impl Default for InstructionsConfig {
    fn default() -> Self {
        Self {
            enabled: default_instructions_enabled(),
            paths: Vec::new(),
            auto_detect_agents_md: default_auto_detect_agents_md(),
        }
    }
}

fn default_instructions_enabled() -> bool {
    true
}

fn default_auto_detect_agents_md() -> bool {
    true
}

fn default_coder_count() -> u32 {
    2
}

fn default_max_source_chars() -> usize {
    8000
}

fn default_single_agent_max_complexity() -> Complexity {
    // Bounded/sequential tasks (Low complexity) are the ones that don't benefit
    // from the multi-agent chain's isolation tax, so they collapse to the
    // single-agent fast-path by default (BUILD_PLAN 3.2).
    Complexity::Low
}

fn default_output_dir() -> String {
    ".niki".to_string()
}

/// Which agent topology NIKI uses for a run (BUILD_PLAN 3.2, P2.2).
///
/// - `Auto` (default): pick by task shape — bounded/sequential tasks collapse
///   to the single-agent fast-path; everything else runs the full multi-agent
///   chain (which is what pays for the isolation guarantees).
/// - `MultiAgent`: always run the full Planner → Coder → Tester → Reviewer
///   (± Red/Blue, SecurityAuditor) chain.
/// - `SingleAgent`: always use the fast-path (one solo Coder session after the
///   Planner), skipping the Tester/Reviewer/Red re-ingestion tax.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TopologyMode {
    /// Decide per task shape (estimated complexity + whether security/parallel need the full chain).
    #[default]
    Auto,
    /// Always run the full multi-agent chain.
    MultiAgent,
    /// Always collapse to the single-agent fast-path.
    SingleAgent,
}

/// A user-defined, ordered pipeline of agent stages.
///
/// This replaces the hardcoded flow: each stage binds an `AgentRole` to a
/// provider/model, and may be skipped. The revision loop re-runs every stage
/// after the Planner (in order) until a Reviewer stage returns a terminal
/// verdict or `max_revision_rounds` is exhausted.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PipelineConfig {
    #[serde(default)]
    pub stages: Vec<PipelineStageConfig>,
    /// Override for the revision loop length; falls back to `general.max_revision_rounds`.
    #[serde(default)]
    pub max_revision_rounds: Option<u32>,
    /// Agent topology for the run (BUILD_PLAN 3.2). `Auto` lets NIKI pick by
    /// task shape; the other variants force a topology.
    #[serde(default)]
    pub topology: TopologyMode,
    /// In `Auto` mode, tasks whose `estimated_complexity` is at or below this
    /// level collapse to the single-agent fast-path. Defaults to `Low`.
    #[serde(default = "default_single_agent_max_complexity")]
    pub single_agent_max_complexity: Complexity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStageConfig {
    pub role: AgentRole,
    pub provider: String,
    pub model: String,
    /// When true, this stage is omitted from the run.
    #[serde(default)]
    pub skip: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub max_revision_rounds: u32,
    #[serde(default = "default_output_dir")]
    pub output_dir: String,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            max_revision_rounds: 3,
            output_dir: ".niki".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub default_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentsConfig {
    #[serde(default = "default_anthropic_agent")]
    pub planner: AgentConfig,
    #[serde(default = "default_anthropic_agent")]
    pub coder: AgentConfig,
    #[serde(default = "default_openai_agent")]
    pub tester: AgentConfig,
    #[serde(default = "default_anthropic_agent")]
    pub reviewer: AgentConfig,
    /// Reconciles parallel coder diffs into one coherent change (#3).
    #[serde(default = "default_anthropic_agent")]
    pub synthesizer: AgentConfig,
    /// Independent security review pass (#4).
    #[serde(default = "default_anthropic_agent")]
    pub security_auditor: AgentConfig,
    /// Adversarial "Red" agent (#1.2). Runs a strong model by default because its
    /// job is to find what the Coder and Reviewer missed.
    #[serde(default = "default_red_agent")]
    pub red: AgentConfig,
}

impl Default for AgentsConfig {
    fn default() -> Self {
        Self {
            planner: default_anthropic_agent(),
            coder: default_anthropic_agent(),
            tester: default_openai_agent(),
            reviewer: default_anthropic_agent(),
            synthesizer: default_anthropic_agent(),
            security_auditor: default_anthropic_agent(),
            red: default_red_agent(),
        }
    }
}

fn default_red_agent() -> AgentConfig {
    AgentConfig {
        provider: "anthropic".to_string(),
        model: "claude-opus-4".to_string(),
    }
}

fn default_anthropic_agent() -> AgentConfig {
    AgentConfig {
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4-20250514".to_string(),
    }
}

fn default_openai_agent() -> AgentConfig {
    AgentConfig {
        provider: "openai".to_string(),
        model: "gpt-4o-mini".to_string(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentConfig {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerConfig {
    pub base_image: String,
    pub extra_packages: Vec<String>,
    pub memory_limit: String,
    pub cpu_limit: f32,
    /// Sandbox backend: `docker` (container, default), `worktree` (git worktree +
    /// local process, no Docker), or `cloud` (NIKI infra, beta).
    #[serde(default)]
    pub backend: SandboxBackend,
}

impl Default for DockerConfig {
    fn default() -> Self {
        Self {
            base_image: "niki-sandbox:24.04".to_string(),
            extra_packages: vec!["nodejs".into(), "npm".into(), "python3".into()],
            memory_limit: "2g".to_string(),
            cpu_limit: 2.0,
            backend: SandboxBackend::Docker,
        }
    }
}

impl NikiConfig {
    pub fn load(project_dir: &Path) -> Result<Self> {
        let mut config = Self::default();

        let global_path = dirs::home_dir().map(|h| h.join(".config/niki/niki.toml"));

        let local_path = project_dir.join("niki.toml");

        if let Some(gp) = &global_path
            && gp.exists()
        {
            let content = fs::read_to_string(gp)?;
            let c: NikiConfig = toml::from_str(&content)?;
            config.merge(c);
        }

        if local_path.exists() {
            let content = fs::read_to_string(&local_path)?;
            let c: NikiConfig = toml::from_str(&content)?;
            config.merge(c);
        }

        config.apply_env_vars();

        Ok(config)
    }

    /// Save theme preference to global config using toml::Value mutation.
    /// Never uses toml::to_string(&NikiConfig) to avoid clobbering user config.
    pub fn save_theme(preference: ThemePreference) -> Result<()> {
        let global_path = dirs::home_dir()
            .map(|h| h.join(".config/niki/niki.toml"))
            .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;

        // Read existing or start with empty table
        let mut root: toml::Value = if global_path.exists() {
            let content = fs::read_to_string(&global_path)?;
            toml::from_str(&content)?
        } else {
            toml::Value::Table(toml::map::Map::new())
        };

        // Ensure [ui] table exists
        let table = root
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("Config root is not a table"))?;
        if !table.contains_key("ui") {
            table.insert("ui".to_string(), toml::Value::Table(toml::map::Map::new()));
        }

        let ui = table
            .get_mut("ui")
            .and_then(|v| v.as_table_mut())
            .ok_or_else(|| anyhow::anyhow!("ui section is not a table"))?;

        ui.insert(
            "theme".to_string(),
            toml::Value::String(preference.as_str().to_string()),
        );

        // Atomic write: write to temp file, then rename
        let parent = global_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Config path has no parent"))?;
        fs::create_dir_all(parent)?;
        let tmp_path = parent.join("niki.toml.tmp");
        fs::write(&tmp_path, toml::to_string_pretty(&root)?)?;
        fs::rename(&tmp_path, &global_path)?;

        Ok(())
    }

    fn merge(&mut self, other: NikiConfig) {
        self.general.max_revision_rounds = other.general.max_revision_rounds;
        self.general.output_dir = other.general.output_dir;

        for (k, v) in other.providers {
            self.providers.insert(k, v);
        }

        self.agents = other.agents;

        // Only merge docker settings if the other config explicitly changed them
        // from defaults. This preserves global backend settings when the local
        // config doesn't set a [docker] section.
        let default_docker = DockerConfig::default();
        if other.docker.base_image != default_docker.base_image {
            self.docker.base_image = other.docker.base_image;
        }
        if other.docker.extra_packages != default_docker.extra_packages {
            self.docker.extra_packages = other.docker.extra_packages;
        }
        if other.docker.memory_limit != default_docker.memory_limit {
            self.docker.memory_limit = other.docker.memory_limit;
        }
        if (other.docker.cpu_limit - default_docker.cpu_limit).abs() > f32::EPSILON {
            self.docker.cpu_limit = other.docker.cpu_limit;
        }
        if other.docker.backend != default_docker.backend {
            self.docker.backend = other.docker.backend;
        }

        // Topology overrides are additive: only apply the parts the user set.
        if !other.pipeline.stages.is_empty() {
            self.pipeline.stages = other.pipeline.stages;
        }
        if other.pipeline.max_revision_rounds.is_some() {
            self.pipeline.max_revision_rounds = other.pipeline.max_revision_rounds;
        }
        if other.pipeline.topology != TopologyMode::default() {
            self.pipeline.topology = other.pipeline.topology;
        }

        // Knowledge ingestion is additive: union the doc globs and URLs.
        self.knowledge.doc_globs.extend(other.knowledge.doc_globs);
        self.knowledge.urls.extend(other.knowledge.urls);
        if other.knowledge.max_source_chars != default_max_source_chars() {
            self.knowledge.max_source_chars = other.knowledge.max_source_chars;
        }

        // Security audit is an explicit toggle: if the other config enabled it,
        // adopt its enabled flag and any provider/model overrides.
        if other.security.enabled {
            self.security.enabled = true;
            if let Some(p) = other.security.provider {
                self.security.provider = Some(p);
            }
            if let Some(m) = other.security.model {
                self.security.model = Some(m);
            }
        }

        // Parallel-coder mode is also an explicit toggle.
        if other.parallel.enabled {
            self.parallel.enabled = true;
            self.parallel.coder_count = other.parallel.coder_count;
        }

        // Red/Blue verification is an explicit toggle (default on, but a user
        // can turn it off). Adopt the enabled flag and any provider/model overrides.
        if other.red_blue.enabled {
            self.red_blue.enabled = true;
            if let Some(p) = other.red_blue.provider {
                self.red_blue.provider = Some(p);
            }
            if let Some(m) = other.red_blue.model {
                self.red_blue.model = Some(m);
            }
        }

        // UI tips config: only override if the other config explicitly changed defaults.
        let default_tips = TipsConfig::default();
        if other.ui.tips.enabled != default_tips.enabled {
            self.ui.tips.enabled = other.ui.tips.enabled;
        }
        if other.ui.tips.rotation_seconds != default_tips.rotation_seconds {
            self.ui.tips.rotation_seconds = other.ui.tips.rotation_seconds;
        }

        // UI theme preference: only override if not default (Auto).
        if other.ui.theme != ThemePreference::default() {
            self.ui.theme = other.ui.theme;
        }
    }

    fn apply_env_vars(&mut self) {
        // Ensure provider entries exist so that environment variables are picked up
        // even when no provider block is present in the TOML config.
        self.providers.entry("anthropic".to_string()).or_default();
        self.providers.entry("openai".to_string()).or_default();
        self.providers.entry("google".to_string()).or_default();

        // Standard provider keys take precedence, so a vanilla `ANTHROPIC_API_KEY`
        // (or `OPENAI_API_KEY`) always wins. Gateway-style tokens
        // (ANTHROPIC_AUTH_TOKEN / OPENROUTER_API_KEY) are only fallbacks. This keeps
        // NIKI standard and BYOK: users supply their own OpenAI/Anthropic (or any
        // compatible) key via env or `niki.toml`, and nothing is tied to a specific
        // gateway.
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY")
            && !key.is_empty()
            && let Some(p) = self.providers.get_mut("anthropic")
        {
            p.api_key = Some(key);
        }
        if let Ok(key) = std::env::var("NIKI_PROVIDERS_ANTHROPIC_API_KEY")
            && !key.is_empty()
            && let Some(p) = self.providers.get_mut("anthropic")
            && p.api_key.is_none()
        {
            p.api_key = Some(key);
        }
        if let Some(p) = self.providers.get_mut("anthropic")
            && p.api_key.is_none()
        {
            if let Ok(token) = std::env::var("ANTHROPIC_AUTH_TOKEN") {
                if !token.is_empty() {
                    p.api_key = Some(token);
                }
            } else if let Ok(key) = std::env::var("OPENROUTER_API_KEY")
                && !key.is_empty()
            {
                p.api_key = Some(key);
            }
        }
        if let Ok(key) = std::env::var("OPENAI_API_KEY")
            && let Some(p) = self.providers.get_mut("openai")
            && p.api_key.is_none()
        {
            p.api_key = Some(key);
        }
        if let Ok(key) = std::env::var("GOOGLE_API_KEY")
            && let Some(p) = self.providers.get_mut("google")
            && p.api_key.is_none()
        {
            p.api_key = Some(key);
        }

        // Standard base-URL overrides (SDK convention: a host/base, not the full
        // endpoint — the provider appends the path). Env takes precedence over
        // whatever is in niki.toml.
        if let Ok(base) = std::env::var("ANTHROPIC_BASE_URL")
            && !base.is_empty()
            && let Some(p) = self.providers.get_mut("anthropic")
        {
            p.base_url = Some(base.trim_end_matches('/').to_string());
        }
        if let Ok(base) = std::env::var("OPENAI_BASE_URL")
            && !base.is_empty()
            && let Some(p) = self.providers.get_mut("openai")
        {
            p.base_url = Some(base.trim_end_matches('/').to_string());
        }

        // Standard model overrides. Applied to agents still using the provider's
        // built-in default, so an explicit per-agent model in niki.toml is respected.
        if let Ok(model) = std::env::var("ANTHROPIC_MODEL")
            && !model.is_empty()
        {
            if let Some(p) = self.providers.get_mut("anthropic") {
                p.default_model = model.clone();
            }
            apply_env_model_to_agents(&mut self.agents, "anthropic", &model);
        }
        if let Ok(model) = std::env::var("OPENAI_MODEL")
            && !model.is_empty()
        {
            if let Some(p) = self.providers.get_mut("openai") {
                p.default_model = model.clone();
            }
            apply_env_model_to_agents(&mut self.agents, "openai", &model);
        }
    }
}

/// Override an agent's model with an env-provided model when that agent is bound
/// to `provider` and is still using the provider's built-in default. Agents with
/// an explicit model set in niki.toml are left untouched.
fn apply_env_model_to_agents(agents: &mut AgentsConfig, provider: &str, model: &str) {
    let default_model = if provider == "anthropic" {
        "claude-sonnet-4-20250514"
    } else {
        "gpt-4o-mini"
    };
    for a in [
        &mut agents.planner,
        &mut agents.coder,
        &mut agents.tester,
        &mut agents.reviewer,
        &mut agents.synthesizer,
        &mut agents.security_auditor,
    ] {
        if a.provider == provider && a.model == default_model {
            a.model = model.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_new_agents_and_sections() {
        let c = NikiConfig::default();
        assert!(c.agents.synthesizer.provider.len() > 0);
        assert!(c.agents.security_auditor.provider.len() > 0);
        assert!(!c.security.enabled);
        assert_eq!(c.parallel.coder_count, 2);
        // Red/Blue is on by default — it is the product's core thesis.
        assert!(c.red_blue.enabled);
        assert!(c.agents.red.provider.len() > 0);
    }

    #[test]
    fn toml_round_trips_new_sections() {
        let toml = r#"
[general]
max_revision_rounds = 5

[security]
enabled = true

[parallel]
enabled = true
coder_count = 4

[red_blue]
enabled = true

[agents.security_auditor]
provider = "anthropic"
model = "claude-opus-4"

[agents.red]
provider = "anthropic"
model = "claude-opus-4"
"#;
        let c: NikiConfig = crate::config::types::toml::from_str(toml).unwrap();
        assert!(c.security.enabled);
        assert!(c.parallel.enabled);
        assert_eq!(c.parallel.coder_count, 4);
        assert!(c.red_blue.enabled);
        assert_eq!(c.agents.security_auditor.model, "claude-opus-4");
        assert_eq!(c.agents.red.model, "claude-opus-4");
    }

    #[test]
    fn merge_toggles_override() {
        let mut base = NikiConfig::default();
        let ov: NikiConfig = toml::from_str(
            "[security]\nenabled = true\n[parallel]\nenabled = true\ncoder_count = 3\n[red_blue]\nenabled = true\n",
        )
        .unwrap();
        base.merge(ov);
        assert!(base.security.enabled);
        assert!(base.parallel.enabled);
        assert_eq!(base.parallel.coder_count, 3);
        assert!(base.red_blue.enabled);
    }

    #[test]
    fn default_security_config_has_per_role_policies() {
        let c = NikiConfig::default();
        assert!(c.security.policies.contains_key("tester"));
        assert!(c.security.policies.contains_key("coder"));
        assert!(c.security.policies.contains_key("reviewer"));
    }

    #[test]
    fn tester_policy_denies_write_commands() {
        let policy = default_tester_policy();
        // Should deny git push, git commit, rm, etc.
        assert!(policy.denied_commands.contains(&"git push".to_string()));
        assert!(policy.denied_commands.contains(&"git commit".to_string()));
        assert!(policy.denied_commands.contains(&"rm".to_string()));
        // Should allow read-only and test commands
        assert!(policy.allowed_commands.contains(&"cargo test".to_string()));
        assert!(policy.allowed_commands.contains(&"git diff".to_string()));
    }

    #[test]
    fn coder_policy_allows_write_but_denies_push() {
        let policy = default_coder_policy();
        // Should allow write commands
        assert!(policy.allowed_commands.contains(&"git add".to_string()));
        assert!(policy.allowed_commands.contains(&"git commit".to_string()));
        assert!(policy.allowed_commands.contains(&"rm".to_string()));
        // Should deny push
        assert!(policy.denied_commands.contains(&"git push".to_string()));
    }

    #[test]
    fn reviewer_policy_is_read_only() {
        let policy = default_reviewer_policy();
        // Should deny all write commands
        assert!(policy.denied_commands.contains(&"git push".to_string()));
        assert!(policy.denied_commands.contains(&"git commit".to_string()));
        assert!(policy.denied_commands.contains(&"rm".to_string()));
        assert!(policy.denied_commands.contains(&"mv".to_string()));
        // Should allow read-only commands
        assert!(policy.allowed_commands.contains(&"git diff".to_string()));
        assert!(policy.allowed_commands.contains(&"git log".to_string()));
        assert!(policy.allowed_commands.contains(&"git show".to_string()));
    }

    #[test]
    fn security_config_toml_round_trip() {
        let toml = r#"
[security]
enabled = true

[security.policies.tester]
allowed_commands = ["cargo test", "git diff"]
denied_commands = ["git push", "rm"]
max_exec_seconds = 600
"#;
        let c: NikiConfig = toml::from_str(toml).unwrap();
        assert!(c.security.enabled);
        let tester = c.security.policies.get("tester").unwrap();
        assert_eq!(tester.max_exec_seconds, 600);
        assert_eq!(tester.allowed_commands.len(), 2);
        assert_eq!(tester.denied_commands.len(), 2);
    }
}
