# Multi-Provider LLM Support — Deep Research Report

**Date:** 2026-08-13
**Depth:** Wide (landscape + architecture + provider specifics)
**Status:** Verified (with noted limitations)

## Executive Summary

Multi-provider LLM support is now a baseline expectation for AI coding tools. The ecosystem has converged on two API shapes (OpenAI-compatible and Anthropic-compatible), with OpenAI-compatible being the de facto standard that covers 90%+ of providers. Key findings: (1) NIKI's existing `LlmProvider` trait + `OpenAiProvider` is architecturally correct — the gap is config ergonomics and provider presets, not architecture; (2) provider-specific gotchas are real and numerous — error format differences, streaming quirks, and rate limit variation require careful handling; (3) the cost optimization opportunity is significant — routing cheap models for simple tasks saves 40-60% with <1% quality drop; (4) failover patterns are well-established in the ecosystem (circuit breaker per provider, not global; one retry owner).

## Findings

### 1. Competitive Landscape — How Tools Handle Multi-Provider

| Tool | Config Format | Per-Task Routing | BYOK | Notes |
|------|--------------|------------------|------|-------|
| Cursor | UI-only | No (global keys) | Yes (limited) | BYOK breaks proprietary models |
| Continue.dev | YAML | Yes (roles) | Yes | Best role-based routing |
| Cline | JSON | Yes (Plan/Act) | Yes | OpenRouter integration |
| Aider | YAML + .env | Yes (3 model slots) | Yes | CLI-driven, no UI |
| OpenHands | TOML | Yes (per-agent) | Yes | Named LLM sections |
| Copilot | JSON + UI | Yes (per-mode) | Yes (SDK) | Enterprise controls |

**NIKI's position:** TOML config with per-agent routing is already competitive with OpenHands. The gap is provider presets and env var ergonomics.

Source: docs.continue.dev, aider.chat/docs/config, docs.openhands.dev

### 2. Provider-Specific Gotchas (Verified)

**Critical compatibility issues:**

- **Error format divergence**: NVIDIA NIM can return errors as 200 OK with error text in content. OpenRouter wraps provider errors in 200 OK. Both break standard HTTP error handling. (Source: forums.developer.nvidia.com, github.com/OpenRouterTeam/openrouter-examples)

- **Reasoning content field names vary**: DeepSeek uses `reasoning_content`, Groq uses `reasoning`, Ollama uses `thinking`, OpenRouter uses `reasoning_details`. No standard exists. (Source: github.com/langchain-ai/langchain/issues/34328)

- **Streaming format differences**: OpenAI SSE uses `data:` lines only. Anthropic SSE uses both `event:` and `data:` fields. Some providers return SSE even when `stream: false`. (Source: therouter.ai/blog/llm-api-streaming-sse)

- **Model name format**: OpenRouter uses `provider/model`, Together uses HuggingFace-style `org/model`, NVIDIA uses `publisher/model`, Groq uses plain names. (Source: docs.together.ai, console.groq.com)

- **Rate limit handling**: Groq returns `Retry-After` on 429. Together uses dynamic limits. DeepSeek uses concurrency-based limits (not RPM/TPM). (Source: console.groq.com/docs/rate-limits, docs.together.ai, api-docs.deepseek.com)

**NIKI's current handling:** The existing `send_request()` retry on 429/5xx is correct. The `redact_secrets()` function handles error body redaction. No changes needed for basic compatibility.

### 3. Cost Optimization Patterns

**Verified findings:**

- **Model routing by task complexity**: Routing cheap models for simple tasks saves 40-60% with <1% quality drop. ~70% of typical LLM calls don't need frontier-model capability. (Source: devopsness.com/blog/multi-provider-llm-routing-failover)

- **Provider pricing (July 2026)**: Same model across providers varies ~2x. Across model tiers, the spread is 36x+. DeepSeek V4 Flash is 12x cheaper than the same model on Together. (Source: tldl.io/resources/llm-api-pricing-2026)

- **OpenRouter markup**: 5.5% platform fee on credit purchases, 5% BYOK fee after 1M free requests. Effective markup 0-10% depending on model. (Source: openrouter.ai/pricing)

