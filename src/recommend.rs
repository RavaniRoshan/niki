//! Per-agent model recommendations (#10).
//!
//! NIKI runs a chain of specialized agents, each with a different cost/quality
//! tradeoff. This module encodes a curated `(strong, cheap)` model pairing per
//! role plus the reasoning, and turns it into a human-readable recommendation
//! with a per-run cost estimate from the [`crate::cost`] price table.

use crate::artifacts::types::AgentRole;
use crate::cost::lookup_price;
use serde::Deserialize;

/// A curated recommendation for one pipeline role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        RoleRec {
            role: AgentRole::Red,
            strong: ("anthropic", "claude-opus-4"),
            cheap: ("anthropic", "claude-sonnet-4-20250514"),
            rationale: "The Red agent's job is to find what stronger models miss; it defaults to strong.",
        },
        RoleRec {
            role: AgentRole::Critic,
            strong: ("anthropic", "claude-opus-4"),
            cheap: ("anthropic", "claude-sonnet-4-20250514"),
            rationale: "Verdict-grounding is mechanical; sonnet suffices, opus for high-stakes runs.",
        },
    ]
}

/// Whether a provider actually offers the model we are about to recommend.
///
/// The table above is a hardcoded opinion about models that existed when it was
/// written. On a provider that fronts hundreds of models under their own names
/// — OpenRouter — it is confidently wrong more often than right, and a
/// recommendation a user cannot run is worse than none, because it reads as
/// authoritative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// The catalogue contains it.
    Offered,
    /// The catalogue was read and does not contain it.
    NotOffered,
    /// No catalogue: the provider has no endpoint, the key could not read one,
    /// or the user has not configured one. Not a claim either way.
    Unknown,
}

/// Match a recommended model against a provider's catalogue.
///
/// `None` for the catalogue means `Unknown` — the distinction that matters. A
/// provider without a `/models` endpoint is normal and says nothing about
/// whether the model exists.
pub fn availability(
    catalogue: Option<&[crate::cli::catalogue::CatalogueEntry]>,
    model: &str,
) -> Availability {
    let Some(entries) = catalogue else {
        return Availability::Unknown;
    };
    // Exact id, or a bare id matching a vendor-qualified one. OpenRouter calls
    // the same model `anthropic/claude-sonnet-4` where our table says
    // `claude-sonnet-4`, and neither spelling is a different model.
    let offered = entries.iter().any(|e| {
        e.id == model
            || e.id.rsplit('/').next() == Some(model)
            || model.rsplit('/').next() == Some(e.id.as_str())
    });
    if offered {
        Availability::Offered
    } else {
        Availability::NotOffered
    }
}

/// The closest things a provider *does* offer, for a model we cannot find.
///
/// Families first, then anything sharing a word, capped small. This is a
/// suggestion shown to a person, so being wrong costs a glance; it is never
/// used to pick a model on their behalf.
pub fn suggestions(
    catalogue: Option<&[crate::cli::catalogue::CatalogueEntry]>,
    model: &str,
    limit: usize,
) -> Vec<String> {
    let Some(entries) = catalogue else {
        return Vec::new();
    };
    let needle = model.to_ascii_lowercase();
    let family = needle
        .split(|c: char| c == '-' || c == '_' || c.is_ascii_digit())
        .find(|w| w.len() > 3)
        .unwrap_or("");
    let mut scored: Vec<(usize, &str)> = entries
        .iter()
        .map(|e| {
            let id = e.id.to_ascii_lowercase();
            let score = if id == needle {
                0
            } else if !family.is_empty() && id.contains(family) {
                1
            } else {
                2
            };
            (score, e.id.as_str())
        })
        .filter(|(s, _)| *s < 2)
        .collect();
    scored.sort_by_key(|(s, id)| (*s, id.len()));
    scored
        .into_iter()
        .take(limit)
        .map(|(_, id)| id.to_string())
        .collect()
}

