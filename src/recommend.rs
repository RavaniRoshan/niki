//! Per-agent model recommendations (#10).
//!
//! NIKI runs a chain of specialized agents, each with a different cost/quality
//! tradeoff. This module encodes a curated `(strong, cheap)` model pairing per
//! role plus the reasoning, and turns it into a human-readable recommendation
//! with a per-run cost estimate from the [`crate::cost`] price table.

use crate::artifacts::types::AgentRole;
use crate::cost::lookup_price;

/// A curated recommendation for one pipeline role.
pub struct RoleRec {
    pub role: AgentRole,
    /// A high-capability (but pricier) model for when quality matters most.
    pub strong: (&'static str, &'static str), // (provider, model)
    /// A cost-efficient model for when the role is mechanical or the budget is tight.
    pub cheap: (&'static str, &'static str),
    /// One-line explanation of the tradeoff.
    pub rationale: &'static str,
}

/// The curated per-role model pairings. Models are matched by substring in
/// [`crate::cost::lookup_price`], so version suffixes resolve correctly.
pub fn recommendations() -> Vec<RoleRec> {
    vec![
        RoleRec {
            role: AgentRole::Planner,
            strong: ("anthropic", "claude-opus-4"),
            cheap: ("anthropic", "claude-haiku-4-5"),
            rationale: "Planning rewards strong reasoning; haiku is adequate for trivial tasks.",
        },
        RoleRec {
            role: AgentRole::Coder,
            strong: ("anthropic", "claude-sonnet-4-20250514"),
            cheap: ("anthropic", "claude-haiku-4-5"),
            rationale: "Coding needs precise instruction-following; haiku for small edits.",
        },
        RoleRec {
            role: AgentRole::Tester,
            strong: ("openai", "gpt-4o-mini"),
            cheap: ("openai", "gpt-4o-mini"),
            rationale: "Test authoring is well-served by gpt-4o-mini at low cost.",
        },
        RoleRec {
            role: AgentRole::Reviewer,
            strong: ("anthropic", "claude-opus-4"),
            cheap: ("anthropic", "claude-sonnet-4-20250514"),
            rationale: "Critical review wants the strongest model; sonnet is a solid default.",
        },
        RoleRec {
            role: AgentRole::Synthesizer,
            strong: ("anthropic", "claude-sonnet-4-20250514"),
            cheap: ("anthropic", "claude-haiku-4-5"),
            rationale: "Merging diffs is mechanical; sonnet balances cost and correctness.",
        },
        RoleRec {
            role: AgentRole::SecurityAuditor,
            strong: ("anthropic", "claude-opus-4"),
            cheap: ("anthropic", "claude-sonnet-4-20250514"),
            rationale: "Security findings demand the strongest reasoning; sonnet for triage.",
        },
    ]
}

/// Whether a role defaults to the *strong* model under a `balanced` preference.
/// Quality-critical gates (Reviewer, SecurityAuditor) lean strong; mechanical
/// roles (Tester, Synthesizer) lean cheap.
pub fn role_prefers_strong(role: AgentRole) -> bool {
    matches!(
        role,
        AgentRole::Reviewer | AgentRole::SecurityAuditor | AgentRole::Planner | AgentRole::Coder
    )
}

/// Estimate the USD cost of one run for a `(provider, model)` pair given
/// estimated input/output token counts, using the live price table.
pub fn estimate_cost(provider: &str, model: &str, est_in: u32, est_out: u32) -> f64 {
    match lookup_price(provider, model) {
        Some(p) => {
            (est_in as f64 / 1_000_000.0) * p.input_per_million
                + (est_out as f64 / 1_000_000.0) * p.output_per_million
        }
        None => 0.0,
    }
}

/// Rough token estimate for a task description, scaled by whether we are
/// estimating a generation-heavy role. Returns `(est_in, est_out)`.
pub fn estimate_tokens(task: Option<&str>) -> (u32, u32) {
    match task {
        Some(t) => {
            let chars = t.chars().count() as u32;
            // ~4 chars/token in, generous output for code/artifacts.
            ((chars / 4) + 1500, 2500)
        }
        None => (4000, 2500),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_has_a_recommendation() {
        use crate::artifacts::types::AgentRole::*;
        let present: Vec<AgentRole> = recommendations().iter().map(|r| r.role).collect();
        for role in [Planner, Coder, Tester, Reviewer, Synthesizer, SecurityAuditor] {
            assert!(present.contains(&role), "missing recommendation for {:?}", role);
        }
    }

    #[test]
    fn strong_gates_prefer_strong() {
        assert!(role_prefers_strong(AgentRole::Reviewer));
        assert!(role_prefers_strong(AgentRole::SecurityAuditor));
        assert!(!role_prefers_strong(AgentRole::Tester));
    }

    #[test]
    fn estimate_is_priced_or_free() {
        // Unknown/local models price as free.
        assert_eq!(estimate_cost("ollama", "llama3", 1_000_000, 1_000_000), 0.0);
        // A known model returns a positive figure.
        let c = estimate_cost("anthropic", "claude-sonnet-4-20250514", 1_000_000, 1_000_000);
        assert!(c > 0.0);
    }
}