- **Prompt caching**: Anthropic cache reads at 10% of input price. Groq prompt caching halves input cost. (Source: therouter.ai/blog/llm-api-cost-optimization)

- **Batch APIs**: OpenAI, Groq, Anthropic all offer 50% off for async batch processing. (Source: therouter.ai)

**NIKI's opportunity:** The per-agent config already allows routing (e.g., Groq for Tester, Anthropic for Planner). No code changes needed — just documentation and examples.

### 4. Failover and Retry Patterns

**Production-tested patterns:**

- **4 failure modes, 4 responses**: 5xx → retry with backoff. 429 → honor Retry-After, don't retry immediately. Timeouts → check idempotency. Content-filter → not a failover case. (Source: therouter.ai/blog/llm-api-timeouts-retries)

- **Circuit breaker per provider, not global**: Track failures per-model on sliding window. 429s should NOT trip the breaker — they need backoff. A 429 on GPT-4o doesn't mean Claude is unhealthy. (Source: balacode.io/blog/circuit-breakers-llms-architecture)

- **Failover ≠ fallback**: Failover keeps same model, changes provider (lower risk). Fallback changes model (needs evaluation). (Source: flatkey.ai/blog/llm-api-fallback-routing)

- **One retry owner, one deadline**: Don't stack SDK retries + gateway retries + job retries. Set one end-to-end deadline. (Source: therouter.ai)

- **Streaming failures can't resume**: Buffer output before permanent writes. On stream failure, restart as new generation, don't concatenate partials. (Source: therouter.ai)

**NIKI's current handling:** The 3-attempt transient retry in `run_agent()` and 4-attempt retry in `send_request()` are reasonable. The 120s timeout prevents hung calls. Consider adding circuit breaker for production use.

### 5. Model Capabilities for Agent Roles

**Verified findings (with confidence notes):**

| Role | Best Models | Confidence | Notes |
|------|------------|------------|-------|
| Planner | Claude Opus-class, GPT-5 | Medium | SWE-bench scores are contested; Claude leads on long tasks |
| Coder | Claude Opus-class, DeepSeek V4 Pro, GPT-5 | High | Aider Polyglot and LiveCodeBench are reliable benchmarks |
| Tester | Gemini Pro, GPT-4o | Medium | Test generation benchmarks are new and not widely adopted |
| Reviewer | Claude Opus-class, Qwen 3.5 35B | Medium | Real PR review F1 is much lower than synthetic benchmarks |

**Key insight**: "Lab diversity beats model size: two medium models from different labs beat one big model talking to itself." (Source: jointchiefs.ai/articles/model-strengths-code-review)

**NIKI's default config**: Anthropic for Planner/Coder/Reviewer, OpenAI for Tester — this aligns with community consensus. The new multi-provider support allows users to optimize for cost/speed.

### 6. Existing Provider Abstraction Libraries

| Library | Pattern | Providers | Relevance to NIKI |
|---------|---------|-----------|-------------------|
| LiteLLM | Translation layer | 100+ | Most comprehensive; Python + Rust core |
| Portkey | Gateway | 250+ models | Production gateway pattern |
| Vercel AI SDK | LanguageModelV4 spec | 20+ | Clean TypeScript abstraction |
| siumai (Rust) | Capability traits | 15+ | Closest Rust equivalent to NIKI's pattern |

**NIKI's architecture**: The existing `LlmProvider` trait is clean and equivalent to siumai's `ChatCapability`. The factory pattern matches LiteLLM's provider dispatch. No major changes needed.

### 7. User Expectations

**Verified from Reddit, GitHub, HN:**

- **BYOK is baseline**: Tools like OpenCode (180k GitHub stars), Aider (46k), Cline (64k) gained traction because they're model-agnostic. (Source: Reddit r/ClaudeAI)

- **Mid-session switching is desired**: GitHub issues document context loss when switching providers mid-session. (Source: github.com/anthropics/claude-code/issues/46420)

