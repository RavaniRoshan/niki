# Research Report: Coding-Agent Architecture & Structured-Output Robustness

**Topic:** How proven coding agents (Claude Code, Cloud Code, OpenCode, KiloCode, Cline, Aider) handle structured JSON output — and what we should adopt to fix the pipeline stall when free LLMs (NVIDIA endpoints) emit JSON that doesn't match our schemas.

**Date:** 2026-08-04
**Method:** 8 parallel research subagents + 1 adversarial verifier subagent (failed on output-token limits; resolved via targeted manual verification of 3 critical claims + NVIDIA docs fetch)
**Status:** Complete

---

## Executive Summary

Our pipeline stalls because `run_agent` (`src/agents/mod.rs`) only retries *transient network errors* and has **no recovery path** when an LLM returns malformed or schema-invalid JSON — `validate_artifact` (`src/artifacts/validate.rs`) fails hard, and the `?` propagates up through `run_stage` (`src/orchestrator/pipeline.rs:461`), aborting the entire stage.

This is a solved problem for production coding agents. The proven pattern is a **three-layer recovery stack**:

1. **Constrain at the source** (P0): use provider-native structured output (JSON mode / constrained decoding) instead of free-text + regex extraction. NVIDIA NIM supports this via `nvext.guided_json` with xgrammar; OpenAI/Gemini/vLLM support `json_schema`; Groq supports it only on select models.
2. **Repair locally, then re-prompt** (P0): replace naive `extract_json` with a `json_repair`-style salvage parser (strip fences, fix trailing commas, extract from prose), and on validation failure feed the error + bad output back to the model with a small retry budget (2–3).
3. **Degrade, don't die** (P1): if repair fails after the budget is exhausted, emit a partial/best-effort artifact and continue the pipeline with a warning — only escalate to human review on permanent failures (refusals, auth errors).

The industry-standard retry budget is **3 attempts** for transient API errors. For structured-output repair specifically, sources disagree: claudelab recommends a *single* repair (diminishing returns, >80% first-try success), while Instructor/LangChain/Boundary default to 2–3. We should make ours **configurable, defaulting to 2**.

---

## Findings by Sub-Question

### R1 — Structured-Output Mechanisms

**Question:** What structured-output mechanisms do coding agents use, and which work on free/NVIDIA NIM/Groq/Together/OpenRouter?

**Findings:**

