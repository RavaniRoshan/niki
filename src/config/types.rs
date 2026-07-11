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
