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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
            base_image: "ubuntu:24.04".to_string(),
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

        // Prefer live gateway credentials (ANTHROPIC_AUTH_TOKEN / OPENROUTER_API_KEY)
        // over any key hardcoded in niki.toml, which may be stale/revoked.
        if let Some(p) = self.providers.get_mut("anthropic") {
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

        if let Ok(key) = std::env::var("NIKI_PROVIDERS_ANTHROPIC_API_KEY") {
            if let Some(p) = self.providers.get_mut("anthropic") {
                if p.api_key.is_none() {
                    p.api_key = Some(key);
                }
            }
        }
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            if let Some(p) = self.providers.get_mut("anthropic") {
                if p.api_key.is_none() {
                    p.api_key = Some(key);
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
    }
}
