//! Custom slash commands system.
//!
//! Allows users to define custom commands in config or markdown files
//! that inject templates into the conversation.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Palette category for a command — used to group commands in the palette
/// and command menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandCategory {
    /// Context / memory management (compact, cost).
    Context,
    /// Session lifecycle (sessions, undo, redo).
    Session,
    /// File operations (export).
    Files,
    /// Version control.
    Git,
    /// Agent control (steer, spawn).
    Agent,
    /// App / system (help, theme, quit).
    System,
}

impl CommandCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommandCategory::Context => "context",
            CommandCategory::Session => "session",
            CommandCategory::Files => "files",
            CommandCategory::Git => "git",
            CommandCategory::Agent => "agent",
            CommandCategory::System => "system",
        }
    }

    /// All categories, in palette display order.
    pub fn all() -> &'static [CommandCategory] {
        &[
            CommandCategory::Context,
            CommandCategory::Session,
            CommandCategory::Files,
            CommandCategory::Git,
            CommandCategory::Agent,
            CommandCategory::System,
        ]
    }
}

/// A custom slash command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashCommand {
    pub name: String,
    pub description: String,
    pub template: String,
    pub agent: Option<String>,
    pub model: Option<String>,
    /// Optional logical group (e.g. "session", "context") for the command menu.
    pub group: Option<String>,
    /// Aliases that also resolve to this command (e.g. "u" → "undo").
    pub aliases: Vec<String>,
    /// Palette category.
    pub category: CommandCategory,
}

impl SlashCommand {
    /// Builder helper for a basic command (no group/aliases, System category).
    pub fn basic(name: &str, description: &str, template: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            template: template.to_string(),
            agent: None,
            model: None,
            group: None,
            aliases: Vec::new(),
            category: CommandCategory::System,
        }
    }

    /// Builder helper with group + category + aliases.
    pub fn with_meta(
        name: &str,
        description: &str,
        template: &str,
        group: &str,
        category: CommandCategory,
        aliases: &[&str],
    ) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            template: template.to_string(),
            agent: None,
            model: None,
            group: Some(group.to_string()),
            aliases: aliases.iter().map(|a| a.to_string()).collect(),
            category,
        }
    }
}

/// Manages custom slash commands.
pub struct CommandRegistry {
    commands: HashMap<String, SlashCommand>,
    /// Maps alias → canonical command name.
    alias_map: HashMap<String, String>,
}

impl CommandRegistry {
    /// Create a new command registry with built-in commands.
    pub fn new() -> Self {
        let mut registry = Self {
            commands: HashMap::new(),
            alias_map: HashMap::new(),
        };
        registry.register_builtins();
        registry
    }

    /// Register a custom command (also wires up its aliases).
    pub fn register(&mut self, cmd: SlashCommand) {
        for alias in &cmd.aliases {
            self.alias_map.insert(alias.clone(), cmd.name.clone());
        }
        self.commands.insert(cmd.name.clone(), cmd);
    }

