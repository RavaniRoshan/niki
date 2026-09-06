//! Cost & performance accounting for NIKI runs.
//!
//! Token counts come from the LLM providers' own usage reports (see
//! [`crate::llm::provider::StreamChunk::Usage`]); this module turns those into a
//! USD cost using a best-effort price table.
//!
//! Honesty rules (Phase 1, goal-a3f9c2):
//! - Unknown models price as `0.0` **with a loud warning** (see [`compute_cost`])
//!   so a run still completes but nobody mistakes the total for complete.
//! - Cached input tokens price at 10% of the input rate; reasoning tokens price
//!   at the output rate. These ratios are documented approximations, not vendor
//!   quotes — update them alongside the table.
//! - [`PRICE_TABLE_AS_OF`] stamps the table's freshness; a unit test fails when
//!   the table goes stale so refreshes can't be silently skipped.

use crate::llm::provider::TokenUsage;

/// ISO-8601 date the price table below was last verified against vendor pages.
/// Bump this whenever rates are refreshed. `price_table_is_fresh` fails the
/// build when the table is older than [`PRICE_TABLE_MAX_AGE_DAYS`].
pub const PRICE_TABLE_AS_OF: &str = "2026-09-06";

/// Maximum age of the price table before `price_table_is_fresh` fails.
pub const PRICE_TABLE_MAX_AGE_DAYS: i64 = 180;

/// USD price per 1,000,000 tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelPrice {
    pub input_per_million: f64,
    pub output_per_million: f64,
}

impl ModelPrice {
    fn cost(&self, usage: &TokenUsage) -> f64 {
        (usage.input_tokens as f64 / 1_000_000.0) * self.input_per_million
            + (usage.output_tokens as f64 / 1_000_000.0) * self.output_per_million
            // Cache hits bill well below list input rates across vendors; 10%
            // is the documented approximation (see module docs).
            + (usage.cached_input_tokens as f64 / 1_000_000.0) * self.input_per_million * 0.1
            // Reasoning/thinking tokens bill at output rates.
            + (usage.reasoning_tokens as f64 / 1_000_000.0) * self.output_per_million
    }
}

/// Returns the price for a `(provider, model)` pair if we recognize the model.
///
/// Matching is by case-insensitive substring over the model id, so version
/// suffixes (`claude-sonnet-4-20250514`) and provider-specific prefixes still
/// resolve to the right entry.
pub fn lookup_price(provider: &str, model: &str) -> Option<ModelPrice> {
    let m = model.to_lowercase();
    let p = provider.to_lowercase();

    // Local / self-hosted providers have no per-token cost.
    if p.contains("ollama") {
        return None;
    }

    // Order matters: more specific prefixes first so they aren't shadowed by a
    // shorter substring match.
    let table: &[(&str, ModelPrice)] = &[
        // Anthropic — Claude 4 family
        (
            "claude-opus-4",
            ModelPrice {
                input_per_million: 15.0,
                output_per_million: 75.0,
            },
        ),
        (
            "claude-sonnet-4",
            ModelPrice {
                input_per_million: 3.0,
                output_per_million: 15.0,
            },
        ),
        (
            "claude-haiku-4-5",
            ModelPrice {
                input_per_million: 1.0,
                output_per_million: 5.0,
            },
        ),
        (
            "claude-haiku",
            ModelPrice {
                input_per_million: 0.80,
                output_per_million: 4.0,
            },
        ),
        // OpenAI
        (
            "gpt-4o-mini",
            ModelPrice {
                input_per_million: 0.15,
                output_per_million: 0.60,
            },
        ),
        (
            "gpt-4o",
            ModelPrice {
                input_per_million: 2.50,
                output_per_million: 10.0,
            },
        ),
        (
            "o3-mini",
            ModelPrice {
                input_per_million: 1.10,
                output_per_million: 4.40,
            },
        ),
        (
            "o1",
            ModelPrice {
                input_per_million: 15.0,
                output_per_million: 60.0,
            },
        ),
        // Google
        (
            "gemini-2.0-flash",
            ModelPrice {
                input_per_million: 0.10,
                output_per_million: 0.40,
            },
        ),
        (
            "gemini-1.5-pro",
            ModelPrice {
                input_per_million: 1.25,
                output_per_million: 5.00,
            },
        ),
        (
            "gemini-1.5-flash",
            ModelPrice {
                input_per_million: 0.075,
                output_per_million: 0.30,
            },
        ),
    ];

    table
        .iter()
        .find(|(needle, _)| m.contains(needle))
        .map(|(_, price)| *price)
}

