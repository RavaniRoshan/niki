# Multi-Provider Architecture — NIKI

**Status:** Implemented (v0.3.0+)
**Last updated:** 2026-08-13

## Overview

NIKI supports multiple LLM providers via a trait-based abstraction. Users can mix
providers per-agent (BYOK — Bring Your Own Key) and choose from pre-configured
presets or custom endpoints.

## Supported Providers

| Provider | Protocol | Auth Method | Base URL | Preset Name |
|----------|----------|-------------|----------|-------------|
| Anthropic | Anthropic Messages API | `x-api-key` header | `https://api.anthropic.com` | `anthropic` |
| OpenAI | OpenAI Chat Completions | `Authorization: Bearer` | `https://api.openai.com/v1` | `openai` |
| OpenRouter | OpenAI-compatible | `Authorization: Bearer` | `https://openrouter.ai/api/v1` | `openrouter` |
| NVIDIA NIM | OpenAI-compatible | `Authorization: Bearer` | `https://integrate.api.nvidia.com/v1` | `nvidia` |
| Together AI | OpenAI-compatible | `Authorization: Bearer` | `https://api.together.xyz/v1` | `together` |
| Groq | OpenAI-compatible | `Authorization: Bearer` | `https://api.groq.com/openai/v1` | `groq` |
| DeepSeek | OpenAI-compatible | `Authorization: Bearer` | `https://api.deepseek.com/v1` | `deepseek` |
| Google Gemini | Gemini API | `x-goog-api-key` | `https://generativelanguage.googleapis.com` | `google` |
| Ollama | Ollama API | Optional Bearer | `http://localhost:11434` | `ollama` |

## Architecture

```
src/llm/
├── mod.rs              # Module exports
├── provider.rs         # LlmProvider trait + create_provider() factory
├── anthropic.rs        # Anthropic Messages API adapter
├── openai.rs           # OpenAI Chat Completions adapter
├── google.rs           # Google Gemini API adapter
├── ollama.rs           # Ollama local API adapter
├── mock.rs             # Mock provider for tests
└── repair.rs           # JSON repair utilities
```

### Provider Trait

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;
    async fn stream(&self, request: CompletionRequest)
        -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>>;
    fn provider_name(&self) -> &str;
    fn supports_structured_output(&self) -> bool { false }
    async fn request_structured(&self, request: CompletionRequest, schema: &Value)
        -> Result<CompletionResponse> { self.complete(request).await }
}
```

### Factory Function

```rust
pub fn create_provider(name: &str, config: &ProviderConfig) -> Result<Box<dyn LlmProvider>> {
    match name {
        "anthropic" => Ok(Box::new(AnthropicProvider::new(config)?)),
        "openai" | "openrouter" | "nvidia" | "together" | "groq" | "deepseek" => {
            Ok(Box::new(OpenAiProvider::new(config)?))
        }
        "google" => Ok(Box::new(GoogleProvider::new(config)?)),
        "ollama" => Ok(Box::new(OllamaProvider::new(config)?)),
        "mock" => Ok(Box::new(MockProvider::new(config.base_url.as_deref())?)),
        _ => Err(anyhow!("Unknown provider: {}", name)),
    }
}
```

All OpenAI-compatible providers share the same `OpenAiProvider` implementation.
The only difference is the `base_url` configured in `niki.toml`.

## Configuration

### Provider Config (`niki.toml`)

```toml
[providers.anthropic]
api_key = "sk-ant-..."
default_model = "claude-sonnet-4-20250514"

[providers.openrouter]
api_key = "sk-or-..."
base_url = "https://openrouter.ai/api/v1"
default_model = "anthropic/claude-sonnet-4"

[providers.nvidia]
api_key = "nvapi-..."
base_url = "https://integrate.api.nvidia.com/v1"
default_model = "meta/llama-3.1-405b-instruct"

[providers.openai]
api_key = "sk-..."
default_model = "gpt-4o"
```

### Per-Agent Model Assignment

```toml
[agents.planner]
provider = "anthropic"
model = "claude-sonnet-4-20250514"

[agents.coder]
provider = "openrouter"
model = "anthropic/claude-sonnet-4"

[agents.tester]
provider = "groq"
model = "llama-3.1-70b-versatile"

[agents.reviewer]
provider = "nvidia"
model = "meta/llama-3.1-405b-instruct"
```

### Environment Variables

Each provider supports env var overrides:

| Provider | API Key Env | Base URL Env | Model Env |
|----------|-------------|--------------|-----------|
| Anthropic | `ANTHROPIC_API_KEY` | `ANTHROPIC_BASE_URL` | `ANTHROPIC_MODEL` |
| OpenAI | `OPENAI_API_KEY` | `OPENAI_BASE_URL` | `OPENAI_MODEL` |
| OpenRouter | `OPENROUTER_API_KEY` | — | — |
| NVIDIA | `NVIDIA_API_KEY` | — | — |
| Together | `TOGETHER_API_KEY` | — | — |
| Groq | `GROQ_API_KEY` | — | — |
| DeepSeek | `DEEPSEEK_API_KEY` | — | — |
| Google | `GOOGLE_API_KEY` | — | — |

## Provider Selection Logic

1. Per-stage override: `[pipeline.stages]` → highest priority
2. Per-agent default: `[agents.<role>]` → `provider` field
3. Pipeline default: `[pipeline]` → `default_provider` field
4. Fallback: `"anthropic"` (if configured)

## Testing

- Unit tests per provider (endpoint construction, auth headers)
- Integration tests with `wiremock` HTTP mocks
- Mock provider for full pipeline testing without real API calls
- E2E tests with real providers (gated by env vars)

## Adding a New Provider

1. If OpenAI-compatible: just add a preset name in `create_provider()` match
2. If custom protocol: create `src/llm/<name>.rs`, implement `LlmProvider`
3. Add env var resolution in `config/types.rs::apply_env_vars()`
4. Add tests (endpoint, auth, streaming)
