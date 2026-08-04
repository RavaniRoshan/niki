use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::artifacts::types::AgentRole;
use crate::memory::store::load_memory;

/// Tracks token usage against a context window budget.
/// Inspired by Focus (arXiv:2601.07190) and CALMem's budget-aware injection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBudget {
    /// Total context window capacity (in tokens) for the model in use.
    pub capacity: u32,
    /// Current token count consumed (approximate).
    pub used: u32,
    /// Threshold at which the agent should consider compression (e.g., 0.6 = 60%).
    pub early_warning_at: f32,
    /// Threshold at which a session switch is triggered (e.g., 0.8 = 80%).
    pub session_switch_at: f32,
}

impl ContextBudget {
    pub fn new(capacity: u32) -> Self {
        Self {
            capacity,
            used: 0,
            early_warning_at: 0.6,
            session_switch_at: 0.8,
        }
    }

    /// Current fill ratio (0.0 to 1.0).
    pub fn fill_ratio(&self) -> f32 {
        if self.capacity == 0 {
            return 0.0;
        }
        self.used as f32 / self.capacity as f32
    }

    /// Should the agent trigger memory sync / compression now?
    pub fn should_compress(&self) -> bool {
        self.fill_ratio() >= self.early_warning_at
    }

    /// Is a full session switch warranted?
    pub fn needs_session_switch(&self) -> bool {
        self.fill_ratio() >= self.session_switch_at
    }
}

/// Strategy for compressing context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionStrategy {
    /// Drop oldest entries and summarize key learnings.
    Summarize,
    /// Keep only the most recent N entries (sliding window).
    Trim,
    /// Replace with a compressed knowledge block (Focus-style).
    KnowledgeBlock,
}

/// A compressed knowledge block that replaces raw conversation history.
/// Mirrors Focus's "Knowledge" block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressedKnowledge {
    /// A concise summary of key learnings from the session.
    pub summary: String,
    /// Critical facts the agent must remember.
    pub key_facts: Vec<String>,
    /// Decisions made during the session.
    pub decisions: Vec<String>,
    /// Warnings and gotchas the agent encountered.
    pub warnings: Vec<String>,
    /// Token savings achieved (approximate).
    pub tokens_saved: Option<u32>,
}

impl CompressedKnowledge {
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("## Compressed Session Knowledge\n\n");
        if !self.summary.is_empty() {
            out.push_str(&format!("**Summary:** {}\n\n", self.summary));
        }
        if !self.key_facts.is_empty() {
            out.push_str("**Key Facts:**\n");
            for fact in &self.key_facts {
                out.push_str(&format!("- {}\n", fact));
            }
            out.push('\n');
        }
        if !self.decisions.is_empty() {
            out.push_str("**Decisions Made:**\n");
            for d in &self.decisions {
                out.push_str(&format!("- {}\n", d));
            }
            out.push('\n');
        }
        if !self.warnings.is_empty() {
            out.push_str("**Warnings & Gotchas:**\n");
            for w in &self.warnings {
                out.push_str(&format!("- {}\n", w));
            }
            out.push('\n');
        }
        out
    }
}

/// Compress an agent's context. This is the agent-controlled compression hook
/// inspired by Focus (arXiv:2601.07190), where the agent autonomously decides
/// when to consolidate learnings and prune raw history.
///
/// The actual summarization is done by calling an LLM — this function provides
/// the structure and persistence layer. The orchestrator calls this with a
/// pre-computed summary or delegates to the LLM for generation.
///
/// Returns the compressed knowledge block and writes it to disk for reuse.
pub fn compress_context(
    project_dir: &Path,
    agent_role: AgentRole,
    _strategy: CompressionStrategy,
    summary: String,
    key_facts: Vec<String>,
    decisions: Vec<String>,
    warnings: Vec<String>,
    tokens_saved: Option<u32>,
) -> Result<CompressedKnowledge> {
    let knowledge = CompressedKnowledge {
        summary,
        key_facts,
        decisions,
        warnings,
        tokens_saved,
    };

    // Persist the compressed knowledge so subsequent agent stages can reuse it.
    let dir = project_dir.join(".niki").join("memory").join("compressed");
    std::fs::create_dir_all(&dir)?;
    let path = compression_path(&dir, agent_role);
    let json = serde_json::to_string_pretty(&knowledge)?;
    std::fs::write(&path, json)?;

    Ok(knowledge)
}