/// Whether a role defaults to the *strong* model under a `balanced` preference.
/// Quality-critical gates (Reviewer, SecurityAuditor) lean strong; mechanical
/// roles (Tester, Synthesizer) lean cheap.
pub fn role_prefers_strong(role: AgentRole) -> bool {
    matches!(
        role,
        AgentRole::Reviewer
            | AgentRole::SecurityAuditor
            | AgentRole::Red
            | AgentRole::Planner
            | AgentRole::Coder
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

/// One stage metric as persisted in `.niki/tasks/*/task.json`. A minimal view
/// (not the full `TaskRecord`) so history reads tolerate schema drift.
#[derive(Debug, Deserialize)]
struct HistoryMetric {
    role: AgentRole,
    provider: String,
    model: String,
    #[serde(default)]
    cost_usd: f64,
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct HistoryRecord {
    #[serde(default)]
    agent_metrics: Vec<HistoryMetric>,
}

/// Observed per-(role, provider, model) spend aggregated from past runs.
#[derive(Debug, Clone)]
pub struct ObservedSpend {
    pub role: AgentRole,
    pub provider: String,
    pub model: String,
    pub runs: usize,
    pub avg_cost_usd: f64,
    pub avg_input_tokens: f64,
    pub avg_output_tokens: f64,
}

/// Aggregate `agent_metrics` from every `.niki/tasks/*/task.json` under
/// `tasks_dir`. Unreadable files are skipped — history is advisory, never fatal.
pub fn observed_spend(tasks_dir: &std::path::Path) -> Vec<ObservedSpend> {
    use std::collections::HashMap;

    type SpendKey = (AgentRole, String, String);
    type SpendAcc = (usize, f64, u64, u64); // runs, cost sum, in-tok sum, out-tok sum
    let mut acc: HashMap<SpendKey, SpendAcc> = HashMap::new();
    let entries = std::fs::read_dir(tasks_dir).map(|rd| {
        rd.filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect::<Vec<_>>()
    });
    for entry in entries.into_iter().flatten() {
        let record_path = entry.path().join("task.json");
        let Ok(content) = std::fs::read_to_string(&record_path) else {
            continue;
        };
        let Ok(record) = serde_json::from_str::<HistoryRecord>(&content) else {
            continue;
        };
        for m in &record.agent_metrics {
            let e = acc
                .entry((m.role, m.provider.clone(), m.model.clone()))
                .or_insert((0, 0.0, 0, 0));
            e.0 += 1;
            e.1 += m.cost_usd;
            e.2 += m.input_tokens as u64;
            e.3 += m.output_tokens as u64;
        }
    }

    let mut out: Vec<ObservedSpend> = acc
        .into_iter()
        .map(
            |((role, provider, model), (runs, cost, inp, outp))| ObservedSpend {
                role,
                provider,
                model,
                runs,
                avg_cost_usd: cost / runs as f64,
                avg_input_tokens: inp as f64 / runs as f64,
                avg_output_tokens: outp as f64 / runs as f64,
            },
        )
        .collect();
    out.sort_by(|a, b| {
        (a.role as u8)
            .cmp(&(b.role as u8))
            .then(b.runs.cmp(&a.runs))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_has_a_recommendation() {
        use crate::artifacts::types::AgentRole::*;
        let present: Vec<AgentRole> = recommendations().iter().map(|r| r.role).collect();
        for role in [
            Planner,
            Coder,
            Tester,
            Reviewer,
            Synthesizer,
            SecurityAuditor,
        ] {
            assert!(
                present.contains(&role),
                "missing recommendation for {:?}",
                role
            );
        }
    }

    use crate::cli::catalogue::CatalogueEntry;

    fn entry(id: &str) -> CatalogueEntry {
        CatalogueEntry {
            id: id.to_string(),
            price_per_mtok: None,
            traits: Vec::new(),
        }
    }

    /// The table is an opinion about models that existed when it was written.
    /// On a provider that fronts hundreds under its own names it is wrong more
    /// often than right, and a recommendation a user cannot run reads as
    /// authoritative.
    #[test]
    fn a_recommendation_the_provider_does_not_offer_is_said_so() {
        let catalogue = vec![entry("anthropic/claude-sonnet-4"), entry("openai/o3-mini")];
        assert_eq!(
            availability(Some(&catalogue), "claude-sonnet-4"),
            Availability::Offered,
            "a vendor-qualified id is the same model as the bare name our table uses"
        );
        assert_eq!(
            availability(Some(&catalogue), "claude-opus-4"),
            Availability::NotOffered,
            "naming a model this account cannot run must be visible"
        );
    }

    /// No catalogue is not a claim that the model is missing. Several
    /// providers have no `/models` endpoint, and a key that cannot read one
    /// says nothing about what exists.
    #[test]
    fn no_catalogue_means_unknown_not_missing() {
        assert_eq!(availability(None, "claude-opus-4"), Availability::Unknown);
    }

    /// The suggestion is for a person to read, so it errs toward the family
    /// and is capped — it is never used to pick on the user's behalf.
    #[test]
    fn a_missing_model_offers_real_alternatives() {
        let catalogue = vec![
            entry("anthropic/claude-sonnet-4"),
            entry("anthropic/claude-haiku-4-5"),
            entry("openai/o3-mini"),
            entry("meta-llama/llama-3.3-70b-instruct"),
        ];
        let s = suggestions(Some(&catalogue), "claude-opus-4", 3);
        assert!(
            s.iter().any(|x| x.contains("claude")),
            "the same family should surface first: {s:?}"
        );
        assert!(
            !s.iter().any(|x| x.contains("llama")),
            "and unrelated models should not be dressed up as substitutes: {s:?}"
        );
        assert_eq!(suggestions(None, "claude-opus-4", 3).len(), 0);
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
        let c = estimate_cost(
            "anthropic",
            "claude-sonnet-4-20250514",
            1_000_000,
            1_000_000,
        );
        assert!(c > 0.0);
    }
}