- **Claude Code** uses a "synthetic-tool trick": wraps the user's JSON schema as a fake tool definition named `StructuredOutput`, forces the model to call it via Anthropic's `tool_use` API, validates client-side with Ajv, and enforces a retry budget `MAX_STRUCTURED_OUTPUT_RETRIES` (default 5). It does NOT use Anthropic's native structured-outputs beta.
  - (source: https://kenhuangus.substack.com/p/chapter-15-structured-output-and)
- **Anthropic** now offers native structured outputs via `output_format: {type: "json_schema", schema: ...}` with beta header `structured-outputs-2025-11-13` and `client.beta.messages.parse(...)` — an alternative to the synthetic-tool approach Claude Code uses.
  - (source: https://docs.claude.com/en/docs/build-with-claude/structured-outputs, https://thomas-wiegold.com/blog/claude-api-structured-output/)
- **OpenCode** exposes only `--output-format json` (wraps CLI response in a JSON object, no schema enforcement). Schema-constrained output is an open feature request (issues #10456, #9320 — not yet implemented).
  - (source: https://github.com/opencode-ai/opencode)
- **Cline** uses "tool-first enforcement": every model turn *must* be a tool call; plain-text refusal is rejected with an error message. Uses XML-based tool calling. No dedicated JSON-schema output mechanism — structure emerges from forced tool calls.
  - (source: https://medium.com/@floralan212/inside-cline-how-its-agentic-chat-system-really-works-3d582935efa5, https://arxiv.org/html/2604.03515v2)
- **Aider** deliberately avoids structured JSON/tool-calling for code edits. Uses plain-text SEARCH/REPLACE blocks. Benchmarked: models produce *worse* code when forced to wrap edits as JSON, even with OpenAI "strict" mode.
  - (source: https://aider.chat/2024/08/14/code-in-json.html)
- **NVIDIA NIM / Build API** uses `extra_body={"nvext": {"guided_json": schema}}` — grammar-constrained decoding with the **xgrammar** backend (fastest), falling back to outlines. Does NOT use standard `response_format` for schema-constrained output. Supports JSON schema, regex (`guided_regex`), EBNF grammar (`guided_grammar`). Docs explicitly warn: `response_format={"type":"json_object"}` "permits the model to produce any valid JSON, including empty objects" — use `guided_json` instead.
  - (source: https://docs.nvidia.com/nim/large-language-models/1.13.0/structured-generation.html — **VERIFIED**, Jan 2026)
- **Groq** supports `json_schema` with `strict: true` (constrained decoding, **guaranteed** schema compliance), but **ONLY** on `gpt-oss-20b`, `gpt-oss-120b`, `gpt-oss-safeguard-20b`. All other models (e.g. `llama-3.3-70b-versatile`) only support `json_object` (valid JSON syntax, **no schema guarantee**).
  - (source: https://console.groq.com/docs/structured-outputs)
- **OpenRouter** supports `json_schema` with `strict: true`, but support is **per-endpoint/per-provider**, not per-model. Use `require_parameters: true` in provider prefs to force routing to compliant endpoints. The `openrouter/free` router filters for structured-output support.
  - (source: https://openrouter.ai/docs/guides/features/structured-outputs)
- **Together.ai** supports `response_format` with `json_schema` on select models. Recommends including the schema in the prompt *in addition to* `response_format`.
  - (source: https://docs.together.ai/docs/inference/chat/structured-outputs)
- **DeepInfra** supports only `json_object` mode; no `json_schema`.
  - (source: https://deepinfra.com/blog/json-mode)
- **Local backends** (vLLM, SGLang) support structured outputs via their own `guided_json`/`response_format` paths with xgrammar/outlines/llguidance backends.
  - (sources: https://docs.vllm.ai/en/latest/features/structured_outputs/, https://docs.sglang.ai/advanced_features/structured_outputs.html)
- **Backend performance**: xgrammar fastest for cached schemas; guidance lowest per-request latency for many distinct schemas; outlines most mature but slowest first-token.
  - (source: https://discuss.vllm.ai/t/general-questions-on-structured-output-backend/1444, https://developers.redhat.com/articles/2025/06/03/structured-outputs-vllm-guiding-ai-responses)

### R2 — Repair & Retry Loops

**Question:** How do coding agents handle malformed JSON or schema validation failures?

**Findings:**

- **The standard pattern is: detect → local deterministic repair → if still failing, feed error back to model → retry → cap → degrade/fallback.**
- **Instructor** wraps the provider client; on `ValidationError` or `JSONDecodeError`, captures the error, formats it as feedback, appends the bad completion, and re-asks the LLM. Instructor default `max_retries=0`; production setting 2–3. Tenacity guidance: validation errors → 2–3 attempts / 1s→10s; rate limits → 5 / 1s→60–120s; network → 4 / 2s→30s.
  - (source: https://python.useinstructor.com/learning/validation/retry_mechanisms/, https://python.useinstructor.com/concepts/retrying/)
- **LangChain** `OutputFixingParser` / `RetryOutputParser` send the original prompt + failed completion + parse error back to the LLM ("The following output didn't parse correctly..."). `AgentExecutor` `handle_parsing_errors` feeds the failure back into the agent scratchpad as a tool observation so the model retries. `handle_errors` defaults to `True` (retry on validation errors); can be set `False` for hard-abort or a custom filter.
  - (source: https://github.com/langchain-ai/langchain/blob/e09699298428d1a2e23193e4074f9c9a99413c1c/libs/langchain/langchain_classic/output_parsers/fix.py, https://docs.langchain.com/oss/python/langchain/structured-output)
- **OpenCode** implements `experimental_repairToolCall` that detects JSON parse errors in tool-call arguments and repairs truncation, unescaped strings, missing closers, and duplicated objects before surfacing failure.
  - (source: https://github.com/anomalyco/opencode/pull/23064)
- **Pydantic AI** raises `ModelRetry` from output validators; supports per-tool `max_retries` and a global run budget.
  - (source: https://pydantic.dev/docs/ai/core-concepts/output/)
- **Boundary** classifies the failure, generates a *targeted* repair message from the specific schema violation, and replays it to the model (not a blind retry). Defaults to 3 attempts, no delay.
  - (source: https://docs.withboundary.com/concepts/repair-loop)

**Retry-budget norms:**
- **3 attempts** is the near-universal default for transient API errors (Vercel AI SDK `maxRetries=3`, LangChain `.with_retry(max_attempts=3)`, Cloudflare `this.retry()` maxAttempts=3, Copilot driver 3 attempts, Copilot extension max 3 retries, Clarion AI).
  - (sources: https://ai-sdk.dev/docs/ai-sdk-core/error-handling, https://developers.cloudflare.com/agents/api-reference/retries/, https://github.com/github/gh-aw/pull/25329)
- **Structured-output repair** budgets are lower: claudelab recommends **one** repair (diminishing returns, >80% first-try success); Instructor/LangChain/agentcast/Boundary default to **2–3** for validation errors.
  - (source: https://claudelab.net/en/articles/api-sdk/claude-api-structured-output-schema-validation-repair-loop)
- **Disagreement:** claudelab ("one repair") vs. Instructor/LangChain/Boundary ("2–3 retries"). Both converge on "don't loop forever."

**Backoff:**
- Exponential with jitter for transient/API errors; little-to-no delay for schema-repair retries.
- Claude Code `withRetry`: `BASE_DELAY_MS = 500` ± 25% jitter, total `DEFAULT_MAX_RETRIES = 10` — but this is for **transient HTTP/API errors**, **not** structured-output repair.
  - (source: https://claudelab.net/en/articles/api-sdk/claude-api-structured-output-schema-validation-repair-loop, https://claude-wiki.com/error-handling-and-recovery.html)

**Distinguishing retryable from permanent failures:**
- Read the stop reason / finish reason **before** parsing.
- Anthropic: `stop_reason == "max_tokens"` → truncation (raise `max_tokens` and re-run); `stop_reason == "refusal"` → never retry.
  - (source: https://platform.claude.com/docs/en/build-with-claude/handling-stop-reasons, https://claudelab.net/en/articles/api-sdk/claude-api-stop-reason-handling-guide)
- Error taxonomy: 429/529/503/connect-timeout → retry w/ backoff; 500/502/504 → limited retry; 400/422/invalid-JSON-schema → no retry, fix prompt/schema; 401/403/content-filter → no retry; 413/context-length → no retry, truncate/swap.
  - (source: https://eastondev.com/blog/en/posts/ai/20260506-llm-structured-output/)
- Boundary's 8 failure categories: `EMPTY_RESPONSE`, `REFUSAL` (no retry by default), `NO_JSON`, `TRUNCATED`, `PARSE_ERROR`, `VALIDATION_ERROR`, `RULE_ERROR`, `RUN_ERROR`.
  - (source: https://docs.withboundary.com/concepts/repair-loop)

**Local deterministic repair strategies (the "cheaper rung of the ladder"):**
- Strip markdown code fences (#1 issue), extract JSON from prose/thinking blocks, fix trailing commas, single→double quotes, close unclosed brackets/braces, escape control characters, normalize Python literals (`True`→`true`, `False`→`false`, `None`→`null`).
  - (sources: https://github.com/ndcoder/outputguard, https://github.com/mangiucugna/json_repair)
- **Do NOT repair truncated JSON by closing braces** — that fabricates values the model never generated. Re-run with more `max_tokens` instead.
  - (source: https://dreaming.press/posts/when-structured-output-breaks-repair-recovery-playbook.html)

### R3 — Resilient JSON Extraction / Parsing

**Question:** What resilient JSON parsing tools/techniques exist beyond naive extraction?

**Findings:**

- **json_repair (Python)**: drop-in replacement for `json.loads()`. Parses via BNF grammar + heuristic fixes: missing quotes/commas/brackets, stray prose, comments, single quotes, Python literals. Strict `json.loads` first, falls back to repair parser. Supports `stream_stable` for streaming and schema-guided repairs (`standard` / `salvage` modes).
  - (source: https://raw.githubusercontent.com/mangiucugna/json_repair/main/README.md)
- **jsonrepair (JS/TS)**: streaming `fixJson(input)` + `jsonrepairTransform()` API. Fixes: missing key quotes, missing escapes, trailing commas, truncated JSON, single quotes, Python constants, comments, fenced code blocks, smart quotes, MongoDB types, string concatenation, NDJSON→array.
  - (source: https://github.com/josdejong/jsonrepair)
- **repair-json-stream (JS/TS)**: O(n) state machine, zero regex, zero deps, 5.5KB. `repairJson` (never throws), `extractJson` (pulls JSON from prose/thinking blocks), `preprocessJson` (strips wrappers), `IncrementalJsonRepair` (real-time UI), `jsonRepairStream` (Web Streams TransformStream).
  - (source: https://github.com/prxtenses/repair-json-stream)
- **partial-json (npm)**: zero-dep partial parser. `parse(str, allowPartial)` with Allow flags. Returns best valid prefix of in-progress JSON. Used in TanStack AI SDK's `PartialJSONParser`.
  - (source: https://www.npmjs.com/package/partial-json)
- **LangChain** `JsonOutputParser`: strips ```` ```json ```` fenced blocks via regex. `parse_partial_json`: char-by-char with `is_inside_string`/`escaped`/`stack` tracking; closes unterminated strings, reverses stack to close brackets, pops chars from end until valid parse found.
  - (source: https://raw.githubusercontent.com/langchain-ai/langchain/master/libs/core/langchain_core/utils/json.py)
- **Vercel AI SDK** `parsePartialJson`: 3-stage approach — (1) `safeParseJSON` direct parse, (2) `fixJson` 18-state FSM, (3) failed-parse fallback. Returns a `state` field distinguishing clean vs. repaired parses.
  - (sources: https://raw.githubusercontent.com/vercel/ai/main/packages/ai/src/util/parse-partial-json.ts, https://raw.githubusercontent.com/vercel/ai/main/packages/ai/src/util/fix-json.ts)
- **DSPy** `JSONAdapter.parse()`: `json_repair.loads(completion)`, fallback to regex `r"\{(?:[^{}]|(?R))*?\}"` for embedded JSON. Filters to known output fields, casts to type annotations.
  - (source: https://github.com/stanfordnlp/dspy/blob/main/dspy/adapters/json_adapter.py)
- **Anthropic/Claude** cookbook: no formal JSON mode. Three techniques: (1) **tool use** with `input_schema` (production-grade), (2) assistant prefill starting with `{`, (3) regex extraction from backticks/XML tags. Explicitly: "always wrap `json.loads()` in try/except."
  - (source: https://platform.claude.com/cookbook/misc-how-to-enable-json-mode)
- **PromptPort canonicalizer** (5-step best-practice pipeline): strip fences → extract span via bracket matching → repair syntax → normalize keys/values → fill missing with null. Benchmarks show "format collapse" in weak models (Gemma-2B strict 0.116 vs. 0.246 after canonicalization).
  - (source: https://arxiv.org/pdf/2601.06151)
- **Fail-loudly**: if JSON drives a financial/permission/destructive action, do NOT silently repair — reject, retry, or surface raw output.
  - (source: https://fixjson.org/guides/repair-llm-json-output)
- **Fail-loudly pattern**: for state-changing actions, reject rather than auto-repair corrupted JSON.

### R4 — Claude Code / Cloud Code Architecture

**Question:** Internal architecture of Claude Code and Cloud Code — agent loop, harness, prompt/schema versioning, session/context management.

**Findings:**

- **Claude Code** uses a 1,729-line async generator `queryLoop()` with a 9-step pipeline and a 3-stage prompt-too-long recovery cascade. Embodies "minimal scaffolding, maximal operational harness" with 98.4% deterministic infrastructure.
  - (source: https://claude-wiki.com/architecture.html)
- **Claude Code compaction**: 5-layer pipeline (budget reduction → snip → microcompact → context collapse → auto-compact), cheapest-first, before every model call. Four-level `CLAUDE.md` hierarchy with lazy loading + `@include` directives. Append-only JSONL transcripts with UUID-based boundary patching for resume.
  - (source: https://claudecode.io/architecture)
- **Claude Code structured output**: explicit `error_max_structured_output_retries` with model-fallback escalation, Zod schemas for 27 hook events, 3-failure circuit breaker on auto-compaction.
  - (source: https://claude-wiki.com/architecture.html)
- **Cloud Code / Gemini CLI**: event-based system (`GeminiEventType` enum) with `ChatCompressed`, `ContextWindowWillOverflow`, `LoopDetected` events. Uses `FunctionDeclaration` JSON Schema for tool params. Shared technology across CLI and IDE with `GEMINI.md` configuration.
  - (source: https://cloud.google.com/blog/products/application-development/gemini-cli-architecture)

### R5 — OpenCode / KiloCode Architecture

**Question:** Internal architecture of OpenCode and KiloCode, what we can port directly.

**Findings:**

- **KiloCode** V1 `packages/core/src/v1/session.ts` defines `StructuredOutputError` (with `retries: NonNegativeInt` field) and `OutputFormatJsonSchema` with `retryCount`. V1 session uses `llm.request` + `LLM.generate` (via Effect layer) with `maxRetries` and streaming event handling.
  - (source: claimed via GitHub KiloCode repo — **UNVERIFIED**, no concrete URL fetched)
- **OpenCode** core model in `@opencode-ai/schema/model` package. `Model.Info` includes `api: ProviderV2.Api` (with `options` for route selection — baseURL/headers), `capabilities` (parallelTool, cache, reasoning), and `variants`. Error schema is in `packages/llm/src/schema/errors.ts` (not `packages/llm/src/errors.ts`).
  - (source: claimed via GitHub OpenCode repo — **UNVERIFIED**, no concrete URL fetched)

### R6 — Free/NVIDIA Endpoint Quirks

**Question:** Capabilities and quirks of free/NVIDIA/OpenRouter/Groq/Together/DeepSeek endpoints.

**Findings:**

- **NVIDIA NIM / Build API** (`https://integrate.api.nvidia.com/v1`): supports `response_format` with `json_object`, `json_schema`, and `text`. Recommended approach for schema-constrained output: `extra_body={"nvext": {"guided_json": schema}}` with xgrammar (falls back to outlines). **Free tier ≈ 40 RPM baseline — a hard cap**, model- and traffic-dependent, **no per-key increases** (confirmed by NVIDIA staff on official forums). 429s can hit below 40 RPM on some models (e.g. Kimi K2.6). No usage/credit API — developers must log their own.
  - (source: https://docs.nvidia.com/nim/large-language-models/1.13.0/structured-generation.html — **VERIFIED**, https://forums.developer.nvidia.com/t/api-rate-limit-increase-is-not-granted)
- **Groq**: `json_schema` with `strict: true` (constrained decoding, guaranteed) only on `gpt-oss-20b`/`gpt-oss-120b`/`gpt-oss-safeguard-20b`; all other models only `json_object` (no schema guarantee). 30 RPM free, 1000 RPD most models, TPM 6K–30K. **Markdown code fence wrapping** confirmed in GitHub issues — Groq wraps JSON in fences even in JSON mode. Schema validation 400 errors with `failed_generation` field for raw output recovery. Escaped-character issues on Llama 4.
  - (source: https://console.groq.com/docs/structured-outputs, https://github.com/langchain-ai/langchain/issues/31459)
- **OpenRouter**: `json_schema` with `strict: true`, per-endpoint not per-model. `require_parameters: true` to force compliant routing. 20 RPM always; **50 requests/day** if never purchased credits (default free); 1,000 req/day if purchased ≥10 credits.
  - (source: https://openrouter.ai/docs/guides/features/structured-outputs, https://openrouter.ai/docs/api_reference/limits)
- **Together.ai**: `response_format` with `json_schema`. **No free trial — requires minimum $5 credit purchase** (official docs). 60 RPM free, dynamic rate limits, `dynamic_request_limited`/`dynamic_token_limited` 429 errors. **Discrepancy**: third-party source claims "1M tokens/month, no credit card required" — **official docs are authoritative** ($5 minimum).
  - (source: https://docs.together.ai/docs/billing-credits, https://docs.together.ai/docs/inference/chat/structured-outputs)
- **DeepSeek**: `response_format` supports only `json_object` (NOT `json_schema`). Requires literal word "json" in prompt. **Markdown code fence wrapping** confirmed in GitHub issues. Empty content on some calls (no error). 5M token one-time free grant (no credit card). 500 concurrent (v4-pro), 2500 (v4-flash). Truncation (`finish_reason: "length"`) produces invalid JSON. No `refusal` field.
  - (source: https://api-docs.deepseek.com/guides/json_mode)
- **DeepInfra**: `json_object` only, no `json_schema`.

### R7 — Schema & Prompt Design for Weak Models

**Question:** Best practices for getting weak/free LLMs to emit schema-conforming JSON.

**Findings:**

- **Minimal schemas** — smaller schemas reduce schema-drift and token overhead; complex nested schemas increase malformed-output probability.
- **Few-shot examples in the prompt** — include 1–3 complete examples of correct JSON output in the prompt to anchor format.
- **Explicit "JSON only, no markdown" instruction** — state clearly: "Return only valid JSON, no markdown fences, no prose, no explanations."
- **Temperature near 0** for deterministic structured output (0.0–0.2 typical).
- **`additionalProperties: false`** on schemas to prevent models from inventing extra fields.
- **Prompt should include the schema** in addition to any provider `response_format` — Together.ai and others recommend this for best results.
- **YAML-then-parse** as an intermediate — some recommend asking the model for YAML (more lenient grammar) then parsing to JSON.
- **Explicit field-name instructions** — name fields clearly in the prompt to reduce naming drift.
- **Handle refusals explicitly** — check stop_reason/finish_reason before parsing; never blind-retry a refusal.
- **Format collapse** is a real, measured phenomenon — weak models degrade under JSON mode pressure (Gemma-2B strict 0.116 vs. 0.246).
  - (source: https://arxiv.org/pdf/2601.06151)

### R8 — Pipeline Failure Semantics / Degradation

**Question:** What should a pipeline do when a stage's structured output fails — retry-with-repair, degrade, skip, abort?

**Findings:**

- **Three-tier strategy (AWS Well-Architected Agentic AI Lens AGENTREL07-BP02 + Clarion AI):** retry transient errors with bounded exponential backoff → fallback to alternative models/artifacts for persistent failures → escalate to human review for unrecoverable ones. Never apply uniform retry to every failure type.
  - (source: https://docs.aws.amazon.com/wellarchitected/latest/agentic-ai-lens/agentrel07-bp02.html, https://clarion.ai/insights-resilient-agentic-ai-pipelines-retry-fallback-human-in-the-loop/)
- **The Architecture of Errors** (arXiv:2605.30628, **VERIFIED**): within a bounded operational "patch," failures are sparse, repetitive, and cluster into ~8–20 named modes. Reliability becomes a local catalog-discovery problem. Implication: build an intervention catalog for the specific failure modes you actually see.
  - (source: https://arxiv.org/abs/2605.30628 — verified real, submitted 28 May 2026)
- **LangChain** defaults `handle_errors=True` (retry on validation errors via ToolMessage feedback); `with_fallback()` chains define backup strategies for graceful degradation. Recommends Pydantic validators → OutputFixingParser (1 retry) → `.with_retry` → backoff. "If fix rate > 20%, prompts need work."
  - (source: https://docs.langchain.com/oss/python/langchain/structured-output, https://github.com/langchain-ai/langchain/pull/33663)
- **Vercel AI SDK** `generateObject`/`streamObject` throw `AI_NoObjectGeneratedError` on parse/validate failure (hard abort). `maxRetries=3` only covers API errors (429/5xx), **NOT schema validation** — this was a recognized gap, addressed in PR #9083.
  - **WARNING**: PR #9083 was later noted (Jul 2026) as **deprecated** — `generateObject`/`streamObject` are now deprecated in favor of `generateText`/`streamText` with output. The retry-onError approach is superseded.
  - (source: https://github.com/vercel/ai/pull/9083 — **VERIFIED but STALE/SUPERSEDED**)
- **DSPy** `dspy.Assert` (hard constraint): backtracking retry to failing module with error in prompt; `max_backtracks=2` default, then halts with `AssertionError`. `dspy.Suggest` (soft): same retry mechanism, but after max_backtracks it logs `SuggestionError` and **continues** execution. `dspy.Refine`/`BestOfN` run up to N times and return best prediction.
  - **VERIFIED**: DSPy Issue #7693 confirms newer versions (post-litellim/adapter) **removed built-in Pydantic validation retries** — users must now explicitly wrap modules in `BestOfN` or `Refine`.
  - (source: https://dspy.ai/api/modules/Refine/, https://github.com/stanfordnlp/dspy/issues/7693 — **VERIFIED**)
- **GitHub Copilot**: Coding Agent opens a PR with partial work + explanation when stuck (graceful degradation). CLI driver retries with `--resume` on partial-session failures (3 attempts, 5s→10s→20s backoff, only when session produced output). Agent mode has per-edit checkpoint/rollback ("Restore") and auto-corrects from build/test/tool results.
  - (source: https://github.com/github/awesome-copilot, https://code.visualstudio.com/blogs/2025/02/24/introducing-copilot-agent-mode, https://github.com/github/gh-aw/pull/25329)
- **Cloudflare Agents SDK** `this.retry()`: 3 attempts, full-jitter exponential backoff (base 100ms, max 3000ms), applies to `queue()` and `schedule()` too.
  - (source: https://developers.cloudflare.com/agents/api-reference/retries/)
- **Error locality / bounded failure**:
  - AWS AGENTREL07-BP01: design staged workflows with incremental recovery — stages checkpoint independently so failure in one stage doesn't restart prior stages.
    - (source: https://docs.aws.amazon.com/wellarchitected/latest/agentic-ai-lens/agentrel07-bp01.html)
  - AGENTTRACE (arXiv:2603.14688): early-stage errors have disproportionate downstream impact; root-cause localization is step-level via backward causal-graph tracing.
    - (source: https://arxiv.org/abs/2603.14688)
  - ERRORPROBE (ACL 2026): "step-level localization" — detects anomalies at specific agent steps, backward-traces to origin.
    - (source: https://aclananthology.org/2026.findings-acl.98/)
  - COCO framework (arXiv:2508.13815): "Contextual Rollback" preserves per-node snapshots; on failure, only the erroneous node is rolled back and retried with error-context-augmented prompt.
    - (source: https://arxiv.org/abs/2508.13815)
  - DSPy LM Assertions (arXiv:2312.13382): error locality = assertion bound to a specific module/step; retry scoped to the failing component only.
    - (source: https://arxiv.org/abs/2312.13382)

---

## Disagreements, Verification Issues & Open Questions

### Verified claims (manual check)
- ✅ **arXiv:2605.30628 "The Architecture of Errors"** — confirmed real, submitted 28 May 2026, 5 authors, matches summarized claims.
- ✅ **DSPy Issue #7693 "[Feature] Retries on Pydantic Validation Errors"** — confirmed real open issue; matches claim that newer DSPy removed built-in validation retries.
- ✅ **NVIDIA NIM docs** — fetched and confirmed `nvext.guided_json` with xgrammar backend, Jan 2026 update.
- ✅ **Vercel AI PR #9083** — confirmed merged; PR text matches "maxRetries only handles model API errors... schema validation errors, JSON parsing errors."

### Stale / superseded
- ⚠️ **Vercel PR #9083** — while the PR is real and merged, a later comment (Jul 2026) states `generateObject`/`streamObject` are now **deprecated**, superseded by `generateText`/`streamText` with the `output` option, and a `repair` callback. The fix-mapping should reference the *current* API (`generateText` + `output` + `repair`), not the deprecated `generateObject`.
- ⚠️ **Together.ai "no free trial"** — official docs require $5 minimum, but third-party sources claim "1M tokens/month free." Official docs win, but the discrepancy suggests a recent policy change or regional difference.

### Unverifiable (single-source, contested topics)
- ❓ **R5 — KiloCode `StructuredOutputError`**: researcher cited GitHub repo path `packages/core/src/v1/session.ts` but no concrete commit URL was fetched. Cannot confirm `retries: NonNegativeInt` field exists in current source.
- ❓ **R5 — OpenCode `Model.Info`**: similarly unverified; no concrete source URL fetched.
- ❓ **R1 — Claude Code synthetic-tool trick**: claims come from a Substack architectural analysis (kenhuangus), not official Anthropic source. Plausible but not independently verifiable against the closed-source Claude Code binary. The native Anthropic structured-outputs API also exists, creating an apparent discrepancy the sources themselves note.
- ❓ **R1 — claudelab "one repair" recommendation**: a third-party blog article, not a primary source from a coding agent team. It's a well-reasoned opinion but single-source.

### Genuine disagreements
- **Structured-output retry budget**: claudelab recommends **one** repair; Instructor/LangChain/agentcast/Boundary default to **2–3**. Both agree on a bounded limit.
- **NVIDIA `guided_json` syntax**: latest NIM docs (2.0.4+) show `extra_body={"guided_json": schema}` (no `nvext`); older versions require `extra_body={"nvext": {"guided_json": schema}}`. Version-dependent — depends on NIM version.

### Open questions
- Whether Claude Code has migrated from the synthetic-tool trick to Anthropic's native `structured-outputs` beta API.
- Full list of OpenRouter free models that support `json_schema` (structured outputs) vs. only `json_object`.
- Whether NVIDIA's 40 RPM free tier applies per-model or account-wide.
- The exact JSON parsing/repair logic Claude Code uses internally (non-CLI) — source code not open.
- Whether Vercel's current `generateText`+`output`+`repair` API has been adopted as the stable path or remains experimental.

---

## What We Should Adopt (Fix Mapping)

The following table maps each proven practice → specific files in our repo → priority. This is the basis for a subsequent implementation plan.

| # | Proven Practice | Our File(s) | Priority | What Changes |
|---|---|---|---|---|
| 1 | **Provider-native structured output** (constrained decoding) instead of free-text + regex extraction | `src/llm/provider.rs` — extend `LlmProvider` trait with optional schema-aware completion; `src/llm/{anthropic,openai,google,ollama}.rs` — implement per-provider response_format/JSON mode / `guided_json` for NIM | **P0** | Add `request_structured()` to the trait. For NVIDIA NIM: pass `extra_body={"nvext":{"guided_json":schema}}` or `{"guided_json":schema}`. For OpenAI: `response_format={"type":"json_schema","json_schema":{"schema":...}}`. For Anthropic: use the synthetic-tool trick or native beta. For Gemini: `response_schema`. This directly addresses the root cause — free LLMs that ignore prompts still emit valid schema-conforming JSON when constrained at the API level. |
| 2 | **Layered recovery stack** (local repair → re-prompt) in `run_agent` | `src/agents/mod.rs` lines 71-105 (retry loop), 155-170 (validation/extraction) | **P0** | Add a *second* retry phase that triggers specifically on `ParseError` / `ValidationError` / `NO_JSON` / `TRUNCATED`. On failure: (a) attempt local deterministic repair (#3), (b) if still invalid, feed the validation error + bad output back to the LLM with an explicit repair prompt and retry budget. Make budget **configurable (default 2)**, distinguishing retryable (malformed JSON, schema mismatch, truncation) from permanent (refusal, auth, 401). |
| 3 | **Resilient JSON parsing** (json_repair-style salvage) | `src/agents/mod.rs::extract_json`, `src/artifacts/validate.rs` | **P0** | Replace naive `extract_json` (first `{` → last `}`) with: (a) strip markdown code fences, (b) extract JSON from surrounding prose/thinking blocks, (c) fix trailing commas / single quotes / unescaped control chars / Python literals, (d) partial-JSON prefix extraction for streaming. Return structured error with field-level detail (which keys failed). |
| 4 | **Pipeline degradation** (don't hard-abort) | `src/orchestrator/pipeline.rs::run_stage` (line 448-476) | **P1** | On persistent `ArtifactValidation` after retry budget exhausted: emit a **partial artifact** (best-effort parsed object with defaults/nulls for missing fields) and mark the stage as "degraded" with a warning — **do not** `?`-propagate abort. Surface the failure to the user with context. Only hard-abort on permanent errors (auth, refusal, schema-unsupported-by-provider). |
| 5 | **Local deterministic repair library** (the "cheaper rung of the ladder") | New module `src/llm/repair.rs` (or integrate `serde_json` + salvage logic) | **P0** | Steal the proven strategies from outputguard/json-repair: ordered list of 15 local repairs (strip fences #1, extract from prose, fix trailing commas, single→double quotes, close brackets, escape control chars, normalize Python literals). **Do NOT** fabricate truncated JSON by closing braces — re-run with more budget instead. |
| 6 | **Prompt & schema design for weak models** | `src/llm/templates/*.md` (prompt templates), `schemas/*.schema.json` | **P1** | Add explicit "JSON only, no markdown fences, no prose" instruction to all agent prompts. Use `additionalProperties: false` on schemas. Lower temperature for repair passes. Include schema in prompt as well as response_format. Minimal schemas for weak models. |
| 7 | **Error locality / bounded failure** | `src/orchestrator/pipeline.rs`, `src/orchestrator/state.rs` | **P2** | Per-stage checkpointing (already partially present via StageMetric). On stage failure, don't restart prior stages. Build an intervention catalog of the ~8-20 specific failure modes we actually observe (per Architecture of Errors paper), not hypothetical ones. |
| 8 | **Circuit breaker at provider level** | `src/llm/provider.rs` (orchestration layer) | **P2** | After 3 consecutive provider failures (hard errors, not validation), escalate to a fallback provider — mirroring the layered strategy and Cloudflare's `this.retry()` pattern. |

### Implementation ordering (recommended)

1. **P0 items first** (#1 provider structured output, #3 resilient parsing, #5 local repair, #2 layered recovery) — these together eliminate the stall at its root. A free LLM that emits malformed JSON now gets: local repair → if that fails, schema-constrained re-prompt with error feedback → if that fails, graceful degradation.
2. **P1 items** (#4 pipeline degradation, #6 prompt/schema hardening) — these make the degradation graceful and prevent recurrence.
3. **P2 items** (#7 error locality, #8 circuit breaker) — these are architectural maturity improvements.

### Key numbers to adopt
- **Retry budget**: 3 attempts for transient API errors (existing); **2 attempts** for structured-output repair (configurable; industry range 1-3).
- **Backoff**: exponential with jitter for transient; minimal/no delay for repair passes.
- **3-attempt circuit breaker** for provider-level escalation (matches Cloudflare/AWS norms).

---

## Source List (all URLs cited)

### Primary sources (verified / official)
- NVIDIA NIM Structured Generation: https://docs.nvidia.com/nim/large-language-models/1.13.0/structured-generation.html
- Anthropic Structured Outputs: https://docs.claude.com/en/docs/build-with-claude/structured-outputs
- Anthropic Claude Cookbook (JSON mode): https://platform.claude.com/cookbook/misc-how-to-enable-json-mode
- Anthropic Stop Reason Handling: https://platform.claude.com/docs/en/build-with-claude/handling-stop-reasons
- Groq Structured Outputs: https://console.groq.com/docs/structured-outputs
- OpenRouter Structured Outputs: https://openrouter.ai/docs/guides/features/structured-outputs
- OpenRouter Limits: https://openrouter.ai/docs/api_reference/limits
- OpenRouter FAQ: https://openrouter.ai/docs/faq
- Together.ai Structured Outputs: https://docs.together.ai/docs/inference/chat/structured-outputs
- Together.ai Billing: https://docs.together.ai/docs/billing-credits
- Together.ai Rate Limits: https://docs.together.ai/docs/serverless/rate-limits
- DeepSeek JSON Mode: https://api-docs.deepseek.com/guides/json_mode
- DeepInfra JSON Mode: https://deepinfra.com/blog/json-mode
- vLLM Structured Outputs: https://docs.vllm.ai/en/latest/features/structured_outputs/
- SGLang Structured Outputs: https://docs.sglang.ai/advanced_features/structured_outputs.html
- OpenAI Structured Outputs: https://openai.com/index/introducing-structured-outputs-in-the-api/
- Gemini Structured Outputs: https://ai.google.dev/gemini-api/docs/structured-output
- LangChain Fix Parsers: https://github.com/langchain-ai/langchain/blob/e09699298428d1a2e23193e4074f9c9a99413c1c/libs/langchain/langchain_classic/output_parsers/fix.py
- LangChain Structured Output DOcs: https://docs.langchain.com/oss/python/langchain/structured-output
- LangChain Issue #31459: https://github.com/langchain-ai/langchain/issues/31459
- LangChain Issue #8068 / PR #8091: https://github.com/langchain-ai/langchainjs/issues/8068
- LangChain PR #33663: https://github.com/langchain-ai/langchain/pull/33663
- Vercel AI SDK Error Handling: https://ai-sdk.dev/docs/ai-sdk-core/error-handling
- Vercel AI SDK Generating Structured Data: https://ai-sdk.dev/docs/ai-sdk-core/generating-structured-data
- Vercel AI SDK Error Recovery: https://vercel-ai.mintlify.app/advanced/error-recovery
- Vercel AI PR #9083: https://github.com/vercel/ai/pull/9083
- DSPy JSONAdapter: https://github.com/stanfordnlp/dspy/blob/main/dspy/adapters/json_adapter.py
- DSPy Refine: https://dspy.ai/api/modules/Refine/
- DSPy Errors: https://dspy.ai/api/utils/Errors/
- DSPy Issue #7693: https://github.com/stanfordnlp/dspy/issues/7693
- DSPy PR #8031: https://github.com/stanfordnlp/dspy/pull/8031
- Cloudflare Agents Retries: https://developers.cloudflare.com/agents/api-reference/retries/
- AWS Well-Architected Agentic AI Lens: https://docs.aws.amazon.com/wellarchitected/latest/agentic-ai-lens/agentrel07-bp02.html (and BP01, BP03)
- Instructor Retry Mechanisms: https://python.useinstructor.com/learning/validation/retry_mechanisms/
- Instructor Retrying: https://python.useinstructor.com/concepts/retrying/
- Pydantic AI Output: https://pydantic.dev/docs/ai/core-concepts/output/
- DeepSeek AI (unofficial guide): https://deepseekai.guide/api/deepseek-api-json-mode
- Dreaming.press recovery playbook: https://dreaming.press/posts/when-structured-output-breaks-repair-recovery-playbook.html
- Dreaming.press truncated responses: https://dreaming.press/posts/how-to-handle-a-truncated-llm-response.html
- Claudelab repair loop: https://claudelab.net/en/articles/api-sdk/claude-api-structured-output-schema-validation-repair-loop
- Claudelab stop reason handling: https://claudelab.net/en/articles/api-sdk/claude-api-stop-reason-handling-guide
- Claude Wiki architecture: https://claude-wiki.com/architecture.html
- Claude Wiki error handling: https://claude-wiki.com/error-handling-and-recovery.html
- ClaudeCode.io architecture: https://claudecode.io/architecture
- OpenCode GitHub: https://github.com/opencode-ai/opencode
- OpenCode PR #23064 (repairToolCall): https://github.com/anomalyco/opencode/pull/23064
- OpenCode issues #10456, #9320: https://github.com/anomalyco/opencode/issues/10456
- KiloCode GitHub: https://github.com/Kilo-Org/kilocode
- Cline Inside: https://medium.com/@floralan212/inside-cline-how-its-agentic-chat-system-really-works-3d582935efa5
- Aider code-in-JSON: https://aider.chat/2024/08/14/code-in-json.html
- Aider base coder: https://github.com/Aider-AI/aider/blob/5dc9490b/aider/coders/base_coder.py
- Cline claude-code provider: https://github.com/cline/cline/blob/9dea336c/src/core/api/providers/claude-code.ts
- OpenRouter free router: https://openrouter.ai/openrouter/free
- OpenRouter pricing: https://openrouter.ai/pricing
- TokenMix Groq free tier: https://tokenmix.ai/blog/groq-free-tier-limits-2026
- Eeseel AI Groq pricing: https://eesel.ai/blog/groq-pricing
- GitHub Copilot awesome-copilot: https://github.com/github/awesome-copilot/blob/8d91380b/website/src/content/learning-hub/using-copilot-coding-agent.md
- VS Code Copilot Agent Mode: https://code.visualstudio.com/blogs/2025/02/24/introducing-copilot-agent-mode
- GH Actions Copilot (IT-Journey): https://it-journey.dev/quests/1001/agentic-safe-execution-and-error-handling/
- Copilot Auto-Retry extension: https://marketplace.visualstudio.com/items?itemName=MaximMazurok.vscode-copilot-auto-retry
- gh-aw PR #25329: https://github.com/github/gh-aw/pull/25329
- Clarion AI resilient pipelines: https://clarion.ai/insights-resilient-agentic-ai-pipelines-retry-fallback-human-in-the-loop/
- AiTechWorlds LangChain output fixer: https://www.aitechworlds.com/category/agent-development/langchain-output-fixer-retry-malformed-responses/
- Geeky Codes LangChain fixer: https://geekycodes.in/generative-ai/how-to-fix-langchain-outputparserexception-in-production-llm-pipelines/
- Easton Dev error taxonomy: https://eastondev.com/blog/en/posts/ai/20260506-llm-structured-output/
- Boundary docs: https://docs.withboundary.com/concepts/repair-loop
- Boundary CLAUDE Lab article: https://claudelab.net/en/articles/api-sdk/claude-api-structured-output-schema-validation-repair-loop
- OutputGuard: https://github.com/ndcoder/outputguard
- AgentJSON (Rust): https://github.com/sigridjineth/agentjson
- JSON Sanity: https://github.com/peak-agent/json-sanity-public
- QAgents recovery: https://github.com/Quanted-AI/QAgents/blob/main/quanted_agents/recovery.py
- AI Skill Certs multi-agent error handling: https://aiskillcerts.com/concepts/agentic-architecture/multi-agent-error-handling-and-routing
- Geodocs retry strategy: https://geodocs.dev/ai-agents/agent-retry-strategy-spec
- arxiv 2605.30628 (Architecture of Errors): https://arxiv.org/abs/2605.30628
- arxiv 2603.14688 (AGENTTRACE): https://arxiv.org/abs/2603.14688
- ACL 2026 ERRORPROBE: https://aclananthology.org/2026.findings-acl.98/
- arXiv 2508.13815 (COCO): https://arxiv.org/abs/2508.13815
- arXiv 2312.13382 (DSPy LM Assertions): https://arxiv.org/abs/2312.13382
- PromptPort canonicalizer: https://arxiv.org/pdf/2601.06151
- FixJSON guides: https://fixjson.org/guides/repair-llm-json-output
- json_repair: https://raw.githubusercontent.com/mangiucugna/json_repair/main/README.md
- jsonrepair: https://github.com/josdejong/jsonrepair
- repair-json-stream: https://github.com/prxtenses/repair-json-stream
- partial-json: https://www.npmjs.com/package/partial-json
- LangChain json utils: https://raw.githubusercontent.com/langchain-ai/langchain/master/libs/core/langchain_core/utils/json.py
- Vercel parsePartialJson: https://raw.githubusercontent.com/vercel/ai/main/packages/ai/src/util/parse-partial-json.ts
- Vercel fixJson: https://raw.githubusercontent.com/vercel/ai/main/packages/ai/src/util/fix-json.ts
- Vercel repair JSON cookbook: https://ai-sdk.dev/cookbook/node/repair-json-with-jsonrepair
- DSPy.rb extraction: https://oss.vicente.services/dspy.rb/blog/articles/under-the-hood-json-extraction/
- Anthropic Substack (Ken Huang): https://kenhuangus.substack.com/p/chapter-15-structured-output-and
- Thomas Wiegold Claude structured outputs: https://thomas-wiegold.com/blog/claude-api-structured-output/
- arxiv 2604.03515 (Cline/arXiv taxonomy): https://arxiv.org/html/2604.03515v2
- Red Hat structured outputs vLLM: https://developers.redhat.com/articles/2025/06/03/structured-outputs-vllm-guiding-ai-responses
- Dreaming.press: https://dreaming.press/
- Vibrant Labs Groq: https://the-neuralbase.com/langchain/learn/intermediate/handling-partial-or-invalid-structured-output/ (note: URL path is a heuristic reconstruction — may not resolve exactly)

### Secondary / non-verified sources (used for context only)
- Substack architectural analyses (kenhuangus) — third-party, plausibly accurate but not from agent teams.
- Third-party blogs (claudelab, eastondev, the-neuralbase, tokenmix, eeseel) — informative but not primary.
- DeepSeek unofficial guide (deepseekai.guide) — not official DeepSeek docs.

### Sources flagged as stale or superseded
- Vercel `generateObject`/`streamObject` API and PR #9083 — superseded by `generateText`/`streamText` with `output` + `repair` (Jul 2026).

### Sources that could not be verified
- KiloCode `packages/core/src/v1/session.ts` (StructuredOutputError claim) — no concrete GitHub URL fetched.
- OpenCode `packages/core/src/model.ts` (Model.Info structure) — no concrete GitHub URL fetched.
- Claude Code `queryLoop()` source — described via third-party claudecode.io analysis, not from open-source code.