/// Total USD cost for a completion, or `0.0` when the model is unknown.
///
/// Unknown models warn loudly (log + caller-facing `is_unpriced` checks) so a
/// `$0.00` total is never mistaken for a free run. Local providers
/// (`ollama`, `mock`) are legitimately free and do not warn.
pub fn compute_cost(provider: &str, model: &str, usage: &TokenUsage) -> f64 {
    match lookup_price(provider, model) {
        Some(price) => price.cost(usage),
        None => {
            let p = provider.to_lowercase();
            if !p.contains("ollama") && !p.contains("mock") {
                tracing::warn!(
                    target: "niki::cost",
                    provider,
                    model,
                    input_tokens = usage.input_tokens,
                    output_tokens = usage.output_tokens,
                    "Model not in price table (as of {}) — cost reported as $0.00 UNDERSTATES real spend",
                    PRICE_TABLE_AS_OF,
                );
            }
            0.0
        }
    }
}

/// True when `(provider, model)` has no table entry and is not a known-free
/// local provider — i.e. a `$0.00` cost means "unmeasured", not "free".
pub fn is_unpriced(provider: &str, model: &str) -> bool {
    let p = provider.to_lowercase();
    if p.contains("ollama") || p.contains("mock") {
        return false;
    }
    lookup_price(provider, model).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_known_models_by_substring() {
        // Versioned model id should still resolve.
        assert!(lookup_price("anthropic", "claude-sonnet-4-20250514").is_some());
        assert!(lookup_price("openai", "gpt-4o-mini").is_some());
        assert!(lookup_price("google", "gemini-2.0-flash").is_some());
    }

    #[test]
    fn local_provider_is_free() {
        assert_eq!(lookup_price("ollama", "llama3"), None);
        assert_eq!(
            compute_cost(
                "ollama",
                "llama3",
                &TokenUsage {
                    input_tokens: 1_000_000,
                    output_tokens: 1_000_000,
                    ..Default::default()
                }
            ),
            0.0
        );
    }

    #[test]
    fn unknown_model_prices_as_zero() {
        assert_eq!(
            compute_cost(
                "anthropic",
                "some-future-model",
                &TokenUsage {
                    input_tokens: 100,
                    output_tokens: 100,
                    ..Default::default()
                }
            ),
            0.0
        );
    }

    #[test]
    fn sonnet_cost_math() {
        // claude-sonnet-4: $3 / 1M in, $15 / 1M out.
        let price = lookup_price("anthropic", "claude-sonnet-4-20250514").unwrap();
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            ..Default::default()
        };
        assert_eq!(price.cost(&usage), 3.0 + 15.0);
    }

    #[test]
    fn more_specific_prefix_wins() {
        // "claude-haiku-4-5" must resolve to haiku-4-5, not the generic "claude-haiku".
        let price = lookup_price("anthropic", "claude-haiku-4-5-20251001").unwrap();
        assert_eq!(price.input_per_million, 1.0);
    }

    #[test]
    fn price_table_is_fresh() {
        // The table is a frozen snapshot of vendor pricing. Failing here means
        // rates must be re-verified and PRICE_TABLE_AS_OF bumped — not that
        // the test should be weakened.
        let as_of =
            chrono::NaiveDate::parse_from_str(PRICE_TABLE_AS_OF, "%Y-%m-%d").expect("valid date");
        let age = chrono::Local::now()
            .date_naive()
            .signed_duration_since(as_of);
        assert!(
            age.num_days() <= PRICE_TABLE_MAX_AGE_DAYS,
            "price table is {} days old (as of {}); re-verify vendor rates",
            age.num_days(),
            PRICE_TABLE_AS_OF
        );
    }

    #[test]
    fn cached_and_reasoning_tokens_are_priced() {
        let price = lookup_price("anthropic", "claude-sonnet-4-20250514").unwrap();
        let usage = TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: 1_000_000,
            reasoning_tokens: 1_000_000,
        };
        // 10% of $3 input + 100% of $15 output.
        assert_eq!(price.cost(&usage), 0.3 + 15.0);
    }

    #[test]
    fn unpriced_detector() {
        assert!(is_unpriced("groq", "llama-3.1-70b-versatile"));
        assert!(!is_unpriced("anthropic", "claude-sonnet-4-20250514"));
        assert!(!is_unpriced("ollama", "llama3"));
    }
}
