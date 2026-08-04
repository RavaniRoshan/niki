# Implementation Plan: Structured-Output Robustness (Fix the Pipeline Stall)

## Derived from
`research/coding-agent-structured-output-architecture.md` + `research/features-fixes-tracking.md`

## Problem
`run_agent` (`src/agents/mod.rs`) only retries *transient network errors*. When a free LLM (NVIDIA endpoint) returns malformed or schema-invalid JSON, `extract_json` fails or `validate_artifact` (`src/artifacts/validate.rs`) fails hard, and the `?` propagates through `run_stage` (`src/orchestrator/pipeline.rs:461`) — **the entire stage and pipeline stalls**. No repair, no re-prompt, no degradation.

## Decided Defaults (user deferred to research-backed judgment)

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Repair retry budget | **2** | claudelab: 1 (diminishing returns, 87.4% first-try). agentcast: attempt 2 adds +9.1%, attempt 3 +2.8%. 2 captures 96.5%. |
| Provider structured output order | **NVIDIA first, then OpenAI-compatible fallback** | We currently use NVIDIA free tier (40 RPM hard cap). Implement `guided_json` → fall back to `json_object` if schema unsupported. Other providers (Anthropic/Gemini) get basic `response_format` support. |
| Pipeline degradation | **Partial artifact + warning** | Proven by GH Copilot (partial PR), DSPy Suggest (continue after max_backtracks). Hard-abort ONLY on permanent errors (refusal, auth). |
| JSON repair library | **Custom implementation** | No mature Rust port of `json_repair` exists. We control the 15 repair strategies directly. Avoid deps; keep in-tree. |
| Implementation scope | **P0 + P1** (F-A through F-G); **P2 items deferred** (F-H through F-L) | P0-P1 fixes the stall. P2 (circuit breaker, error locality, checkpoints, streaming partial JSON, field-level validation) are architectural maturity — can be a follow-up. |

## Phases

### Phase 1: Structured Output at the Provider Layer (F-A, P0)

**Goal:** Constrain LLM output at the API level so weak/free models can't emit non-conforming JSON.

**Changes:**
1. **`src/llm/provider.rs`** — Extend `LlmProvider` trait:
   - Add `fn supports_structured_output(&self) -> bool` (default `false`)
   - Add `fn request_structured(&self, model: &str, schema: &serde_json::Value, ...) -> Result<String>` (default: delegates to existing `request()`)
2. **`src/llm/nvidia.rs`** (NEW) — implement `supports_structured_output() -> true`:
   - Send `extra_body={"nvext": {"guided_json": schema}}` (xgrammar-constrained decoding)
   - Fallback: if provider returns error mentioning schema syntax, retry with plain `response_format={"type":"json_object"}`
   - Rate limit: 40 RPM free tier → this is why constrained decoding is critical (fewer retries needed)
3. **`src/llm/openai.rs`** — implement structured output:
   - `response_format={"type":"json_schema","json_schema":{"schema":..., "strict":true}}` on supported models
   - Fallback to `{"type":"json_object"}` on unsupported
4. **`src/llm/anthropic.rs`** — implement structured output:
   - Synthetic-tool trick (wrap schema as tool named `StructuredOutput`, force `tool_choice`)
   - OR native `output_format` beta if available
5. **`src/llm/google.rs`** — implement structured output:
   - `response_format={"type":"json","schema":...}`
6. **`src/llm/ollama.rs`** — no structured output support; mark `supports_structured_output() -> false`, fall back to prompt-only JSON instruction

**Verify:** `cargo check`, unit test `supports_structured_output()` on each provider.

### Phase 2: Resilient JSON Parsing (F-C, F-D, P0)

**Goal:** Replace naive `extract_json` with a repair-capable parser.

