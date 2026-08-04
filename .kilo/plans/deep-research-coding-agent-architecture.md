# Deep Research — Coding-Agent Architecture & Structured-Output Robustness

## Goal

Run the deep-research workflow end-to-end (decompose → parallel fan-out → adversarial verify → resolve → synthesize) to answer one core decision and one broad question:

1. **Core decision (the bug we must fix):** How do mature, proven coding agents keep their pipeline from stalling when a (weak/free) LLM returns JSON that doesn't match the expected output format? We ship strict, all-or-nothing schema validation with no repair, retry, or structured-output path — and free LLMs (NVIDIA endpoints, etc.) break it.
2. **Broad question:** What is the internal architecture of proven coding agents (Claude Code, Cloud Code, OpenCode, KiloCode) and which parts should we adopt to fix things we got wrong and add missing features?

## Problem statement (grounded in our code)

- `src/agents/mod.rs:71-105` — the retry loop **only retries transient network errors** (timeout/rate/429/503/network). It never retries on malformed JSON or schema-validation failure.
- `src/agents/mod.rs:155-170` — naive `extract_json` (code fence or first `{`→last `}`) then strict `validate_artifact`; any failure returns `NikiError::ArtifactValidation`.
- `src/artifacts/validate.rs:13` — all-or-nothing `jsonschema::is_valid`; no partial salvage; error only lists top-level keys.
- `src/orchestrator/pipeline.rs:461` + `src/orchestrator/pipeline.rs:497-499` — `run_agent(...).await?` propagates the validation failure up, stalling the entire stage and the pipeline.
- **No provider-native structured output** (JSON mode, function/tool calling, grammar-constrained decoding) is used anywhere. `src/llm/provider.rs` completes/streams free text only.

This is exactly why free LLMs stall us: they return *valid but different* JSON (or lightly malformed), strict validation fails hard, and there is no repair/retry/degrade path.

## Confirmed scope

