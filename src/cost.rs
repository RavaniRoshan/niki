//! Cost & performance accounting for NIKI runs.
//!
//! Token counts come from the LLM providers' own usage reports (see
//! [`crate::llm::provider::StreamChunk::Usage`]); this module turns those into a
//! USD cost using a best-effort price table. Unknown models (e.g. a local Ollama
//! model or a brand-new model id) price as `0.0` so a run still completes and the
//! report shows the token counts even when we can't attach a dollar figure.

use crate::llm::provider::TokenUsage;

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
        ("claude-opus-4", ModelPrice { input_per_million: 15.0, output_per_million: 75.0 }),
        ("claude-sonnet-4", ModelPrice { input_per_million: 3.0, output_per_million: 15.0 }),
        ("claude-haiku-4-5", ModelPrice { input_per_million: 1.0, output_per_million: 5.0 }),
        ("claude-haiku", ModelPrice { input_per_million: 0.80, output_per_million: 4.0 }),
        // OpenAI
        ("gpt-4o-mini", ModelPrice { input_per_million: 0.15, output_per_million: 0.60 }),
        ("gpt-4o", ModelPrice { input_per_million: 2.50, output_per_million: 10.0 }),
        ("o3-mini", ModelPrice { input_per_million: 1.10, output_per_million: 4.40 }),
        ("o1", ModelPrice { input_per_million: 15.0, output_per_million: 60.0 }),
        // Google
        ("gemini-2.0-flash", ModelPrice { input_per_million: 0.10, output_per_million: 0.40 }),
        ("gemini-1.5-pro", ModelPrice { input_per_million: 1.25, output_per_million: 5.00 }),
        ("gemini-1.5-flash", ModelPrice { input_per_million: 0.075, output_per_million: 0.30 }),
    ];

    table
        .iter()
        .find(|(needle, _)| m.contains(needle))
        .map(|(_, price)| *price)
}

/// Total USD cost for a completion, or `0.0` when the model is unknown.
pub fn compute_cost(provider: &str, model: &str, usage: &TokenUsage) -> f64 {
    match lookup_price(provider, model) {
        Some(price) => price.cost(usage),
        None => 0.0,
    }
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
        assert_eq!(compute_cost("ollama", "llama3", &TokenUsage { input_tokens: 1_000_000, output_tokens: 1_000_000 }), 0.0);
    }

    #[test]
    fn unknown_model_prices_as_zero() {
        assert_eq!(compute_cost("anthropic", "some-future-model", &TokenUsage { input_tokens: 100, output_tokens: 100 }), 0.0);
    }

    #[test]
    fn sonnet_cost_math() {
        // claude-sonnet-4: $3 / 1M in, $15 / 1M out.
        let price = lookup_price("anthropic", "claude-sonnet-4-20250514").unwrap();
        let usage = TokenUsage { input_tokens: 1_000_000, output_tokens: 1_000_000 };
        assert_eq!(price.cost(&usage), 3.0 + 15.0);
    }

    #[test]
    fn more_specific_prefix_wins() {
        // "claude-haiku-4-5" must resolve to haiku-4-5, not the generic "claude-haiku".
        let price = lookup_price("anthropic", "claude-haiku-4-5-20251001").unwrap();
        assert_eq!(price.input_per_million, 1.0);
    }
}