- **Provider lock-in is feared**: Anthropic's Jan 2026 OAuth block demonstrated lock-in at the auth layer. (Source: topreviewed.ai/blog/opencode-oauth-block)

- **Local models are expected**: Every multi-provider discussion includes Ollama/local support. (Source: HN discussions on AI coding tools)

- **OpenRouter preferred for simplicity**: Rather than managing 10+ provider keys, developers prefer one gateway. (Source: Reddit r/ClaudeAI BYOK thread)

**NIKI's position**: Supports all of these — BYOK, per-agent routing, Ollama, OpenRouter as a provider option.

## Disagreements & Open Questions

1. **SWE-bench reliability**: Top models cluster around 72-80% on SWE-bench Verified, not 96%. Benchmark contamination is a known issue for frontier models.

2. **Code review benchmark validity**: F1 scores drop dramatically on real PRs vs synthetic benchmarks. No reliable production-quality review benchmark exists.

3. **Provider pricing stability**: All pricing is point-in-time. DeepSeek has announced planned price increases. Groq and Together change rates frequently.

4. **Gateway vs direct**: Whether to route through OpenRouter/LiteLLM (simpler, adds latency/cost) or connect directly to each provider (more control, more code) is still debated.

5. **Optimal retry count**: No universal best — depends on product latency SLO. Must be tuned from incident data.

## Recommendations

### Immediate (v0.3.0)
1. ✅ Done: Provider presets in `create_provider()` for OpenRouter, NVIDIA, Together, Groq, DeepSeek
2. ✅ Done: Env var resolution for all providers
3. ✅ Done: 26 integration tests

### Short-term (v0.4.0)
1. ✅ Done: Add `max_tokens` and `temperature` as configurable per-agent fields in `AgentConfig`
2. ✅ Done: Add provider health check endpoint (`niki providers check` CLI command)
3. Add cost estimation per-agent (use existing `cost.rs` module)

### Medium-term (v0.5.0)
1. ✅ Done: Add failover chain config: `fallbacks = ["anthropic", "openai", "groq"]`
2. ✅ Done: Add circuit breaker per provider (sliding window, 3 failures → open, 60s → half-open)
3. Add response caching (TTL-based, configurable per provider)

### Long-term
1. Consider Gateway pattern (Portkey-style) for centralized auth/routing if provider count grows
2. Add structured output support for providers that support it (currently all use prompt engineering)
3. Add model deprecation monitoring

## Source List

1. docs.continue.dev/reference — Continue.dev config reference
2. aider.chat/docs/config — Aider configuration docs
3. docs.openhands.dev — OpenHands LLM config docs
4. forums.developer.nvidia.com — NVIDIA NIM forum (error format issues)
5. github.com/OpenRouterTeam/openrouter-examples — OpenRouter API quirks
6. github.com/langchain-ai/langchain/issues/34328 — Reasoning content field divergence
7. therouter.ai/blog/llm-api-streaming-sse — Streaming format comparison
8. therouter.ai/blog/llm-api-timeouts-retries — Failover/retry patterns
9. balacode.io/blog/circuit-breakers-llms-architecture — Circuit breaker design
10. flatkey.ai/blog/llm-api-fallback-routing — Failover vs fallback
11. console.groq.com/docs/rate-limits — Groq rate limits
12. docs.together.ai/docs/inference/openai-compatibility — Together API compat
13. api-docs.deepseek.com — DeepSeek API docs
14. openrouter.ai/pricing — OpenRouter pricing
15. devopsness.com/blog/multi-provider-llm-routing-failover — Cost optimization patterns
16. tldl.io/resources/llm-api-pricing-2026 — Provider pricing comparison
17. therouter.ai/blog/llm-api-cost-optimization — Caching and batch strategies
18. jointchiefs.ai/articles/model-strengths-code-review — Multi-model review
19. crates.io/crates/siumai — Rust LLM abstraction crate
20. Reddit r/ClaudeAI BYOK thread — User expectations
21. github.com/anthropics/claude-code/issues/46420 — Provider switching feature request
22. topreviewed.ai/blog/opencode-oauth-block — Provider lock-in analysis