- **Full architecture survey** — all 8 sub-questions (the JSON stall is the anchor; we also map each mature agent's internal architecture).
- **Research + fix mapping** — the report ends with a "what we should adopt" section mapping each proven practice to specific files (`run_agent`, `validate.rs`, `llm/provider.rs` + each provider, `pipeline.rs`, `mock.rs`) and a prioritized feature list.

## Research workflow

This plan only *designs* the research. All actual searching happens inside subagents during implementation so main context stays clean. The implementation will follow the steps below mechanically.

---

### Step 1 — Decompose (8 independent sub-questions)

Each is answerable without needing another sub-question's answer first:

1. **Structured-output mechanisms.** What do Claude Code / OpenCode / Cline / Aider use to get *guaranteed* JSON from an LLM — provider JSON mode, function/tool calling, grammar-constrained decoding (outlines / guidance / xgrammar), or prompt-only? Which mechanisms work on free / NVIDIA NIM endpoints?
2. **Repair & retry on validation failure.** How do mature agents handle schema-invalid or malformed JSON? Self-healing loops (send the error + schema back to the model to fix), fallback prompts, re-prompt budgets, backoff strategies?
3. **Resilient JSON extraction/parsing.** What tooling exists beyond naive fence-stripping — `json_repair`, partial-JSON salvage, streaming parsers, handling code fences / trailing prose / markdown-table pseudo-JSON? What does Claude Code's own output parsing do?
4. **Claude Code / Cloud Code internal architecture.** The proven commercial agents: agent loop shape (perceive → tool use → validate → critique → retry), the "agent harness" pattern, schema/prompt versioning, session & context management (compaction, summarization).
5. **OpenCode / KiloCode internal architecture.** Both are open source and readable — examine their provider abstraction, structured-output handling, error resilience, and what we can port directly.
6. **Free / NVIDIA endpoint quirks.** Known behavior of free LLM endpoints (NVIDIA NIM build API, OpenRouter free tier, Groq, Together): which actually support JSON mode / structured output, how their JSON responses differ (field naming, nesting, extra prose), and rate limits.
7. **Schema & prompt design for weak models.** Best practices to coax schema-conforming JSON from weak models: minimal schemas, few-shot examples, "JSON only, no markdown" instruction, `additionalProperties: false`, temperature, YAML-then-parse as an intermediate.
8. **Pipeline failure semantics / degradation.** What should a pipeline do when a stage's structured output fails — retry with repair, degrade to partial artifacts, skip-with-warning, or abort? What do production frameworks (LangChain, Vercel AI SDK, DSPy) recommend?

### Step 2 & 3 — Fan out (parallel research)

Spawn 8 subagents via the `Task` tool, **all in parallel** (single message, multiple `task` calls), subagent_type `general`. Each gets this exact brief (substituting the sub-question):

> You are a research subagent. Research ONLY: "<sub-question N>"
> 1. Run 3-6 targeted web searches, varying phrasing each time — don't repeat the same query.
> 2. Fetch the 3-5 most relevant sources in full. Don't rely on search snippets alone.
> 3. Extract only claims directly relevant to the question, each with its source URL attached.
> 4. If sources disagree, say so explicitly — don't silently pick a side.
> 5. Return exactly this shape:
>    - **Question:** ...
>    - **Findings:** bullets, each ending in (source: URL)
>    - **Confidence:** high / medium / low — one line why
>    - **Open questions:** anything you couldn't resolve
>
> Do not write files. Do not speculate beyond the evidence.

Wait for every subagent to return before moving on. No primary research in main context.

### Step 4 — Adversarial verify

Spawn exactly **one** subagent (subagent_type `general`), passing it **all 8 findings together**, with this brief:

> You are a verification subagent. You'll receive research findings from multiple other agents.
> 1. For every claim: is it actually supported by its cited source, or is it inference dressed up as fact?
> 2. Flag any two findings that contradict each other.
> 3. Flag any source too old for how fast this topic moves.
> 4. Flag any claim resting on a single source where the topic is genuinely contested.
> Return a list of issues, or "No issues found." Be blunt — this step exists to catch what the researchers got wrong or missed, not to rubber-stamp them.

### Step 5 — Resolve

For every verifier issue: either **fix it** by re-querying the relevant sub-question (one targeted re-fan-out), or **carry it** into the report as an explicit stated limitation. Never silently drop a flagged issue.

### Step 6 — Synthesize

Write one markdown report to `./research/coding-agent-structured-output-architecture.md`:

- **Executive summary** (3-5 sentences)
- **Findings**, organized by sub-question
- **Disagreements & open questions** (from steps 4-5, including carried limitations)
- **What we should adopt (fix mapping)** — table mapping each proven practice → specific files in our repo → priority (P0/P1/P2):
  - Structured output via provider JSON mode / constrained decoding → `src/llm/provider.rs`, each provider, `src/llm/mock.rs`
  - Repair-retry loop on validation failure (self-healing) → `src/agents/mod.rs` `run_agent` (reuse existing retry scaffolding), `src/orchestrator/pipeline.rs`
  - Resilient parsing (`json_repair`-style salvage, field-level error detail) → `src/agents/mod.rs::extract_json`, `src/artifacts/validate.rs`
  - Pipeline degradation (skip warning vs. abort) → `src/orchestrator/pipeline.rs::run_stage`
- **Full source list** (all URLs cited by researchers and verifier)

Report location: `./research/coding-agent-structured-output-architecture.md` (create `./research/` if absent).

---

## Success criteria

- 8 subagents return findings in the required shape; 1 verifier returns issue list or "No issues found."
- Every verifier issue is resolved (fixed by re-fan-out) **or** recorded as a stated limitation — none silently dropped.
- Report exists at `./research/coding-agent-structured-output-architecture.md` with all sections above.
- Fix-mapping table enumerates concrete, file-level, prioritized actions — forming the basis for a subsequent implementation plan (outside this research task).

## Runbook (implementation phase, after plan approval)

1. `mkdir -p research` if absent.
2. Launch 8 `general` subagents in parallel (Step 2/3 briefs) — single message.
3. Collect all 8 results into main context.
4. Launch 1 verification subagent with all 8 findings (Step 4 brief).
5. Resolve issues (Step 5) — targeted re-fan-out for fixable ones.
6. Write and save the report (Step 6).
7. Report back with report path + a short summary of findings and the prioritized fix list.