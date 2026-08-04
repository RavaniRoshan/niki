# Features & Fixes — From Deep Research

All items below are derived from `../research/coding-agent-structured-output-architecture.md`, verified against sources. Priority reflects root-cause ordering: P0 fixes the stall directly.

## P0 — Root Cause (unblock the stall)

| ID  | Feature | File(s) | Source anchors | Description |
|-----|---------|---------|----------------|-------------|
| F-A | Provider-native structured output | `src/llm/provider.rs`, `src/llm/{anthropic,openai,google,ollama,nvidia}.rs` | NVIDIA NIM docs, Groq docs, OpenAI structured outputs, Anthropic tool_use, Gemini response_schema | Extend `LlmProvider` trait with a schema-aware `request_structured()` method. Per-provider: NVIDIA → `nvext.guided_json` (xgrammar); OpenAI → `response_format.json_schema strict:true`; Anthropic → synthetic-tool trick or native `output_format` beta; Gemini → `response_schema`. This constrains tokens at decode time so weak/free LLMs can't emit non-conforming JSON. |
| F-B | Layered recovery stack in `run_agent` | `src/agents/mod.rs` (retry loop L71-105, extract/validate L155-170) | Instructor, LangChain OutputFixingParser, Boundary, claudelab | Add Phase 2 retry triggered only on structured-output failures (PARSE_ERROR, VALIDATION_ERROR, NO_JSON, TRUNCATED). Sequence: local repair → re-prompt with validation error + bad output as feedback → retry with budget. Classify failures BEFORE parsing via stop_reason/finish_reason; REFUSAL and auth errors are permanent (never retry). |
| F-C | Resilient JSON parsing | `src/agents/mod.rs::extract_json`, `src/artifacts/validate.rs` | json_repair, jsonrepair, repair-json-stream, LangChain parse_partial_json, Vercel fixJson FSM | Replace naive `extract_json` (first `{`→last `}`): strip markdown fences, extract from prose/thinking, fix trailing commas/single-quotes/unescaped-control-chars/Python-literals, partial-JSON prefix for streaming. Return field-level error detail. |
| F-D | Local deterministic repair library | new `src/llm/repair.rs` | outputguard (15 strategies), json_repair, dreaming.press recovery ladder | Ordered local repair strategies: (1) strip markdown fences [the #1 issue], (2) extract JSON from prose, (3) fix trailing commas, (4) single→double quotes, (5) close brackets/braces, (6) escape control chars, (7) normalize Python literals. **Do NOT fabricate truncated values by closing braces** — re-run with more budget. |

## P1 — Prevention & Graceful Degradation

| ID  | Feature | File(s) | Source anchors | Description |
|-----|---------|---------|----------------|-------------|
| F-E | Pipeline degradation | `src/orchestrator/pipeline.rs::run_stage` (L448-476) | AWS Well-Architected AGENTREL07-BP02, Copilot partial PR, DSPy Suggest | On persistent `ArtifactValidation` after retry budget: emit **partial artifact** (best-effort with nulls for missing fields), mark stage "degraded" with warning, continue. Hard-abort ONLY on permanent errors (refusal, auth, schema-unsupported). |
| F-F | Prompt & schema hardening | `src/llm/templates/*.md`, `schemas/*.schema.json` | PromptPort canonicalizer, DSPy.rb, Anthropic cookbook | Add "JSON only, no markdown, no prose" instruction; `additionalProperties: false` on schemas; include schema in prompt as well as response_format; lower temperature for repair pass. |
| F-G | Failure-type classification | `src/agents/mod.rs` (retry phase 1), new `src/agents/errors.rs` | Boundary 8 categories, eastondev taxonomy, Anthropic stop_reason docs | Implement 8 failure categories: EMPTY_RESPONSE, REFUSAL (no retry), NO_JSON (repair→re-prompt), TRUNCATED (raise max_tokens, re-run), PARSE_ERROR (local repair), VALIDATION_ERROR (re-prompt with error), RULE_ERROR, RUN_ERROR. Use stop_reason/finish_reason to drive classification. |

## P2 — Architectural Maturity

| ID  | Feature | File(s) | Source anchors | Description |
|-----|---------|---------|----------------|-------------|
| F-H | Error locality / per-stage checkpoint | `src/orchestrator/pipeline.rs`, `src/orchestrator/state.rs` | AWS AGENTREL07-BP01, AGENTTRACE arxiv, COCO framework, DSPy LM Assertions | Per-stage checkpoints so a stage failure doesn't restart prior stages. Build an intervention catalog of observed failure modes (per Architecture of Errors — ~8-20 named modes). |
| F-I | Provider-level circuit breaker | `src/llm/provider.rs` (orchestration layer) | Cloudflare Agents retries, AWS BP02, Clarion AI | After 3 consecutive provider hard failures, escalate to a fallback provider. Full-jitter backoff. |
| F-J | Full-jitter exponential backoff | update existing backoff in `run_agent`/`run_stage` | eastondev/solana.garden, Cloudflare, Claude Code withRetry | `delay = random(0, min(2^attempt * baseDelayMs, maxDelayMs))`, base 500ms, cap 32s. Separate budgets: 3 for transient API, 2 for repair. |
| F-K | Field-level validation detail | `src/artifacts/validate.rs` | fixjson.org, Anthropic cookbook | Replace top-level-only error listing with per-field violation detail (which keys failed, what type mismatch). Drives targeted repair prompts (like Boundary's per-rule repair messages). |
| F-L | Streaming partial-JSON | `src/llm/provider.rs` streaming path | partial-json (TanStack), Vercel parsePartialJson, LangChain parse_partial_json | For streaming output: return best valid JSON prefix on each chunk for live UI; strict-parse the final chunk for the acting value. |

## Open Questions (user decisions needed)
1. Structured-output repair budget: 1 (claudelab) vs 2 (industry middle) vs 3 (Instructor/LangChain max)?
2. Which providers to implement structured output for, and in what order?
3. Pipeline degradation: partial artifact vs skip-with-warning vs abort on persistent failure?
4. JSON repair: integrate existing Rust crate vs custom implementation?
5. Scope: implement full P0-P2, or P0-P1 only (defer circuit breaker/error locality to P2)?
