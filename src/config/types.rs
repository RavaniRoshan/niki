use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use toml;
use crate::artifacts::types::AgentRole;

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

fn default_max_source_chars() -> usize {
    8000
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
}

impl Default for AgentsConfig {
    fn default() -> Self {
        Self {
            planner: default_anthropic_agent(),
            coder: default_anthropic_agent(),
            tester: default_openai_agent(),
            reviewer: default_anthropic_agent(),
        }
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
}

impl Default for DockerConfig {
    fn default() -> Self {
        Self {
            base_image: "niki-sandbox:24.04".to_string(),
            extra_packages: vec!["nodejs".into(), "npm".into(), "python3".into()],
            memory_limit: "2g".to_string(),
            cpu_limit: 2.0,
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
    ] {
        if a.provider == provider && a.model == default_model {
            a.model = model.to_string();
        }
    }
}
