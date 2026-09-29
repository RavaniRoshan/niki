//! The model catalogue: what a provider actually offers, fetched rather than
//! guessed.
//!
//! This did not exist for any hosted provider. `niki recommend` carries a
//! hardcoded table — `claude-opus-4`, `gpt-4o-mini` — which cannot know what
//! your account can reach. On OpenRouter that is doubly wrong: the catalogue is
//! several hundred models wide, names are fully qualified
//! (`anthropic/claude-sonnet-4`), and the interesting axis is not "strong" or
//! "cheap" but *effort* — a reasoning budget a model exposes rather than a
//! property we can assume from its name.
//!
//! Ollama is the one provider that already had a listing, and it is local, so
//! it keeps its existing path. Everything else is OpenAI-compatible
//! `GET {base_url}/models`, which is the shape OpenRouter, Together, Groq,
//! DeepSeek, NVIDIA, OpenCode Zen, Kimi and KiloCode all speak.

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::config::NikiConfig;
use crate::llm::provider::{default_base_url, missing_key_error};

/// The OpenAI-compatible `/models` response.
///
/// Only `id` and `pricing` are read. Everything else a provider sends — context
/// length, modality, deprecation — differs per provider, so a field we
/// half-understand would be worse than a field we do not claim.
#[derive(Debug, Deserialize)]
struct CatalogueResponse {
    /// No `#[serde(default)]`.
    ///
    /// A response without `data` is not an empty catalogue, it is something
    /// else — usually `{"error": "..."}`, which is what a provider answers when
    /// the key is not authorised to browse. With the default, that parsed as
    /// zero models and a user would be told "no models available" when the
    /// real answer was "fix your key". An explicit `{"data": []}` still parses,
    /// and is still empty.
    data: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
    /// OpenRouter and Together report per-token prices here; others omit it.
    #[serde(default)]
    pricing: Option<ModelPricing>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ModelPricing {
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    completion: Option<String>,
}

/// One model as the catalogue reports it, plus what we can add ourselves.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogueEntry {
    pub id: String,
    /// Price per million tokens, when the provider reports one.
    pub price_per_mtok: Option<(Option<f64>, Option<f64>)>,
    /// What this model looks like to a user picking one.
    pub traits: Vec<ModelTrait>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelTrait {
    /// A reasoning model that takes an effort / thinking budget.
    Reasoning,
    /// The provider offers a `:free` tier, or a price of zero.
    Free,
    /// Long-context, which matters for a Coder stage.
    LongContext,
}

impl CatalogueEntry {
    /// The provider family, from a fully-qualified id.
    ///
    /// OpenRouter ids are `vendor/model`; a bare id has no vendor. Only used
    /// for display, never to pick a provider.
    pub fn vendor(&self) -> Option<&str> {
        self.id.split_once('/').map(|(v, _)| v)
    }