**Changes:**
1. **NEW `src/llm/repair.rs`** — local deterministic repair library:
   - `fn repair_json(input: &str) -> Result<String, NikiError>` — ordered strategies:
     1. Try strict `serde_json::from_str` (fast path)
     2. Strip markdown code fences (```` ```json ... ``` ````, ```` ``` ... ``` ````)
     3. Extract JSON from surrounding prose (find outermost `{`/`}`, extract embedded object)
     4. Fix trailing commas before `}`/`]`
     5. Convert single quotes to double quotes
     6. Escape control characters in strings
     7. Normalize Python literals (`True/False/None` → `true/false/null`)
     8. Try partial prefix parse for truncated streams (best valid prefix)
   - **Never** fabricate values by closing braces — return the parse error with context.
   - **Never** silently repair JSON that will drive destructive actions (per fixjson.org fail-loudly).
2. **`src/agents/mod.rs::extract_json`** — replace naive implementation:
   - Call `repair_json()`; if it returns a valid parse, use it.
   - On failure, return structured error with field-level detail.
   - For streaming: yield partial-JSON prefix on each chunk (best-effort; final value strictly parsed).
3. **`src/llm/mock.rs`** — update `MockProvider` to optionally return malformed JSON (to test repair path).

**Verify:** New `tests/repair_json.rs` unit tests covering all 8 repair strategies + malformed edge cases.

### Phase 3: Layered Recovery in `run_agent` (F-B, F-G, P0)

**Goal:** Add a Phase 2 retry that triggers on structured-output failures, with failure-type classification.

**Changes:**
1. **NEW `src/agents/errors.rs`** — failure taxonomy:
   - `enum OutputFailure { Truncated, ParseError, ValidationError { fields: Vec<String> }, NoJson, Refusal, AuthError, NetworkError, Unknown }`
   - `fn classify(&self, response: &CompletionResponse) -> OutputFailure` — reads stop_reason/finish_reason BEFORE parsing
   - `fn is_retryable(&self) -> bool` — Refusal/AuthError = permanent; others = retryable
