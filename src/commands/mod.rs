//! Custom slash commands system.
//!
//! Allows users to define custom commands in config or markdown files
//! that inject templates into the conversation.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// A custom slash command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashCommand {
    pub name: String,
    pub description: String,
    pub template: String,
    pub agent: Option<String>,
    pub model: Option<String>,
}

/// Manages custom slash commands.
pub struct CommandRegistry {
    commands: HashMap<String, SlashCommand>,
}

impl CommandRegistry {
    /// Create a new command registry with built-in commands.
    pub fn new() -> Self {
        let mut registry = Self {
            commands: HashMap::new(),
        };
        registry.register_builtins();
        registry
    }

    /// Register a custom command.
    pub fn register(&mut self, cmd: SlashCommand) {
        self.commands.insert(cmd.name.clone(), cmd);
    }

    /// Get a command by name (without leading /).
    pub fn get(&self, name: &str) -> Option<&SlashCommand> {
        self.commands.get(name)
    }

    /// List all available commands.
    pub fn list(&self) -> Vec<&SlashCommand> {
        let mut cmds: Vec<_> = self.commands.values().collect();
        cmds.sort_by(|a, b| a.name.cmp(&b.name));
        cmds
    }

    /// Load commands from a directory of markdown files.
    pub fn load_from_dir(&mut self, dir: &Path) -> Result<()> {
        if !dir.exists() {
            return Ok(());
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                let content = std::fs::read_to_string(&path)?;
                if let Some(cmd) = Self::parse_command_file(&path, &content) {
                    self.register(cmd);
                }
            }
        }
        Ok(())
    }

    /// Parse a markdown file into a slash command.
    fn parse_command_file(path: &Path, content: &str) -> Option<SlashCommand> {
        let name = path.file_stem()?.to_str()?.to_string();
        let description = format!("Custom command: {}", name);
        Some(SlashCommand {
            name,
            description,
            template: content.to_string(),
            agent: None,
            model: None,
        })
    }

    /// Expand a command with arguments.
    pub fn expand(&self, name: &str, args: &str) -> Option<String> {
        let cmd = self.get(name)?;
        let expanded = cmd.template.replace("$ARGUMENTS", args);
        Some(expanded)
    }

    /// Register built-in commands.
    fn register_builtins(&mut self) {
        let builtins = vec![
            SlashCommand {
                name: "help".to_string(),
                description: "Show help information".to_string(),
                template: "Show all available commands and their usage.".to_string(),
                agent: None,
                model: None,
            },
            SlashCommand {
                name: "compact".to_string(),
                description: "Compact conversation context".to_string(),
                template: "Please summarize the conversation so far to reduce context usage."
                    .to_string(),
                agent: None,
                model: None,
            },
            SlashCommand {
                name: "clear".to_string(),
                description: "Clear conversation".to_string(),
                template: "".to_string(),
                agent: None,
                model: None,
            },
            SlashCommand {
                name: "cost".to_string(),
                description: "Show cost breakdown".to_string(),
                template: "What is the total cost and token usage for this session?".to_string(),
                agent: None,
                model: None,
            },
            SlashCommand {
                name: "model".to_string(),
                description: "Switch or show current model".to_string(),
                template: "".to_string(),
                agent: None,
                model: None,
            },
            SlashCommand {
                name: "sessions".to_string(),
                description: "List saved sessions".to_string(),
                template: "".to_string(),
                agent: None,
                model: None,
            },
            SlashCommand {
                name: "undo".to_string(),
                description: "Undo last change".to_string(),
                template: "".to_string(),
                agent: None,
                model: None,
            },
            SlashCommand {
                name: "redo".to_string(),
                description: "Redo last undone change".to_string(),
                template: "".to_string(),
                agent: None,
                model: None,
            },
            SlashCommand {
                name: "export".to_string(),
                description: "Export conversation to markdown".to_string(),
                template: "".to_string(),
                agent: None,
                model: None,
            },
        ];
        for cmd in builtins {
            self.register(cmd);
        }
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_registry_builtins() {
        let registry = CommandRegistry::new();
        assert!(registry.get("help").is_some());
        assert!(registry.get("compact").is_some());
        assert!(registry.get("clear").is_some());
    }

    #[test]
    fn command_list() {
        let registry = CommandRegistry::new();
        let cmds = registry.list();
        assert!(!cmds.is_empty());
    }

    #[test]
    fn command_expand() {
        let registry = CommandRegistry::new();
        let expanded = registry.expand("help", "");
        assert!(expanded.is_some());
    }

    #[test]
    fn command_register_custom() {
        let mut registry = CommandRegistry::new();
        registry.register(SlashCommand {
            name: "review".to_string(),
            description: "Review code for issues".to_string(),
            template: "Review the following code: $ARGUMENTS".to_string(),
            agent: None,
            model: None,
        });
        let expanded = registry.expand("review", "src/main.rs");
        assert_eq!(expanded.unwrap(), "Review the following code: src/main.rs");
    }
}