    fn classify(&self) -> Vec<ModelTrait> {
        let mut out = Vec::new();
        let id = self.id.to_ascii_lowercase();
        // A list, and an acknowledged one. Whether a model supports an effort
        // control is a property of the model, and providers do not report it
        // in a shape we can rely on — so this is a hint for the user, and the
        // help text says so. A wrong hint costs one wasted experiment; a
        // confident claim we cannot verify costs more.
        if [
            "o1",
            "o3",
            "o4",
            "r1",
            "qwq",
            "thinking",
            "reasoner",
            "deepseek-r",
        ]
        .iter()
        .any(|m| id.contains(m))
        {
            out.push(ModelTrait::Reasoning);
        }
        if id.contains(":free") || id.ends_with("-free") {
            out.push(ModelTrait::Free);
        }
        if let Some((prompt, _)) = self.price_per_mtok {
            if prompt == Some(0.0) {
                out.push(ModelTrait::Free);
            }
        }
        out
    }
}

/// Fetch a provider's catalogue.
///
/// Best effort in the sense that an unreachable or unparseable catalogue is an
/// error the caller can explain, not a panic — and in the sense that a provider
/// that does not implement `/models` is a normal thing to hit, not a failure of
/// this program.
pub async fn fetch(
    provider_name: &str,
    base_url: Option<&str>,
    api_key: Option<&str>,
) -> Result<Vec<CatalogueEntry>> {
    if provider_name == "ollama" {
        return ollama_catalogue();
    }

    let base = base_url
        .map(str::to_string)
        .or_else(|| default_base_url(provider_name).map(str::to_string))
        .with_context(|| {
            format!(
                "{provider_name} has no default base URL. Set base_url under \
                 [providers.{provider_name}] in niki.toml."
            )
        })?;
    let key = api_key.ok_or_else(|| missing_key_error(provider_name))?;

    let url = format!("{}/models", base.trim_end_matches('/'));
    let client = crate::llm::provider::http_client()?;
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {key}"))
        .send()
        .await
        .with_context(|| format!("could not reach {url} to list models"))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .with_context(|| format!("could not read the model list from {url}"))?;
    if !status.is_success() {
        anyhow::bail!(
            "{url} answered {status}. Some providers only expose a catalogue to accounts \
             with billing enabled; `niki providers check` will tell you if the key works at all."
        );
    }
    let parsed: CatalogueResponse = serde_json::from_str(&body)
        .with_context(|| format!("{url} did not answer with a model list"))?;

    Ok(parsed
        .data
        .into_iter()
        .map(|m| {
            let entry = CatalogueEntry {
                price_per_mtok: m.pricing.map(|p| {
                    (
                        p.prompt.as_deref().and_then(parse_price),
                        p.completion.as_deref().and_then(parse_price),
                    )
                }),
                id: m.id,
                traits: Vec::new(),
            };
            let mut entry = entry;
            entry.traits = entry.classify();
            entry
        })
        .collect())
}

/// OpenRouter and Together quote per-token strings; we report per million.
fn parse_price(s: &str) -> Option<f64> {
    s.parse::<f64>()
        .ok()
        .map(|per_token| per_token * 1_000_000.0)
}

fn ollama_catalogue() -> Result<Vec<CatalogueEntry>> {
    let models = crate::cli::auth::ollama_models();
    if models.is_empty() {
        anyhow::bail!(
            "no models found in the local Ollama. Install one with \
             `ollama pull qwen2.5-coder:3b` and try again."
        );
    }
    Ok(models
        .into_iter()
        .map(|id| {
            let mut entry = CatalogueEntry {
                id,
                price_per_mtok: Some((Some(0.0), Some(0.0))),
                traits: Vec::new(),
            };
            entry.traits = entry.classify();
            entry
        })
        .collect())
}

/// Which providers from the config can be asked.
pub fn configured_providers(config: &NikiConfig) -> Vec<(String, String)> {
    config
        .providers
        .iter()
        .map(|(name, cfg)| {
            (
                name.clone(),
                cfg.base_url
                    .clone()
                    .or_else(|| default_base_url(name).map(str::to_string))
                    .unwrap_or_default(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vendor_is_read_from_a_qualified_id() {
        let e = CatalogueEntry {
            id: "anthropic/claude-sonnet-4".into(),
            price_per_mtok: None,
            traits: vec![],
        };
        assert_eq!(e.vendor(), Some("anthropic"));
        let bare = CatalogueEntry {
            id: "gpt-4o-mini".into(),
            price_per_mtok: None,
            traits: vec![],
        };
        assert_eq!(bare.vendor(), None, "a bare id names no vendor");
    }

    #[test]
    fn reasoning_models_are_marked_but_never_asserted_to_the_user() {
        let classify = |id: &str| {
            let mut e = CatalogueEntry {
                id: id.into(),
                price_per_mtok: None,
                traits: vec![],
            };
            e.traits = e.classify();
            e.traits
        };
        assert!(classify("openai/o3-mini").contains(&ModelTrait::Reasoning));
        assert!(classify("deepseek/deepseek-r1").contains(&ModelTrait::Reasoning));
        assert!(!classify("anthropic/claude-sonnet-4").contains(&ModelTrait::Reasoning));
    }

    #[test]
    fn free_tiers_are_marked_from_the_id_or_the_price() {
        let mut by_id = CatalogueEntry {
            id: "meta-llama/llama-3.3-70b-instruct:free".into(),
            price_per_mtok: None,
            traits: vec![],
        };
        by_id.traits = by_id.classify();
        assert!(by_id.traits.contains(&ModelTrait::Free));

        let mut by_price = CatalogueEntry {
            id: "some/zero-priced".into(),
            price_per_mtok: Some((Some(0.0), Some(0.0))),
            traits: vec![],
        };
        by_price.traits = by_price.classify();
        assert!(by_price.traits.contains(&ModelTrait::Free));
    }

    /// Prices arrive per token; users compare per million.
    #[test]
    fn prices_are_converted_from_per_token_to_per_million() {
        assert_eq!(parse_price("0.000003"), Some(3.0));
        assert_eq!(parse_price("0"), Some(0.0));
        assert_eq!(
            parse_price("free"),
            None,
            "an unparseable price is absent, not zero"
        );
    }

    /// The response shape every OpenAI-compatible provider actually sends.
    #[test]
    fn the_catalogue_response_shape_is_what_providers_return() {
        let body = r#"{"data":[
            {"id":"anthropic/claude-sonnet-4","pricing":{"prompt":"0.000003","completion":"0.000015"}},
            {"id":"meta-llama/llama-3.3-70b-instruct:free","pricing":{"prompt":"0","completion":"0"}}
        ]}"#;
        let parsed: CatalogueResponse = serde_json::from_str(body).expect("parses");
        assert_eq!(parsed.data.len(), 2);
        assert_eq!(parsed.data[0].id, "anthropic/claude-sonnet-4");
        let p = parsed.data[0].pricing.clone().expect("pricing present");
        assert_eq!(parse_price(p.completion.as_deref().unwrap()), Some(15.0));
    }

    /// A provider that answers something else must not crash the command.
    #[test]
    fn an_unexpected_response_is_an_error_not_a_panic() {
        let r: Result<CatalogueResponse, _> = serde_json::from_str(r#"{"error":"no access"}"#);
        assert!(
            r.is_err(),
            "a shape we do not understand must not parse as a catalogue"
        );
    }

    /// An entry with no `pricing` at all — several providers omit it entirely.
    #[test]
    fn a_model_without_pricing_still_appears() {
        let body = r#"{"data":[{"id":"local-model"}]}"#;
        let parsed: CatalogueResponse = serde_json::from_str(body).expect("parses");
        assert!(parsed.data[0].pricing.is_none());
        assert_eq!(parsed.data[0].id, "local-model");
    }
}