2. **`src/agents/mod.rs` retry loop (L71-105)** — restructure:
   - Phase 1 (existing): retry transient API errors (429/503/timeout/network) — keep, 3 attempts, exponential backoff with jitter
   - Phase 2 (NEW): on `ParseError`/`ValidationError`/`NoJson`/`Truncated`:
     1. Local repair via `repair_json()` (cheaper than LLM call)
     2. If repair produces valid schema-conforming JSON → use it
     3. If still failing → re-prompt the LLM with: "The following JSON did not match the schema. Errors: [field-level details]. Bad output: [output]. Fix and respond with valid JSON only."
     4. Budget: **2 attempts** (configurable via env `NIKI_REPAIR_RETRIES`, default 2)
     5. On TRUNCATED: double `max_tokens` and re-run (don't just re-prompt)
3. **Full-jitter backoff** (`src/agents/mod.rs`) — update existing backoff:
   - Transient: `delay = random(0, min(2^attempt * 500ms, 32s))`, max 3 attempts
   - Repair: minimal delay (repair passes are cheap; no backoff needed), max 2 attempts

**Verify:** `tests/repair_retry.rs` — mock LLM returning schema-invalid JSON, assert it's repaired then re-prompted, then degrades.

### Phase 4: Graceful Pipeline Degradation (F-E, F-F, P1)

**Goal:** When repair budget is exhausted, degrade gracefully instead of hard-aborting the pipeline.

**Changes:**
1. **`src/orchestrator/pipeline.rs::run_stage` (L448-476)** — on `ArtifactValidation` after budget exhausted:
   - Attempt partial artifact: run `repair_json()` on the raw output (best-effort parse with nulls for missing fields)
   - Construct a `PipelineResult` with `status = "degraded"` and the partial artifact
   - Log a warning via `tracing::warn!` (no raw content — F8 secret-safe)
   - Continue the pipeline (do NOT `?`-propagate)
   - Only `?`-propagate on permanent failures (REFUSAL, AuthError — check `is_retryable()`)
2. **`src/llm/templates/*.md`** — add to every agent prompt:
   - "Return ONLY valid JSON matching the schema below. No markdown fences. No prose. No explanations."
3. **`schemas/*.schema.json`** — for each schema:
   - Add `"additionalProperties": false` to object definitions
   - Add `"description"` to each field for the LLM
   - (Minimal schemas already in place — no change needed if schemas are already minimal)

**Verify:** `tests/pipeline_degradation.rs` — mock LLM returning persistently invalid JSON, assert pipeline continues with degraded artifact, not abort.

### Phase 5: Field-Level Validation Detail (F-K, P1 → folded into Phase 2/3)

**Goal:** Provide per-field violation detail so repair prompts are targeted (like Boundary's per-rule repair messages).

**Changes:**
1. **`src/artifacts/validate.rs`** — replace top-level-only `is_valid` with `validate_detailed`:
   - Use `jsonschema::Validator::iter_errors()` instead of `is_valid()`
   - Return `{ field: "path.to.field", error: "type mismatch, expected string", raw_value: ... }` per field
   - Feed this to the repair re-prompt: "Fix these specific violations: [list]"

**Verify:** Unit test `validate_detailed` returns field-level errors on malformed input.

### Phase 6: Tests

**New test files:**
1. `tests/repair_json.rs` — 8 tests (one per repair strategy + edge cases)
2. `tests/repair_retry.rs` — mock LLM returns schema-invalid JSON → assert repair → re-prompt → degrade flow
3. `tests/pipeline_degradation.rs` — pipeline continues with degraded artifact on persistent failure
4. `tests/structured_output.rs` — `supports_structured_output()` per provider; mock structured output path
5. Update `tests/llm_mock_provider.rs` — add `MockProvider` config to return malformed/misformatted JSON

**Update existing:**
- `tests/common/harness.rs` — add helpers for malformed JSON scenarios
- `tests/common/mod.rs` — re-export new test helpers

## Files Touched (Summary)

```
src/llm/provider.rs          — extend LlmProvider trait (F-A)
src/llm/nvidia.rs            — NEW: NVIDIA NIM structured output (F-A)
src/llm/openai.rs            — structured output support (F-A)
src/llm/anthropic.rs         — structured output support (F-A)
src/llm/google.rs            — structured output support (F-A)
src/llm/mock.rs              — support malformed JSON return (F-C)
src/llm/repair.rs            — NEW: local JSON repair library (F-D)
src/agents/mod.rs            — layered recovery, backoff, structured output dispatch (F-B, F-G, F-J)
src/agents/errors.rs         — NEW: failure taxonomy + classification (F-G)
src/artifacts/validate.rs    — field-level validation detail (F-K)
src/orchestrator/pipeline.rs — graceful degradation (F-E)
src/llm/templates/*.md       — "JSON only no markdown" instruction (F-F)
schemas/*.schema.json        — additionalProperties: false (F-F)
```

## Out of Scope (P2 — deferred to follow-up)

- F-H: Per-stage checkpointing / error locality catalog (AWS AGENTREL07-BP01)
- F-I: Provider-level circuit breaker with fallback escalation
- F-L: Streaming partial-JSON for live UI (partial-json for streaming chunks)
- F-J: Full-jitter backoff (Phase 3 only implements it for new retry paths; existing backoff kept)

## Success Criteria

- [ ] `cargo check` passes with no new warnings
- [ ] `cargo test` passes (all existing 212 tests + new tests)
- [ ] NVIDIA NIM provider returns schema-conforming JSON via `guided_json` in integration test
- [ ] `repair_json()` handles all 8 repair strategies (unit tested)
- [ ] `run_agent` Phase 2: on schema-invalid output → local repair → re-prompt → partial artifact (NOT hard-abort)
- [ ] Pipeline continues with "degraded" artifact when repair budget exhausted (NOT stall)
- [ ] All new code paths are secret-safe (F8): no raw LLM output logged, only lengths and field names