    /// Resolve a command name (or alias) to its canonical name.
    pub fn resolve_alias(&self, name: &str) -> String {
        self.alias_map
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_string())
    }

    /// Get a command by name (or alias).
    pub fn get(&self, name: &str) -> Option<&SlashCommand> {
        let canonical = self.resolve_alias(name);
        self.commands.get(&canonical)
    }

    /// List all available commands.
    pub fn list(&self) -> Vec<&SlashCommand> {
        let mut cmds: Vec<_> = self.commands.values().collect();
        cmds.sort_by(|a, b| a.name.cmp(&b.name));
        cmds
    }

    /// All logical groups present in the registry, in insertion-stable order.
    pub fn groups(&self) -> Vec<String> {
        let mut groups: Vec<String> = self
            .commands
            .values()
            .filter_map(|c| c.group.clone())
            .collect();
        groups.sort();
        groups.dedup();
        groups
    }

    /// Commands belonging to a given group.
    pub fn by_group(&self, group: &str) -> Vec<&SlashCommand> {
        self.commands
            .values()
            .filter(|c| c.group.as_deref() == Some(group))
            .collect()
    }

    /// Commands in a given palette category.
    pub fn by_category(&self, category: CommandCategory) -> Vec<&SlashCommand> {
        self.commands
            .values()
            .filter(|c| c.category == category)
            .collect()
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
            group: Some("custom".to_string()),
            aliases: Vec::new(),
            category: CommandCategory::System,
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
            SlashCommand::with_meta(
                "help",
                "Show help information",
                "Show all available commands and their usage.",
                "system",
                CommandCategory::System,
                &["?"],
            ),
            SlashCommand::with_meta(
                "compact",
                "Compact conversation context",
                "Please summarize the conversation so far to reduce context usage.",
                "context",
                CommandCategory::Context,
                &["c"],
            ),
            SlashCommand::with_meta(
                "clear",
                "Clear conversation",
                "",
                "session",
                CommandCategory::Session,
                &["cls"],
            ),
            SlashCommand::with_meta(
                "cost",
                "Show cost breakdown",
                "What is the total cost and token usage for this session?",
                "context",
                CommandCategory::Context,
                &["$"],
            ),
            SlashCommand::with_meta(
                "model",
                "Switch or show current model",
                "",
                "session",
                CommandCategory::Session,
                &["m"],
            ),
            SlashCommand::with_meta(
                "sessions",
                "List saved sessions",
                "",
                "session",
                CommandCategory::Session,
                &[],
            ),
            SlashCommand::with_meta(
                "exit",
                "Exit Niki",
                "Quit the application. Same as pressing Ctrl+C twice.",
                "system",
                CommandCategory::System,
                &["quit", "q"],
            ),
            SlashCommand::with_meta(
                "undo",
                "Undo last change",
                "",
                "session",
                CommandCategory::Session,
                &["u"],
            ),
            SlashCommand::with_meta(
                "redo",
                "Redo last undone change",
                "",
                "session",
                CommandCategory::Session,
                &["r"],
            ),
            SlashCommand::with_meta(
                "rewind",
                "Rewind to previous checkpoint",
                "",
                "session",
                CommandCategory::Session,
                &["rw"],
            ),
            SlashCommand::with_meta(
                "export",
                "Export conversation to markdown",
                "",
                "files",
                CommandCategory::Files,
                &["e"],
            ),
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
            group: Some("agent".to_string()),
            aliases: vec!["rv".to_string()],
            category: CommandCategory::Agent,
        });
        let expanded = registry.expand("review", "src/main.rs");
        assert_eq!(expanded.unwrap(), "Review the following code: src/main.rs");
    }

    #[test]
    fn command_aliases_resolve() {
        let registry = CommandRegistry::new();
        // "u" is the alias for "undo".
        assert_eq!(registry.resolve_alias("u"), "undo");
        assert_eq!(registry.get("u").unwrap().name, "undo");
        // Unknown names resolve to themselves.
        assert_eq!(registry.resolve_alias("nope"), "nope");
    }

    #[test]
    fn command_groups_and_categories() {
        let registry = CommandRegistry::new();
        let groups = registry.groups();
        assert!(groups.contains(&"session".to_string()));
        assert!(groups.contains(&"context".to_string()));
        let session_cmds = registry.by_group("session");
        assert!(session_cmds.iter().any(|c| c.name == "undo"));
        let context_cmds = registry.by_category(CommandCategory::Context);
        assert!(context_cmds.iter().any(|c| c.name == "compact"));
    }

    #[test]
    fn command_category_strings() {
        assert_eq!(CommandCategory::Session.as_str(), "session");
        assert_eq!(CommandCategory::Context.as_str(), "context");
        assert_eq!(CommandCategory::all().len(), 6);
    }
}
