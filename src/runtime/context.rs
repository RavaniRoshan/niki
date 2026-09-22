//! ContextStore — bounded, incremental, priority-aware context management.
//!
//! Separates static (cacheable prefix) from dynamic context, imposes hard token
//! budgets, suppresses duplicate injections, and tracks what was sent to the model.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Category of context fragment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FragmentKind {
    SystemInstructions,
    ProjectInstructions,
    RepositoryContext,
    TaskContext,
    PlanContext,
    RelevantFiles,
    ToolContext,
    PreviousDecision,
    TestFailure,
    ReviewFinding,
    Knowledge,
    UserInput,
    ArtifactSummary,
    DynamicHistory,
}

impl FragmentKind {
    /// Whether this fragment kind belongs to the stable static prefix by default.
    pub fn is_static_default(&self) -> bool {
        matches!(
            self,
            FragmentKind::SystemInstructions
                | FragmentKind::ProjectInstructions
                | FragmentKind::RepositoryContext
                | FragmentKind::ToolContext
        )
    }

    /// Default priority for this fragment kind (higher = preserved longer).
    pub fn default_priority(&self) -> u32 {
        match self {
            FragmentKind::SystemInstructions => 100,
            FragmentKind::TaskContext => 95,
            FragmentKind::PlanContext => 90,
            FragmentKind::ProjectInstructions => 85,
            FragmentKind::TestFailure => 80,
            FragmentKind::ReviewFinding => 75,
            FragmentKind::RelevantFiles => 70,
            FragmentKind::ArtifactSummary => 65,
            FragmentKind::PreviousDecision => 60,
            FragmentKind::RepositoryContext => 50,
            FragmentKind::Knowledge => 40,
            FragmentKind::ToolContext => 35,
            FragmentKind::UserInput => 30,
            FragmentKind::DynamicHistory => 20,
        }
    }
}

/// Compute a fast 64-bit hash of text content.
pub fn hash_content(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Estimate tokens using standard character/word heuristic (approx 4 chars per token).
pub fn estimate_tokens(text: &str) -> usize {
    (text.len() / 4).max(1)
}

/// A bounded fragment of context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextFragment {
    pub id: String,
    pub kind: FragmentKind,
    pub priority: u32,
    pub token_budget: usize,
    pub content: String,
    pub cacheable: bool,
    pub hash: u64,
    pub updated_at: DateTime<Utc>,
}

impl ContextFragment {
    pub fn new(
        id: impl Into<String>,
        kind: FragmentKind,
        content: impl Into<String>,
        token_budget: usize,
    ) -> Self {
        let text = content.into();
        let hash = hash_content(&text);
        let cacheable = kind.is_static_default();
        let priority = kind.default_priority();

        Self {
            id: id.into(),
            kind,
            priority,
            token_budget,
            content: text,
            cacheable,
            hash,
            updated_at: Utc::now(),
        }
    }

    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_cacheable(mut self, cacheable: bool) -> Self {
        self.cacheable = cacheable;
        self
    }

    /// Truncate content to fit within token budget if needed.
    pub fn enforce_budget(&mut self) {
        let current_tokens = estimate_tokens(&self.content);
        if current_tokens > self.token_budget && self.token_budget > 0 {
            let max_chars = self.token_budget * 4;
            if self.content.len() > max_chars {
                self.content = self.content.chars().take(max_chars).collect();
                self.content.push_str("\n...[truncated to budget]");
                self.hash = hash_content(&self.content);
                self.updated_at = Utc::now();
            }
        }
    }

    pub fn estimated_tokens(&self) -> usize {
        estimate_tokens(&self.content)
    }
}

/// Summary of what was assembled from the context store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssembledContext {
    pub system_prompt: String,
    pub user_prompt: String,
    pub total_tokens: usize,
    pub cacheable_tokens: usize,
    pub dynamic_tokens: usize,
    pub fragment_ids: Vec<String>,
}

/// The store managing all static and dynamic context fragments for an agent session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextStore {
    max_tokens: usize,
    fragments: Vec<ContextFragment>,
}