/// Load compressed knowledge for a role, if it exists.
pub fn load_compressed_knowledge(project_dir: &Path, agent_role: AgentRole) -> Option<CompressedKnowledge> {
    let dir = project_dir.join(".niki").join("memory").join("compressed");
    let path = compression_path(&dir, agent_role);
    if !path.exists() {
        return None;
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

/// Render memory for prompt, but with budget-aware trimming.
/// Instead of always injecting the last N entries, this respects the
/// context budget: if the budget is tight, inject fewer entries or
/// use compressed knowledge instead.
pub fn render_memory_with_budget(
    project_dir: &Path,
    agent_role: AgentRole,
    budget: &ContextBudget,
) -> String {
    // If budget is critically low, return compressed knowledge only
    if budget.fill_ratio() >= budget.session_switch_at {
        if let Some(ck) = load_compressed_knowledge(project_dir, agent_role) {
            return ck.render();
        }
    }

    // If budget is moderately full, use early-warning mode: fewer entries
    let max_entries = if budget.should_compress() {
        5
    } else {
        10
    };

    let memory = load_memory(project_dir, agent_role);
    if memory.entries.is_empty() {
        return String::new();
    }

    let mut output = String::from("## Project Memory (Learned from previous runs)\n\n");
    for entry in memory.entries.iter().rev().take(max_entries) {
        // Truncate content more aggressively when budget is tight
        let max_content = if budget.should_compress() { 200 } else { 500 };
        output.push_str(&format!(
            "- [{}] {}\n  Tags: {}\n  {}\n\n",
            &entry.timestamp[..10],
            entry.task.chars().take(100).collect::<String>(),
            entry.tags.join(", "),
            entry.content.chars().take(max_content).collect::<String>(),
        ));
    }
    output
}

fn compression_path(dir: &Path, role: AgentRole) -> PathBuf {
    let role_name = match role {
        AgentRole::Planner => "planner",
        AgentRole::Coder => "coder",
        AgentRole::Tester => "tester",
        AgentRole::Reviewer => "reviewer",
        AgentRole::Synthesizer => "synthesizer",
        AgentRole::SecurityAuditor => "security_auditor",
        AgentRole::Red => "red",
    };
    dir.join(format!("{}.json", role_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifacts::types::AgentRole;
    use tempfile::TempDir;

    #[test]
    fn context_budget_thresholds() {
        let mut budget = ContextBudget::new(100000);
        budget.used = 65000; // 65% — past early warning
        assert!(budget.should_compress());
        assert!(!budget.needs_session_switch());

        budget.used = 85000; // 85% — past session switch
        assert!(budget.needs_session_switch());
    }

    #[test]
    fn context_budget_fill_ratio() {
        let budget = ContextBudget::new(100000);
        assert_eq!(budget.fill_ratio(), 0.0);

        let mut budget = ContextBudget::new(100000);
        budget.used = 25000;
        assert!((budget.fill_ratio() - 0.25).abs() < 0.001);
    }

    #[test]
    fn compress_and_load() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        let knowledge = compress_context(
            dir,
            AgentRole::Coder,
            CompressionStrategy::KnowledgeBlock,
            "Fixed the N+1 query bug".to_string(),
            vec!["Use select_related for foreign keys".to_string()],
            vec!["Switched to select_related".to_string()],
            vec!["Avoid raw SQL, use ORM".to_string()],
            Some(12000),
        )
        .unwrap();

        assert!(knowledge.render().contains("Compressed Session Knowledge"));
        assert!(knowledge.render().contains("Fixed the N+1 query bug"));

        let loaded = load_compressed_knowledge(dir, AgentRole::Coder);
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().summary, "Fixed the N+1 query bug");
    }

    #[test]
    fn render_memory_budget_aware_trimming() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // Add some memory entries
        crate::memory::store::append_memory(
            dir,
            AgentRole::Planner,
            "task 1",
            vec!["test".into()],
            "content about task 1 with details".to_string(),
            None,
        )
        .unwrap();

        // With tight budget, should still render but truncated
        let mut budget = ContextBudget::new(100);
        budget.used = 90;

        let rendered = render_memory_with_budget(dir, AgentRole::Planner, &budget);
        assert!(rendered.contains("Project Memory"));
    }

    #[test]
    fn compressed_knowledge_render_structure() {
        let knowledge = CompressedKnowledge {
            summary: "Test summary".to_string(),
            key_facts: vec!["Fact 1".to_string(), "Fact 2".to_string()],
            decisions: vec!["Decision A".to_string()],
            warnings: vec!["Warning X".to_string()],
            tokens_saved: Some(5000),
        };

        let rendered = knowledge.render();
        assert!(rendered.contains("**Summary:** Test summary"));
        assert!(rendered.contains("Key Facts"));
        assert!(rendered.contains("Fact 1"));
        assert!(rendered.contains("Decisions Made"));
        assert!(rendered.contains("Decision A"));
        assert!(rendered.contains("Warnings & Gotchas"));
        assert!(rendered.contains("Warning X"));
    }
}
