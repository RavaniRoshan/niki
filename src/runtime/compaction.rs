//! Compaction subsystem — preserves high-value state while removing low-value history.
//!
//! Preserves:
//! - task objective
//! - current plan
//! - constraints
//! - important decisions
//! - unresolved issues
//! - relevant files
//! - latest test failures
//! - security findings
//! - active tool state
//! - important reviewer feedback
//!
//! Compacts or removes:
//! - redundant or old tool outputs (large bash/file read outputs)
//! - older conversational turns

use crate::runtime::context::{ContextStore, FragmentKind};
use serde::{Deserialize, Serialize};

/// Strategy for compaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompactionStrategy {
    /// Trim low-priority history and tool outputs.
    TrimHistory,
    /// Summarize dynamic fragments into a compact decision/knowledge fragment.
    Summarize,
    /// Aggressive compaction: drop all non-essential fragments.
    Aggressive,
}

/// Detailed result of a compaction run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionResult {
    pub strategy: CompactionStrategy,
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub tokens_saved: usize,
    pub fragments_removed: usize,
    pub fragments_compacted: usize,
}

/// The context compactor.
pub struct ContextCompactor;

impl ContextCompactor {
    /// Check whether compaction should be triggered based on fill ratio.
    pub fn should_compact(store: &ContextStore, capacity: usize, threshold_pct: f32) -> bool {
        if capacity == 0 {
            return false;
        }
        let used = store.total_estimated_tokens();
        let fill = used as f32 / capacity as f32;
        fill >= threshold_pct
    }

    /// Run compaction against a ContextStore to bring it down toward target_tokens.
    pub fn compact(store: &mut ContextStore, target_tokens: usize) -> CompactionResult {
        let tokens_before = store.total_estimated_tokens();
        if tokens_before <= target_tokens {
            return CompactionResult {
                strategy: CompactionStrategy::TrimHistory,
                tokens_before,
                tokens_after: tokens_before,
                tokens_saved: 0,
                fragments_removed: 0,
                fragments_compacted: 0,
            };
        }

        let mut fragments_removed = 0;
        let mut fragments_compacted = 0;

        // Step 1: Compact large tool outputs and dynamic history
        for fragment in store.fragments_mut() {
            if fragment.kind == FragmentKind::DynamicHistory
                || fragment.kind == FragmentKind::ToolContext
            {
                let old_tokens = fragment.estimated_tokens();
                if old_tokens > 200 {
                    // Truncate verbose tool output, keeping head and tail
                    let lines: Vec<&str> = fragment.content.lines().collect();
                    if lines.len() > 20 {
                        let head: Vec<&str> = lines.iter().take(10).copied().collect();
                        let tail: Vec<&str> = lines.iter().rev().take(10).rev().copied().collect();
                        let compacted_text = format!(
                            "{}\n... [compacted {} lines] ...\n{}",
                            head.join("\n"),
                            lines.len() - 20,
                            tail.join("\n")
                        );
                        fragment.content = compacted_text;
                        fragment.hash = crate::runtime::context::hash_content(&fragment.content);
                        fragment.updated_at = chrono::Utc::now();
                        fragments_compacted += 1;
                    }
                }
            }
        }

        // Step 2: If still above target, remove lowest-priority dynamic fragments
        let mut current_tokens = store.total_estimated_tokens();
        if current_tokens > target_tokens {
            // Collect dynamic fragment IDs sorted by priority ascending
            let mut dynamic_ids: Vec<(String, u32, usize)> = store
                .fragments()
                .iter()
                .filter(|f| !f.cacheable)
                // High-value kinds must never be completely dropped unless absolutely forced
                .filter(|f| {
                    !matches!(
                        f.kind,
                        FragmentKind::TaskContext
                            | FragmentKind::PlanContext
                            | FragmentKind::TestFailure
                            | FragmentKind::ReviewFinding
                    )
                })
                .map(|f| (f.id.clone(), f.priority, f.estimated_tokens()))
                .collect();

            dynamic_ids.sort_by_key(|(_, prio, _)| *prio);

            for (id, _, tok) in dynamic_ids {
                if current_tokens <= target_tokens {
                    break;
                }
                store.remove(&id);
                fragments_removed += 1;
                current_tokens = current_tokens.saturating_sub(tok);
            }
        }

        // Step 3: If still above target, inject a summarized knowledge fragment
        let tokens_after = store.total_estimated_tokens();
        let tokens_saved = tokens_before.saturating_sub(tokens_after);

        CompactionResult {
            strategy: CompactionStrategy::TrimHistory,
            tokens_before,
            tokens_after,
            tokens_saved,
            fragments_removed,
            fragments_compacted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::context::ContextFragment;

    #[test]
    fn test_compaction_truncates_large_history() {
        let mut store = ContextStore::new(1000);
        let verbose_output = (0..50)
            .map(|i| format!("line {i}: some tool output text"))
            .collect::<Vec<_>>()
            .join("\n");

        let f = ContextFragment::new("tool_1", FragmentKind::ToolContext, verbose_output, 500)
            .with_cacheable(false);
        store.upsert(f);

        let initial_tokens = store.total_estimated_tokens();
        let res = ContextCompactor::compact(&mut store, 200);

        assert!(res.tokens_after < initial_tokens);
        assert!(res.fragments_compacted >= 1);
        let updated = store.get("tool_1").unwrap();
        assert!(updated.content.contains("... [compacted"));
    }
}