impl ContextStore {
    /// Create a new ContextStore with a hard token limit.
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens: if max_tokens == 0 { 128_000 } else { max_tokens },
            fragments: Vec::new(),
        }
    }

    pub fn max_tokens(&self) -> usize {
        self.max_tokens
    }

    pub fn set_max_tokens(&mut self, max: usize) {
        self.max_tokens = max;
    }

    /// Insert or update a context fragment.
    /// If a fragment with the same ID exists:
    /// - If content hash is identical, it's a no-op (duplicate suppression).
    /// - If content changed, it is updated in-place.
    pub fn upsert(&mut self, mut fragment: ContextFragment) -> bool {
        fragment.enforce_budget();
        if let Some(existing) = self.fragments.iter_mut().find(|f| f.id == fragment.id) {
            if existing.hash == fragment.hash {
                return false; // suppressed duplicate
            }
            *existing = fragment;
            true
        } else {
            self.fragments.push(fragment);
            true
        }
    }

    /// Remove a fragment by ID.
    pub fn remove(&mut self, id: &str) -> bool {
        let initial_len = self.fragments.len();
        self.fragments.retain(|f| f.id != id);
        self.fragments.len() < initial_len
    }

    /// Get a fragment by ID.
    pub fn get(&self, id: &str) -> Option<&ContextFragment> {
        self.fragments.iter().find(|f| f.id == id)
    }

    /// All fragments.
    pub fn fragments(&self) -> &[ContextFragment] {
        &self.fragments
    }

    /// Mutable fragments.
    pub fn fragments_mut(&mut self) -> &mut Vec<ContextFragment> {
        &mut self.fragments
    }

    /// Current total token estimate across all fragments.
    pub fn total_estimated_tokens(&self) -> usize {
        self.fragments.iter().map(|f| f.estimated_tokens()).sum()
    }

    /// Assemble the complete context for an inference step.
    /// Preserves stable prefixes for prompt caching:
    /// 1. Cacheable static fragments are assembled into `system_prompt` in deterministic order.
    /// 2. Dynamic fragments are sorted by priority and assembled into `user_prompt`
    ///    fitting strictly within `max_tokens`.
    pub fn assemble(&self) -> AssembledContext {
        let mut static_frags: Vec<&ContextFragment> =
            self.fragments.iter().filter(|f| f.cacheable).collect();
        let mut dynamic_frags: Vec<&ContextFragment> =
            self.fragments.iter().filter(|f| !f.cacheable).collect();

        // Stable order for static fragments
        static_frags.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.id.cmp(&b.id)));

        let mut system_prompt = String::new();
        let mut cacheable_tokens = 0;
        let mut included_ids = Vec::new();

        for f in static_frags {
            if !system_prompt.is_empty() {
                system_prompt.push_str("\n\n");
            }
            system_prompt.push_str(&f.content);
            cacheable_tokens += f.estimated_tokens();
            included_ids.push(f.id.clone());
        }

        // Dynamic fragments sorted by priority descending
        dynamic_frags.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.id.cmp(&b.id)));

        let remaining_budget = self.max_tokens.saturating_sub(cacheable_tokens);
        let mut user_prompt = String::new();
        let mut dynamic_tokens = 0;

        for f in dynamic_frags {
            let f_tokens = f.estimated_tokens();
            if dynamic_tokens + f_tokens > remaining_budget && !included_ids.is_empty() {
                // Budget pressure: skip lower priority fragment
                continue;
            }
            if !user_prompt.is_empty() {
                user_prompt.push_str("\n\n");
            }
            user_prompt.push_str(&f.content);
            dynamic_tokens += f_tokens;
            included_ids.push(f.id.clone());
        }

        AssembledContext {
            system_prompt,
            user_prompt,
            total_tokens: cacheable_tokens + dynamic_tokens,
            cacheable_tokens,
            dynamic_tokens,
            fragment_ids: included_ids,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upsert_and_deduplication() {
        let mut store = ContextStore::new(1000);
        let f1 = ContextFragment::new("sys", FragmentKind::SystemInstructions, "Be concise", 100);
        assert!(store.upsert(f1.clone()));
        // Duplicate with same content returns false
        assert!(!store.upsert(f1));

        // Modified content returns true
        let f2 = ContextFragment::new(
            "sys",
            FragmentKind::SystemInstructions,
            "Be very concise",
            100,
        );
        assert!(store.upsert(f2));
        assert_eq!(store.fragments().len(), 1);
    }

    #[test]
    fn test_assemble_priority_and_limits() {
        let mut store = ContextStore::new(50); // very tight budget
        let f_sys = ContextFragment::new(
            "sys",
            FragmentKind::SystemInstructions,
            "system prompt here",
            20,
        );
        let f_high = ContextFragment::new(
            "high",
            FragmentKind::TaskContext,
            "high priority task description",
            20,
        )
        .with_cacheable(false);
        let f_low = ContextFragment::new(
            "low",
            FragmentKind::DynamicHistory,
            "very long low priority conversational history that should definitely be skipped "
                .repeat(10),
            1000,
        )
        .with_cacheable(false);

        store.upsert(f_sys);
        store.upsert(f_high);
        store.upsert(f_low);

        let assembled = store.assemble();
        assert!(assembled.system_prompt.contains("system prompt"));
        assert!(assembled.user_prompt.contains("high priority"));
        assert!(!assembled.user_prompt.contains("conversational history"));
    }
}
