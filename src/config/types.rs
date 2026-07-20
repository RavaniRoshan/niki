use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use toml;
use crate::artifacts::types::{AgentRole, Complexity};
use crate::sandbox::SandboxBackend;

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

/// Configuration for the optional independent security audit pass (#4).
///
/// When `enabled`, the pipeline injects a `SecurityAuditor` stage (driven by
/// `provider`/`model`, defaulting to the configured `security_auditor` agent)
/// after the Reviewer. The audit verdict is recorded as an artifact but does not
/// gate the revision loop by default.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Optional provider override; defaults to `[agents] security_auditor.provider`.
    #[serde(default)]
    pub provider: Option<String>,
    /// Optional model override; defaults to `[agents] security_auditor.model`.
    #[serde(default)]
    pub model: Option<String>,
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

        let global_path = dirs::home_dir()
            .map(|h| h.join(".config/niki/niki.toml"));
        
        let local_path = project_dir.join("niki.toml");

        if let Some(gp) = global_path {
            if gp.exists() {
                let content = fs::read_to_string(&gp)?;
                let c: NikiConfig = toml::from_str(&content)?;
                config.merge(c);
            }
        }

        if local_path.exists() {
            let content = fs::read_to_string(&local_path)?;
            let c: NikiConfig = toml::from_str(&content)?;
            config.merge(c);
        }

        config.apply_env_vars();

        Ok(config)
    }

    fn merge(&mut self, other: NikiConfig) {
        self.general.max_revision_rounds = other.general.max_revision_rounds;
        self.general.output_dir = other.general.output_dir;

        for (k, v) in other.providers {
            self.providers.insert(k, v);
        }

        self.agents = other.agents;
        self.docker = other.docker;

        // Topology overrides are additive: only apply the parts the user set.
        if !other.pipeline.stages.is_empty() {
            self.pipeline.stages = other.pipeline.stages;
        }
        if other.pipeline.max_revision_rounds.is_some() {
            self.pipeline.max_revision_rounds = other.pipeline.max_revision_rounds;
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
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            if !key.is_empty() {
                if let Some(p) = self.providers.get_mut("anthropic") {
                    p.api_key = Some(key);
                }
            }
        }
        if let Ok(key) = std::env::var("NIKI_PROVIDERS_ANTHROPIC_API_KEY") {
            if !key.is_empty() {
                if let Some(p) = self.providers.get_mut("anthropic") {
                    if p.api_key.is_none() {
                        p.api_key = Some(key);
                    }
                }
            }
        }
        if let Some(p) = self.providers.get_mut("anthropic") {
            if p.api_key.is_none() {
                if let Ok(token) = std::env::var("ANTHROPIC_AUTH_TOKEN") {
                    if !token.is_empty() {
                        p.api_key = Some(token);
                    }
                } else if let Ok(key) = std::env::var("OPENROUTER_API_KEY") {
                    if !key.is_empty() {
                        p.api_key = Some(key);
                    }
                }
            }
        }
        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            if let Some(p) = self.providers.get_mut("openai") {
                if p.api_key.is_none() {
                    p.api_key = Some(key);
                }
            }
        }
        if let Ok(key) = std::env::var("GOOGLE_API_KEY") {
            if let Some(p) = self.providers.get_mut("google") {
                if p.api_key.is_none() {
                    p.api_key = Some(key);
                }
            }
        }

        // Standard base-URL overrides (SDK convention: a host/base, not the full
        // endpoint — the provider appends the path). Env takes precedence over
        // whatever is in niki.toml.
        if let Ok(base) = std::env::var("ANTHROPIC_BASE_URL") {
            if !base.is_empty() {
                if let Some(p) = self.providers.get_mut("anthropic") {
                    p.base_url = Some(base.trim_end_matches('/').to_string());
                }
            }
        }
        if let Ok(base) = std::env::var("OPENAI_BASE_URL") {
            if !base.is_empty() {
                if let Some(p) = self.providers.get_mut("openai") {
                    p.base_url = Some(base.trim_end_matches('/').to_string());
                }
            }
        }

        // Standard model overrides. Applied to agents still using the provider's
        // built-in default, so an explicit per-agent model in niki.toml is respected.
        if let Ok(model) = std::env::var("ANTHROPIC_MODEL") {
            if !model.is_empty() {
                if let Some(p) = self.providers.get_mut("anthropic") {
                    p.default_model = model.clone();
                }
                apply_env_model_to_agents(&mut self.agents, "anthropic", &model);
            }
        }
        if let Ok(model) = std::env::var("OPENAI_MODEL") {
            if !model.is_empty() {
                if let Some(p) = self.providers.get_mut("openai") {
                    p.default_model = model.clone();
                }
                apply_env_model_to_agents(&mut self.agents, "openai", &model);
            }
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
}
