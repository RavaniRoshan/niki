# NIKI Agent-Harness Plan — Full 7-Layer Conformance

Selected objective: **Full 7-layer conformance**. This means hardening plus building the missing framework pieces: converged/vector-backed storage, an executable MCP-backed tool loop as a supported path, skill distillation/promotion with versioning and retirement, and a unified hysteresis/step-cost budget. It is not verification-only and not a UI/model-quality project.

## Progress Tracker (Ralph-loop source of truth)

Legend: `[x]` completed + verified · `[~]` in progress · `[ ]` not started. Every `[x]` requires the phase-exit verification green; otherwise revert to `[~]`. The loop runs plan → work → verify → mark, two full repetitions minimum, until no further tightening is possible and the product is best-in-class (Claude Code league) with end-to-end `niki run` working.

| Phase | Status | Scope | Exit gate |
|---|---|---|---|
| Phase 0 — Ground-truth re-grep | [x] | Zero-caller grep table recorded | No files changed, notes only — DONE 2026-09-20 |
| Phase 1 — Contract truth (1.1–1.7) | [x] | Synthesis/Critic/mcp_tools/drift/roundtrip/degrade/docs | `fmt --check` + `clippy` clean + `cargo test` 611 green + rebuild — DONE 2026-09-20 |
| Phase 2 — Layer 1+2 foundation (2.1–2.5) | [x] | Atomic stores, bounded render, AGENTS.md hierarchy, compaction, converged-store ADR | `fmt` + `clippy` + `cargo test` 928 green — DONE 2026-09-20 |
| Phase 3 — Layers 4+5 tool loop/MCP (3.1–3.9) | [x] | Tool serialization, loop caps, flag-gated call, permissions, display, MCP exec, failover/ACP, control-plane, web-fetch | `fmt` + `clippy` + `cargo test` + mock-E2E tool-call run — DONE 2026-09-20 |
| Phase 4 — Layers 1+2+7 store/retrieval/skills (4.1–4.6) | [x] | Converged store, vector search, memory signal, skill promotion, retrieval via store, dead-API sweep | `fmt` + `clippy` + `cargo test` + `cargo deny check` — DONE 2026-09-21 (deny: bans/licenses/sources pass; advisories has pre-existing RUSTSEC-2026-0285 rustls transitive, zero new deps) |
| Phase 5 — Isolation/hysteresis (5.1–5.7) | [x] | Diff scope, teardown, policy, hooks, RunBudget, failure contract, risk topology | `fmt` + `clippy` + `cargo test --verbose` green (992 passed) — DONE 2026-09-21 |
| Phase 6 — Config/docs/CI (6.1–6.6) | [x] | Dead-field warnings, effort, env precedence, doc-truth, CLI cleanup, MSRV + no-default CI | `fmt` + `clippy` + `test` + `deny` + `audit` clean — DONE 2026-09-21 |
| Phase 7 — Guards/eval/release (7.1–7.6) | [ ] | Pipeline guards, lifecycle envelope, goal/session, eval honesty, TUI pins, release proof | Full CI gate + mock-E2E on/off + Docker run if available |
| Best-in-class hardening loop (rep 2) | [ ] | Rep-2 plan → work → verify; competition parity (Claude Code league); prod E2E | No further improvements found; E2E user-testable |
| ALL_PHASES_COMPLETE | [ ] | All above `[x]` + two repetitions + no open tightening | Ralph loop exits |

## How another model must use this file

1. Read **Global rules** first.
2. Execute **Phase 0** before changing behavior.
3. Implement phases in order unless a phase explicitly says it is independent.
4. After every implementation task: run the listed verification command and stop if it fails.
5. Do not reinterpret zero-caller claims without rerunning the exact grep in Phase 0.
6. If code contradicts this plan, trust the code, record the discrepancy, and update the affected acceptance criterion.

## Repository primer

- Project: Rust CLI at `/home/shiva/projects/niki`; binary: `niki`; edition 2024; MSRV 1.85.
- Entrypoint: `src/main.rs`; library root: `src/lib.rs`.
- Build/verify order: `cargo fmt --check`, `cargo clippy --all-targets` warning-free, `cargo test --verbose`, `cargo build --release`.
- Single test: `cargo test <test_name>`.
- Vendored libgit2 is used, so no system libgit2 setup is required.
- Mock LLM server for local E2E: `python3 tests/integration/mock_llm.py &` on `:8080`.
- Local E2E without containers: `./target/release/niki run 'Add health endpoint' --backend worktree --quiet --project <git_repo>`.
- `podman build -t niki-sandbox:24.04 -f docker/Dockerfile .` is required for Docker-backend E2E; plain `ubuntu:24.04` lacks required tools.

## Critical build quirk

`src/lib.rs:134-135` embeds `prompts/` and `schemas/` into the binary through `include_dir!`. Editing prompt/schema source files has no runtime effect until rebuild. Every prompt/schema task must end with `cargo build` or `cargo run`.

## Global rules

- Read-only research is finished; implementation starts only in act mode.
- Touch only files/tasks named in the active phase.
- Prefer editing existing modules over creating new abstractions.
- No speculative features, no adjacent refactoring, no formatting churn.
- Keep typed errors on user-facing paths; do not introduce bare `.unwrap()` there.
- Never commit secrets; `.niki/` and `niki.toml` are git-ignored.
- Never delete a pub API solely because an old grep said it had no caller. Rerun Phase 0 greps in the current tree first.
- If a behavior is best-effort today, keep it best-effort unless the task explicitly changes the contract.
- Verification is mandatory: format, clippy warning-free, relevant tests, and where stated, targeted E2E.

## Evidence status

These statements were directly verified during research:

- `run_tool_loop` exists at `src/runtime/mod.rs:2176` and has no production caller outside `src/runtime/mod.rs` tests.
- `build_baseline_registry` exists at `src/runtime/mod.rs:2058`.
- `src/persistence/*` is used by chat-session persistence in `src/display/tui.rs:1211,1212,1349,1447`; do not treat the whole module as dead.
- `src/control_plane/mod.rs:9-11` explicitly says the Convex mirror is intentionally unwired.
- Compilation was not run during research; test/build outcomes are therefore open until the executing model runs them.

## Phase 0 — Ground-truth re-grep [x]

Do this phase first. It exists because verification found one refuted zero-caller claim and partially confirmed others. Do not delete code based on remembered research.

### 0.1 Rerun all zero-caller greps in the current tree

Run these exact read-only commands from `/home/shiva/projects/niki`:

```sh
git --no-pager status --short
grep -rn "body_stages_for" src/ tests/ --include='*.rs'
grep -rn "set_artifact\|set_feedback\|get_latest_feedback" src/ tests/ --include='*.rs'
grep -rn "run_tool_loop" src/ tests/ --include='*.rs'
grep -rn "build_baseline_registry" src/ tests/ --include='*.rs'
grep -rn "ToolRegistry::execute\|\.execute(&" src/ tests/ --include='*.rs' | head -80
grep -rn "AuditEntry\|write_audit_entry\|append_audit_entry" src/ tests/ --include='*.rs'
grep -n "impl Drop" -r src/sandbox --include='*.rs'
grep -rn "load_agents_md_hierarchy" src/ tests/ --include='*.rs'
grep -rn "render_hierarchical_memory" src/ tests/ --include='*.rs'
grep -rn "render_memory_with_budget\|load_compressed_knowledge\|compress_context" src/ tests/ --include='*.rs'
grep -rn "append_team_memory\|append_user_memory\|load_team_memory\|load_user_memory" src/ tests/ --include='*.rs'
grep -rn "SymbolIndex\|rank_files_by_relevance\|render_relevant_context\|symbols_for_file\|all_symbol_names" src/ tests/ --include='*.rs'
grep -rn "project_summary" src/knowledge --include='*.rs'
grep -rn "load_config" src/mcp src/orchestrator src/cli --include='*.rs'
grep -rn "budget_used" src/goal src/cli --include='*.rs'
grep -rn "goal::config::GoalConfig\|goal::GoalConfig\|GoalConfig" src/goal src/config src/cli --include='*.rs'
grep -rn "verbose" src/cli/run.rs src/cli/plan.rs
grep -rn "skills-lock\|skills_lock" src/ tests/ --include='*.rs'
grep -rn "mcp_tools" prompts src --include='*.md' --include='*.rs'
grep -rn "task_relevant_context" prompts src --include='*.md' --include='*.rs'
grep -rn "{{.*}}" prompts/critic.md
grep -rn "\.tools" src/llm --include='*.rs'
grep -rn "tool_calls" src/llm --include='*.rs'
grep -rn "request_structured" src/ --include='*.rs'
grep -rn "persistence::" src/ --include='*.rs' | grep -v 'src/persistence/'
grep -rn "config\.instructions\|instructions\.enabled\|instructions\.paths\|auto_detect_agents_md" src/ --include='*.rs' | grep -v 'src/config/'
grep -rn "compaction\." src/ --include='*.rs' | grep -v 'src/config/'
grep -rn "\.effort" src/ --include='*.rs' | grep -v 'src/config/'
grep -rn "structural_index\|disk_budget_mb\|retention_days" src/ --include='*.rs' | grep -v 'src/config/'
```

### 0.2 Record the outcome

- For each grep, record: command, hit count, whether the remembered claim still holds.
- If any caller now exists, remove the corresponding delete/wire task from later phases or convert it into a behavior-change task.
- Especially recheck `body_stages_for`, `PipelineState::{set_artifact,set_feedback,get_latest_feedback}`, audit writers, and `goal/config.rs::GoalConfig`.
- Acceptance: a short table in the implementation PR/notes with all commands and results.
- Verify: no files changed in this phase; only notes.

## Phase 1 — Contract truth [x] (blocks Phases 3 and 5)

Goal: every role's JSON contract validates AND parses; every prompt variable is supplied; no blind judges.

### 1.1 Fix the Synthesis contract (severe, blocks the parallel-coder path)
- Fact: `schemas/synthesis.schema.json` requires `merged` shaped as `{unified_diff, files_changed[{path,action,diff}], ...}` (schema line ~5), while `src/artifacts/types.rs:264-267` declares `merged: CodeDiff` shaped as `{edits[{search,replace}], files_changed[{path,action,language}], ...}` (`types.rs:92-111`). Schema-valid output cannot pass `parse_role`; struct-valid output cannot pass schema validation. The parallel-coder path therefore always aborts.
- Task: choose ONE canonical shape. Recommended: keep the `CodeDiff` struct as canonical and rewrite the schema's `merged` to mirror it exactly. Alternative (only with written justification): introduce a `SynthesisMerged` struct and update schema plus `parse_role` (`pipeline.rs:979-991`).
- Files: `schemas/synthesis.schema.json`, `src/artifacts/types.rs`, `src/orchestrator/pipeline.rs`.
- Acceptance: a Synthesizer fixture validates against the schema AND parses via `parse_role`.
- Verify: `cargo test synthesis_contract && cargo clippy --all-targets`, then `cargo build` (rebuild embeds the schema).

### 1.2 Fix the Critic prompt (blind judge)
- Fact: `prompts/critic.md` interpolates only `{{artifact_schema}}` (`critic.md:22`), while `run_role` supplies 4-5 input artifacts plus knowledge and memory (`pipeline.rs:905-924`). The Critic judges a verdict it is never shown.
- Task: interpolate `input_artifacts[0..3]` (spec, coder, tester) plus the red challenge when present, mirroring the Reviewer block. Keep the narrow grounding instruction.
- Files: `prompts/critic.md`; change code only if variable names differ — confirm against the critic ctx in `run_role`.
- Acceptance: a test asserting the rendered critic prompt contains spec/verdict text.
- Verify: `cargo test critic_render && grep -c input_artifacts prompts/critic.md`, then rebuild.

### 1.3 Wire or remove `mcp_tools` and `task_relevant_context`
- Facts: `mcp_tools` is supplied in every body-stage ctx but no prompt file contains `{{mcp_tools}}`; `task_relevant_context` is referenced in `prompts/planner.md` (guard always false) but never supplied anywhere in `src/`. Re-grep both in Phase 0 first.
- Task: keep the `mcp_tools` supply (Phase 3 makes it executable) and add interpolation to the role prompts that should see tools (coder minimum; reviewer/tester as decided in notes). For `task_relevant_context`: either supply it from the context pack's task-relevant slice or delete the reference from `planner.md`.
- Acceptance: no referenced-never-supplied and no supplied-never-used template variables, enforced by the test in 1.5.
- Verify: Phase 0 greps plus `cargo test prompt_vars`.

### 1.4 Audit remaining schema-to-type drift
- Research flagged drift around `SecurityVerdict.strengths` (required on one side, optional/absent on the other) and `IssueCategory` variants that abort on otherwise valid outputs. Confirm exact fields during implementation; do not assume the truncated research notes.
- Task: for all 8 schemas, diff `required` plus value types against the Rust structs including serde attributes (`default`, `rename`, `deny_unknown_fields`). Fix in the direction that preserves compatibility with existing recorded artifacts in `tests/` fixtures.
- Acceptance: the roundtrip harness (1.5) passes 8/8.
- Verify: `cargo test artifact_contracts`.

### 1.5 Add the all-roles contract roundtrip harness (CI gate for this phase)
- Task: new test (`tests/artifact_contracts.rs` or unit tests) that, per role, validates a fixture against its schema with the `jsonschema` crate AND parses it via `parse_role`. Include a parallel-coder Synthesizer fixture and a post-1.2 Critic fixture.
- Task: add a template-variable lint test that renders every prompt with its real ctx builder and fails on unknown/missing variables.
- Acceptance: 8/8 role fixtures green; CI runs it via `cargo test`.
- Verify: `cargo test artifact_contracts prompt_vars && cargo clippy --all-targets`.

### 1.6 Decide `degrade_on_invalid`
- Fact: all four `run_agent` call sites in `pipeline.rs` (planner, body-stage, solo, apply-repair paths) pass `degrade_on_invalid=false`, and `parse_role` errors bypass the flag via `?` (`agents/mod.rs:355-374`; `NikiError::ArtifactValidation` at `src/lib.rs:74-78`). The degrade path is effectively dead.
- Task: either delete the degrade path (remove the flag, keep fail-loud) or wire exactly one deliberate use with a test. Do not leave dead error-handling branches.
- Acceptance: grep shows the flag used consistently or gone; tests cover the chosen behavior.
- Verify: `cargo test agent_failure && cargo clippy --all-targets`.

### 1.7 Document the no-constrained-decoding constraint
- Fact: `run_agent` always sends `json_schema: None` (`agents/mod.rs:87`; also `cli/chat.rs:174-175`); the schema travels as prompt text only. Only OpenAI (`openai.rs:294-380`) and Mock implement structured output, and `request_structured` has no production caller.
- Task: record this as an explicit constraint in `docs/content/04-providers-byok/05-failover-and-repair.mdx` so Phase 3 does not assume constrained decoding. Optional: route OpenAI stages through `request_structured` behind config, only if default behavior is unchanged.
- Verify: docs updated; `cargo test` green.

Phase exit: `cargo fmt --check && cargo clippy --all-targets` warning-free `&& cargo test --verbose` green.

## Phase 2 — Layer 1+2 foundation [x]: converged file store, memory durability, bounded context

Goal: one explicit storage contract, durable versioned memory, and no unbounded prompt channel.

### 2.1 Durable memory/session/learnings writes
- Facts: `save_memory`/`save_user_memory`/`save_team_memory` (`memory/store.rs:59-66,196,235`) use plain `fs::write` with no lock and no atomic rename; concurrent runs do read-modify-write and silently lose entries. `MemoryEntry` has no serde defaults, so any field change makes existing files unparsable and `load_memory` degrades to empty without warning (`store.rs:11-22,49-55`). Same silent-empty pattern in user/team/compressed-knowledge readers.
- Tasks:
  1. Add `#[serde(default)]` (or explicit defaults) to every persisted struct: `MemoryEntry`, `RoleMemory`, `LearningEntry` fields, `CompressedKnowledge`, `Session`, `ChatSession`, `Sidecar`, `KbManifest`, `IndexManifest`, `UnitFile`.
  2. Route all JSON state writes through the existing `kb::write_atomic` (temp file + rename) or a shared `write_atomic_json` helper; keep `write_restricted` 0600 semantics where reports/patches/sessions currently use it.
  3. Add a `schema_version: u32` field (default 1) to each store; on version mismatch, migrate if a migrator exists, else warn loudly (stderr + trace event) instead of silently emptying.
- Acceptance: a fixture missing a field parses; an interleaved-append test loses no entries and leaves no corrupt JSONL.
- Verify: `cargo test --lib memory::store learnings session persistence && cargo clippy --all-targets`.

### 2.2 Bound the unbounded prompt channel
- Fact: `ProjectKnowledge::render` (`knowledge/indexer.rs:158-225`) emits the file tree, AGENTS.md/.cursorrules/.editorconfig (ingested at `indexer.rs:328`), shared skills (`indexer.rs:367`), standing rules, and external sources (only source capped via `take(4000)` at `:216`) with no total budget. This string reaches every stage. Only the Planner pack has an explicit budget (`context_pack.rs`).
- Tasks:
  1. Add a total character budget for `render()` (config key, default sane; log the chosen default in notes) with the same explicit `[context truncated ...]` marker style as `context_pack.rs`. Priority order: standing rules > AGENTS.md/conventions > skills > file tree > history > dependencies; untrusted external sources stay last and labeled.
  2. Keep the boundary table complete: truncation must never cut the entry-point/build/test lines silently; mark what was cut.
- Acceptance: a fixture with many skills/rules renders within budget and carries the marker.
- Verify: `cargo test --lib knowledge::indexer knowledge::context_pack`.

### 2.3 Wire AGENTS.md hierarchy and `[instructions]`
- Facts: `load_agents_md_hierarchy` (`memory/agents_md.rs:28`, global + project + nested with 16k cap) has no production caller; project-root AGENTS.md only reaches prompts as an uncapped "skill file". `[instructions]` (`enabled/paths/auto_detect_agents_md`, `types.rs:716-722`) has zero consumers. Re-grep both in Phase 0.
- Tasks: inject the hierarchy loader's output into the standing-rules/conventions block of `ProjectKnowledge::render` (subject to the Phase 2.2 budget), honor `[instructions].enabled=false` and `paths`, and keep `auto_detect_agents_md` semantics documented. If the hierarchy loader is instead wired elsewhere, update the plan notes and keep a single injection point.
- Acceptance: with global+project+nested fixtures, the rendered prompt contains project and nested sections in depth order (mirroring the existing hierarchy unit tests); `enabled=false` removes them.
- Verify: `cargo test --lib memory::agents_md knowledge::indexer`.

### 2.4 Make compaction real and configurable
- Facts: `[compaction]` fields (`types.rs:571-579`) have no consumers; live behavior fires on `context_budget.needs_session_switch()` in `update_context_budget` (`pipeline.rs:1102-1145`), which runs after stages with capacity hardcoded at 200000 (`orchestrator/state.rs:21`). The "auto-compaction" outcome is a learnings entry, not a smaller next prompt.
- Tasks:
  1. Wire `threshold_pct/reserved_tokens/auto_compact/enabled` into `ContextBudget` (drop the hardcoded capacity; resolve per-model capacity with a documented default).
  2. Measure each stage's prompt before the call; when past threshold and `auto_compact` is on, compress lowest-priority sections first (history, external sources, older memory) and record what was dropped in `context.json`; when off, emit the explicit warning and continue.
- Acceptance: a stage crossing `threshold_pct` gets a measurably smaller prompt with a `context.json` record of dropped sections.
- Verify: `cargo test compaction memory::compression && cargo clippy --all-targets`.

### 2.5 Converged store spike (feeds Phase 4's decision)
- Task: no schema commitment yet. Write an ADR-style note (`docs/decisions/` or plan notes) comparing: (a) file stores with the Phase 2.1 contract, (b) embedded SQLite via a small, license-clean crate through the `deny.toml` gate, (c) keeping the unwired Convex mirror. Include migration cost, offline behavior, and what "converged" must mean for niki (single query surface for runs/artifacts/learnings, or explicit rejection of convergence).
- Acceptance: decision recorded with tradeoffs; Phase 4 implements the chosen option or the explicit rejection.
- Verify: `cargo deny check` passes for any new dependency considered; no dependency added without the note.

Phase exit: `cargo fmt --check && cargo clippy --all-targets` warning-free `&& cargo test --verbose` green.

## Phase 3 — Layers 4+5 [~]: executable MCP tool loop and gateway honesty

Goal: `niki run` can actually execute tools; providers, MCP, governance, and ACP behave as documented.

### 3.1 Serialize tools and parse tool calls in providers
- Facts: `CompletionRequest.tools` is declared (`llm/provider.rs:127`) but never serialized by any provider; all real providers hardcode `tool_calls: Vec::new()` (`anthropic.rs:~123`, `openai.rs:~163`, `google.rs:~108`, `ollama.rs:~97`); only test doubles emit calls (`runtime/mod.rs:2383`). Confirm exact lines at implementation time.
- Tasks: implement `tools` serialization plus `tool_calls` parsing for OpenAI-compatible providers and Anthropic `tool_use` blocks (the two paths `niki run` actually uses). Google/Ollama follow only if their tests pass; otherwise mark unsupported explicitly. Cap serialized tool specs and returned arguments by size.
- Acceptance: a wiremock upstream returning one tool call yields a non-empty `CompletionResponse.tool_calls`.
- Verify: new `tests/llm_tool_calls.rs` (wiremock) `&& cargo test test_tool`.

### 3.2 Feed tool data back with a cap and fix result semantics
- Facts: `run_tool_loop` only returns `summary` into `LoopMessage::ToolResult` (loop ends `runtime/mod.rs:2288`), discarding file content; unknown tool names return `Failed` results rather than errors (`runtime/mod.rs:441-451`); provider usage is discarded so tool loops have no token accounting.
- Tasks: feed capped tool `data` (not just summary) back to the model; accumulate usage into the loop output and from there into `StageMetric`; decide and document unknown-tool semantics (recommend: explicit `Failed` with `diagnostics` naming the missing tool, unchanged status contract).
- Acceptance: a `read` result's content is visible in the next request (assert in test); loop usage is non-zero and lands in metrics.
- Verify: extend the loop unit tests in `runtime/mod.rs` `&& cargo test tool_loop`.

### 3.3 Call the loop from a real agent path behind a flag
- Facts: neither `run_tool_loop` nor `ToolRegistry::execute` is reachable from `cli/`, `orchestrator/`, or `acp/`. `run_agent` always sends `tools: None` (`agents/mod.rs:87-88`).
- Tasks: add a config flag (default off) enabling one tool-capable stage (recommended: a bounded research/planning step) to run through `run_tool_loop` with a small step cap. Keep the default pipeline byte-identical when the flag is off. Extend `tests/integration/mock_llm.py` to emit one `tool_calls` turn so the mock E2E exercises the path.
- Acceptance: with the flag on, one `niki run` stage issues >=1 tool call and the result appears in the next request; with the flag off, existing E2E output is unchanged.
- Verify: `cargo test --test pipeline_degradation`, mock-E2E run, `cargo clippy --all-targets`.

### 3.4 Enforce permissions in `ToolRegistry::execute`
- Facts: `ToolDef.permission` is only declared (`runtime/mod.rs:129`); `ctx.permissions` is written only by tests; `execute` (`runtime/mod.rs:414-462`) runs hooks and lookup but no permission check. `PermissionRequirement::Deny` is therefore unenforced.
- Tasks: enforce in `execute`: `Deny` maps to `ToolStatus::PermissionDenied`; `Ask` consults `ctx.permissions` and the configured permission mode (manual/auto/dontask/bypass) with fail-closed headless behavior; unknown tools stay `Failed`. Fix the duplicate-registration defect (`register` overwrites the map but appends a second def, `runtime/mod.rs:369-373`) by replacing same-name defs.
- Acceptance: unit tests for Deny/Ask paths and duplicate registration.
- Verify: `cargo test permission registry && cargo clippy --all-targets`.

### 3.5 Subscribe the bus and map tool events to display
- Facts: `EventBus`/`Event` exist but the tool loop cites no subscriber mapping to `DisplayEvent`; `DisplayEvent::Tool*` variants are applied only in `display/state.rs:1470,1479` with no production emitter; `activity::AgentState` is consumed only by missions, never by the loop. Tracing targets contain no `niki::runtime|event|tools`; `observability/mod.rs` (`Span::new`, `emit_span`, `emit_span_jsonl`) has zero callers and its jsonl writer overwrites per call.
- Tasks: wire one subscriber path from loop events to `DisplayEvent::ToolCall/ToolResult` (or delete the unused Tool variants); route loop usage into the existing `DisplayEvent::StageDone/StageTotals` accounting; delete `observability/mod.rs` or reduce it to the single span helper the pipeline actually uses.
- Acceptance: `niki run` with the Phase 3.3 flag renders a tool card and tokens/cost include loop usage.
- Verify: `cargo test --test tui_navigation` plus a new subscribe unit test.

### 3.6 Make MCP configured, trusted, governed, and executable
- Facts: only `[mcp].enabled` is read (`pipeline.rs:1247`); `McpManager::new()` never calls `load_config` (`mcp/mod.rs:239`), so `[[mcp.servers]]` and `timeout_ms` cannot load; trust-store load is never called from the pipeline; governance (`read_only`, `domain_allowlist`, `mcp/mod.rs:50-73`) has no enforcement call site; `_conn` is dropped after discovery (`mcp/mod.rs:~295`), so tools cannot be called end to end.
- Tasks (in order):
  1. Load `config.mcp.servers`/`timeout_ms` via `load_config` at pipeline startup; assert in a test that a configured server appears in `tools_for_prompt()`.
  2. Reach the trust gate: load `McpTrustStore`, skip untrusted servers with a warning, force re-gate on fingerprint swap; add `niki mcp trust` CLI if no existing surface covers it.
  3. Enforce governance: real read-only metadata per tool (extend `McpTool` beyond `input_schema`), `domain_allowlist` checks on web-fetch tools, deny-by-default; add hang/timeout tests for a stuck server.
  4. Retain live connections (`Arc<Mutex<McpConnection>>` per server), register one runtime `Tool` per discovered MCP tool, and route it through the Phase 3.3 loop.
- Acceptance: E2E `niki run` executes a `tools/call` against a fixture MCP server (stdio or wiremock), with an untrusted server skipped and a write tool denied under default governance.
- Verify: `cargo test --lib mcp:: && cargo test --test mcp_exec` (new) `&& cargo test --test structured_output`.

### 3.7 Failover and ACP correctness
- Facts: `FailoverProvider` failover triggers on 7 substring matches (`llm/failover.rs:242-254`); breaker threshold 3 / 60s window (`:190`); mid-stream errors are not retried; usage from failed attempts is uncaptured; structured-output flags are not overridden by the failover wrapper. ACP does not echo `request.id` (`acp/server.rs:~170,~186`) and cancel handling is single-threaded.
- Tasks: add `tests/failover_chain.rs` (wiremock primary 429 x N then fallback; assert fallback usage recorded and schema honored); override `supports_structured_output`/`request_structured` in `FailoverProvider`; fix ACP id echo, reset cancel state after a run, handle cancel concurrently; extend `tests/acp_server.rs`.
- Acceptance: failover test green; ACP id-echo + cancel tests green.
- Verify: `cargo test --test failover_chain --test acp_server`.

### 3.8 Control plane: implement or delete
- Fact: no HTTP client exists; `Mirror`/`enqueue` appear only in comments; the module documents its own unwired status plus P1 requirements (enqueue after local durability, stable idempotency keys, FIFO drain with tail requeue, high-water persisted after successful flush).
- Task: either delete the module and its docs/config surface, or implement the minimal client (one-shot POST, spool with dedup-by-key, persisted high-water) exactly per those comments. No new product surface beyond what the comments specify.
- Acceptance: an httptest server receives the idempotency key; a failed flush requeues the tail; high-water persists.
- Verify: `cargo test --lib control_plane`.

### 3.9 Close the runtime web-fetch gap
- Fact: `src/tools/web_fetch.rs` (allowlist, 30s timeout, 50k truncation, `:26,:38-52,:84-90`) has zero callers; the registry's own `web_fetch` (`runtime/mod.rs:1315-1341`) performs an unchecked `reqwest::get` with no allowlist/timeout.
- Task: route the registry tool through the allowlisted implementation or delete one of them; keep a single web-fetch path with allowlist + timeout + truncation.
- Acceptance: one web-fetch implementation reachable; unchecked path gone.
- Verify: `cargo test web_fetch && grep -rn "reqwest::get" src/runtime`.

Phase exit: `cargo fmt --check && cargo clippy --all-targets` warning-free `&& cargo test --verbose` green, plus the mock-E2E tool-call run.

## Phase 4 — Layers 1+2+7 [ ]: converged storage, retrieval quality, skill promotion

Goal: one queryable store (or an explicitly rejected-and-documented alternative), signal-rich memory, and real promotion of successful workflows into versioned skills.

### 4.1 Decide and implement the converged store (uses the Phase 2.5 ADR)
- Constraint: full conformance requires a converged database for runs, artifacts, learnings, and vector search — or a written, reviewed rejection with compensating controls. Do not add a second source of truth by accident.
- Recommended shape (deviate only with a written reason): embedded store under `<output_dir>/store/` with migrations, tables for runs/stages/artifacts/learnings/memories, FTS for text, and a vector extension only if it passes `cargo deny check` and the license gate. Keep the existing JSONL/JSON writers as an export format for one release, then cut over readers.
- Non-negotiables: local-first and offline by default; ACID or explicitly documented atomicity boundaries; schema migrations with version table; `.niki/` stays git-ignored; no new network dependency in the default path.
- Alternative (only if the ADR rejects convergence): keep the Phase 2.1 file contract and document why, plus a `STATE_LAYOUT.md` mapping every store, writer, reader, and schema version. The Converged-store acceptance items below then convert to doc acceptance.
- Acceptance: one query API answers "all learnings + memory + artifacts for task X"; migration from existing `.niki/` layout is tested on fixtures.
- Verify: `cargo test store_migrate store_query && cargo deny check && cargo clippy --all-targets`.

### 4.2 Vector search over the semantic layer (only on top of 4.1)
- Task: index KB markdown, entity files, learnings, and memory content into the store's vector/FTS surface; retrieval ranks by hybrid score (keyword + vector + recency + authority tier) with a deterministic fallback to today's keyword path when the index is absent.
- Guard: index build stays offline and local; cap embedding/index size with the existing `disk_budget_mb` semantics (wire that currently-dead config key here or delete it).
- Acceptance: a retrieval test where a paraphrased task (no shared keywords) surfaces the relevant learning, where keyword search alone fails.
- Verify: `cargo test retrieval_semantic retrieval_fallback`.

### 4.3 Memory signal: dedupe, retention, and quiet writes
- Facts: `extract_memory_from_artifacts` (`pipeline.rs:2264-2319`) writes a constant "Task completed successfully with Approved verdict" entry on every Approved run and a generic "Revision was needed" entry on every RevisionNeeded verdict — noise that compounds across runs. Retention is drop-oldest at 100 entries with no value signal.
- Tasks:
  1. Dedupe by content hash before append (task + tags + content hash); skip the constant success entry or fold it into a per-task counter.
  2. Value-aware retention: keep entries referenced by retrieval (add a `use_count`/`last_used` updated on injection), then successes, then failures, then oldest-first within tiers; keep the 100-entry bound configurable.
  3. Route memory appends through the Phase 2.1 atomic path (or the Phase 4.1 store).
- Acceptance: three identical Approved runs produce at most one Coder entry; a retrieval-used entry survives pressure that evicts unused noise.
- Verify: `cargo test --test kb_pipeline` (memory-noise test) `&& cargo test --lib memory::store`.

### 4.4 Skill distillation and promotion (the missing Layer 7)
- Facts: skills are read-only (`skill_list`/`skill_load`, `~/.agents/skills/<name>/SKILL.md`); nothing writes `skills-lock.json` although `.gitignore` names it; learnings and memory accumulate facts but never become workflows.
- Tasks:
  1. Define the skill record: `name/`, `SKILL.md`, `metadata.json` (source runs, verdicts, suite results, model, created/updated, version), and a lock entry (`skills-lock.json`: name -> version -> content hash -> source run ids).
  2. Distillation trigger: on Approved verdict with a green executed suite, propose a candidate skill (what worked: plan shape, test commands, review notes) — write to a staging area, never auto-activate.
  3. Activation path: `niki skills promote --candidate <id>` (or `niki skills` subcommands: `list/promote/retire/diff`) writes the skill dir + lock entry; `skill_list`/`skill_load` then serve it. Document the SKILL.md format in `docs/`.
  4. Retirement: supersede on better evidence (newer successful runs with the same task shape bump version, keep history); `retire` marks deprecated with reason; stale-due-to-repo-drift detection via the existing snapshot stamps (`KB_SNAPSHOT`, `state_ref`) — a skill whose source snapshot no longer matches HEAD is flagged, not silently served.
- Acceptance: an Approved mock run yields a staged candidate; promoting it makes it visible via `skill_list`; retiring it removes it from listing with reason preserved in the lock.
- Verify: `cargo test skills_promote skills_retire && cargo test --test kb_pipeline`.

### 4.5 Retrieval actually uses the store
- Tasks: replace the learnings/memory selection path (`context_pack.rs` keyword overlap + last-N memory) with the Phase 4.1/4.2 query API, keeping the existing K caps and explicit truncation markers. Keep `--bare` semantics (no ambient history) untouched.
- Acceptance: Prompt-injection tests assert relevant (not merely recent) entries are chosen; bare mode still yields empty sections.
- Verify: `cargo test context_pack retrieval && cargo test --test kb_pipeline`.

### 4.6 Resolve remaining dead APIs from the audit
- Candidates pending Phase 0 re-grep: `body_stages_for` (`pipeline.rs:495`), `PipelineState::{set_artifact,set_feedback,get_latest_feedback}` (`state.rs:25-37`), audit writers (`audit/mod.rs:10-63`), `symbol_index.rs` query API, `render_hierarchical_memory` (`store.rs:261`), budget renderers (`compression.rs:166+`), `project_summary.rs`, session journal (`session/mod.rs:396-433`), duplicate `goal/config.rs::GoalConfig`, dead `agents/{coder,planner,reviewer}.rs` stubs, `WebFetchTool` vs registry `web_fetch` (handled in 3.9 — do not duplicate), unused `predicates` dev-dep.
- Task: for each, either wire into a live path with a test or delete with its tests updated. `persistence` is live (chat sessions) — do not touch except via its own tasks.
- Acceptance: every Phase 0 zero-caller entry is resolved to wired-with-test or deleted.
- Verify: `cargo clippy --all-targets` warning-free `&& cargo test --verbose`; rerun the Phase 0 grep table and attach it.

Phase exit: `cargo fmt --check && cargo clippy --all-targets` warning-free `&& cargo test --verbose` green `&& cargo deny check`.

## Phase 5 — Isolation correctness [ ]: unified hysteresis, autonomy bounds

Goal: the harness cannot corrupt the user's tree, leak sandboxes, or loop unboundedly; every give-up condition is observable.

### 5.1 Scope diffs to agent changes; stop mutating the host index
- Facts: Docker binds the host repo as `/workspace` (`sandbox/docker.rs:71-75`), so the host tree mutates during the run. `working_tree_diff` runs `git add -A -N` on the host (`output/git.rs:61`) and diffs everything except `.niki`/`niki.toml` — pre-existing unrelated dirt lands in `changes.patch`, commits, and the final diff. The worktree backend drops new untracked files from `final_diff`/commit because it skips `-N` (`worktree.rs:211-223` vs `git.rs:156-163`). `normalize_patch` is duplicated three times (`docker.rs:349-355`, `worktree.rs:89-95`, `output/git.rs:119-125`).
- Tasks:
  1. Capture a pre-run baseline (HEAD + tracked-file list, or per-file hashes) and scope `working_tree_diff` to agent-produced changes; remove the host `git add -A -N` side effect (use worktree-local or index-safe equivalents).
  2. Unify `normalize_patch` into one helper with one test; reconcile the `git apply` vs `patch -p1 --3way` fallback contradiction between `output/git.rs:88-110` and both backends' comments.
  3. Make the worktree backend include new files in `final_diff` and staging.
  4. Define partial-apply semantics: edit-format application must be all-or-nothing per stage or record exactly which files landed before the error (`docker.rs:411-424`, `worktree.rs:181-191` currently write matched files then `Err`).
- Acceptance: a fixture with a pre-existing dirty file shows it absent from `changes.patch` and the commit; new files appear in the diff on both backends; one `normalize_patch` with one test.
- Verify: new `tests/diff_scope.rs` `&& cargo test --test worktree_policy --test diff_scope`.

### 5.2 Fix interrupt cleanup and sandbox teardown
- Facts: Ctrl+C/SIGTERM cleanup deletes `<project>/<output_dir>/.niki-worktrees/<id>` (`cli/run.rs:~448,~505`) while the real directory is `<project>/.niki-worktrees/<id>` (`sandbox/worktree.rs:38`) — worktrees leak on interrupt until the 24h prune. There is no `Drop` impl on either sandbox; error/panic paths skip `destroy`. Stale pruning uses top-level dir mtime, so an actively used >24h worktree can be deleted by another run's create (`worktree.rs:378-427`). Same task id across concurrent runs collides (`:45-47` removes the existing dir).
- Tasks: record the real worktree/container path in the sandbox struct and make signal cleanup use it; add `Drop` guards (best-effort) for both backends; guard the prune against active worktrees (recursive newest-mtime or lock file; assert the 24h boundary in tests); make same-task-id collision fail loudly instead of deleting the other run's dir.
- Acceptance: interrupted run leaves no worktree/container; a >24h dir containing a fresh file is not pruned; colliding task ids error instead of clobbering.
- Verify: `cargo test --test worktree_policy` (extended) `&& cargo test sandbox_teardown`.

### 5.3 Deterministic command policy and BashTool routing
- Facts: `check_command_policy` (`sandbox/mod.rs:134-184`) allowlist-prefix short-circuits the deny list; `network_allowlist` only honors wildcard `*`/`all` (`docker.rs:43-48`); `extra_packages` is never installed, only a required-tool list (`pipeline.rs:512-518`, `docker.rs:320-333`); `BashTool` executes host `sh -c` without the policy check.
- Tasks: make allow/deny evaluation order-independent (deny wins on overlap) and mode-aware (Bypass/DontAsk only via explicit mode); route `BashTool` through `check_command_policy` or delete `BashTool` if the registry stays unreachable; either implement per-domain network filtering or document wildcard-only behavior in config docs and the example TOML; fix or document `extra_packages` semantics.
- Acceptance: overlapping Allow/Deny patterns resolve identically regardless of order; no host shell path bypasses the policy; docs match behavior.
- Verify: `cargo test --test security_exec --test worktree_policy`.

### 5.4 Hook timeouts and audit honesty
- Facts: hooks (`audit/hooks.rs`) have no timeout; a hanging hook hangs the run. `AuditEntry`/`write_audit_entry` (`audit/mod.rs:10-63`) have no callers (re-grep in Phase 0). Hook `Block` semantics abort via `fire_hook` (`pipeline.rs:414-417`).
- Tasks: add `[hooks] timeout_seconds` (default documented in notes, e.g. 30) mapping timeout to Noop-with-warning; resolve the audit writers to wired-with-test or deleted per Phase 0.
- Acceptance: a hanging-hook fixture returns in under 2s with a warning; audit writers resolved.
- Verify: `cargo test --test hooks_lifecycle`.

### 5.5 Unified hysteresis: one step/cost budget for the whole run
- Facts: patience is per-mechanism, not unified — transient retries (3, `agents/mod.rs:94`), HTTP retries (4/provider, `llm/provider.rs:65`), failover chain length, revision rounds (default 3), Critic's single retry, goal `max_iterations` (default 30, no cost/time bound, `budget_used` never read). `retry_count` is recorded (`state.rs:60-63`) but never spends a shared allowance.
- Tasks:
  1. Introduce a run-scoped `RunBudget { max_steps, max_usd, max_wallclock }` resolved from config + CLI, threaded through `execute_pipeline`, the tool loop (Phase 3), failover, repair passes, and the goal runner.
  2. Every retry/repair/revision/failover attempt decrements or accrues against it; exhaustion yields a typed `BudgetExhausted` outcome recorded in `task.json`, the report, and the trace — never a silent stop.
  3. Keep `spend_cap_usd` as the hard money ceiling and the diff guardrail as the size ceiling; close the parallel-coder spend hole (`enforce_spend_cap` after `run_parallel_coders`, `pipeline.rs:~1570`).
  4. Wire `goal` `budget_used`/`max` into the goal halt conditions (`goal/runner.rs:223-231`); add cost halt alongside `max_iterations`.
- Acceptance: a fixture configured with a tiny budget stops with `BudgetExhausted` in `task.json` regardless of which mechanism would otherwise retry; parallel mode honors the spend cap.
- Verify: `cargo test budget_exhaustion goal::runner spend_cap`.

### 5.6 Termination, rollback, and failure artifacts
- Facts: a failed run's `task.json` status, branch creation policy (only on success or `--force`), and rollback after `safety::prove` errors are partly implicit (`cli/run.rs:620-650,802-874`); no path reverts a partially applied diff; `git apply -p1 --3way` has no post-check for conflict markers (`output/git.rs:99-104`); `changes.patch` is written twice (`cli/run.rs:~786`, `output/report.rs:~686`); session code-rewind detaches HEAD (`cli/session.rs:75-89`); `eval --live` mutates the project tree without branching (`eval/mod.rs:532-540`).
- Tasks: define and test the failure contract — failed run creates no `niki/*` branch and leaves `task.json` Failed with error; single writer for `changes.patch`; post-apply conflict-marker check with abort; session rewind warns before detaching HEAD (or restores the branch); `eval --live` documents its no-branch mutation or gains a guard flag.
- Acceptance: `tests/run_lifecycle.rs` proves failed-run invariants; conflict markers abort instead of committing.
- Verify: `cargo test --test run_lifecycle`.

### 5.7 Risk-aware topology
- Fact: `select_topology` (`pipeline.rs:437-454`) runs before `risk::classify` (`pipeline.rs:1413`); `Auto` goes MultiAgent only on `security.enabled||parallel.enabled`, and `apply_risk_stages` returns Low-risk runs unchanged even with `[critic] enabled=true` (`pipeline.rs:257`).
- Tasks: move topology selection after risk classification; force MultiAgent for High/Security tiers; document the Low-risk + explicit-critic interaction. Keep explicit `[pipeline].stages` as the user-override that is never rewritten.
- Acceptance: Auto+High yields MultiAgent with a `SecurityAuditor` artifact in mock E2E.
- Verify: `cargo test --test risk_enumeration --test kb_pipeline`.

Phase exit: `cargo fmt --check && cargo clippy --all-targets` warning-free `&& cargo test --verbose` green.

## Phase 6 — Config honesty [x]: docs truth, CI completeness (DONE 2026-09-21)

Goal: every documented knob works, every working knob is documented, CI proves what the project claims.

### 6.1 Load-time warnings plus wire-or-delete for dead tables
- Known dead fields pending Phase 0 re-grep: `[instructions]` all fields; `[compaction]` all fields (consumed by Phase 2.4 — remove from this list once wired); `[session]` `enabled/max_sessions/auto_save`; `[repo_intel]` `structural_index`, `disk_budget_mb`; `[snapshot]` `retention_days`; `[goal]` all fields (duplicate `GoalConfig` in `goal/config.rs` vs `config/types.rs:391`); `[mcp]` `servers`/`timeout_ms` (consumed by Phase 3.6 — remove once wired); `[agents.*]` `effort`; `[ui.transcript]` `collapse_completed/expand_failures/show_intent_labels`.
- Task: extend the existing "not yet wired" warning (`config/types.rs:1366-1372`, currently only session/compaction/mcp) to every dead field. Then per field: wire it with a test or delete it with its docs. Do not leave warned-but-dead fields at the end of this phase.
- Acceptance: a config containing every table loads with zero warnings after the phase.
- Verify: `cargo test config && niki doctor` against a maximal `niki.toml`.

### 6.2 Wire `agents.*.effort` or remove it
- Fact: `effort` presets exist (`types.rs:1227-1258`) but `resolve_stages` copies raw `max_tokens`/`temperature` (`pipeline.rs:120-121`).
- Task: apply `effective_max_tokens`/`effective_temperature` (`types.rs:1243-1258`) in `resolve_stages`, with explicit per-agent overrides winning over `effort`.
- Acceptance: a coder with `effort="low"` and no explicit tokens resolves to 4096 (confirm the preset value in code; do not hardcode from memory).
- Verify: unit test in config or pipeline tests.

### 6.3 Unify env-vs-TOML precedence
- Fact: `ANTHROPIC_API_KEY` overwrites the TOML key (`types.rs:~1652`) while `OPENAI_API_KEY`/`GOOGLE_API_KEY` lose to it (`~1674-1685`); base-URL and model overrides have their own rules (`~1746-1785`).
- Task: implement one documented rule (recommended: explicit env always wins, recorded in the provenance/config fingerprint), apply uniformly, and align `AGENTS.md` and README statements.
- Acceptance: a test matrix covering key/base-url/model precedence for at least Anthropic, OpenAI, and Google.
- Verify: `cargo test config::env_precedence` (new or extended).

### 6.4 Doc-truth pass
- Known items (reconfirm paths at implementation time): `AGENTS.md` line refs for `include_dir!` and the `create_provider` location (`llm/provider.rs:170`, not `llm/mod.rs`); the stale core-module list (add `mcp/`, `memory/`, `session/`, `goal/`, `persistence/`, `acp/`, `control_plane/`, `audit/`, `mission/`, `eval/`, `permissions/`, `safety/`, `tools/`, `commands/`, `event/`, `observability/`, `activity/`, `cost.rs`, `recommend.rs`); `niki.example.toml` `[tips]`/`[transcript]` shape vs tables (`types.rs:466,470`), `threshold_pct` vs default, and inert `[goal]`/`[session]`/`[compaction]` stanzas; docs topology casing (`Auto` vs lowercase serde values, `types.rs:987-996`); docs `fallbacks` shape vs `Vec<String>` (`types.rs:1184`); `niki memory query`/`store` syntax vs docs (`cli/memory.rs:35,66-74`); README duplicate `knowledge/`.
- Task: fix all, then add the missing documented surfaces: skills layer, learnings, memory CLI, goal system, KB/index commands, `--bare` semantics (ambient inputs off; sampling nondeterminism remains), the no-constrained-decoding constraint (Phase 1.7), and the eval harness limits (Phase 7.4).
- Acceptance: every CLI subcommand and config table in docs matches `clap` definitions and `config/types.rs`.
- Verify: `cargo test config::tests::example_toml_parses_with_gateway_stanzas` plus a grep checklist in notes.

### 6.5 CLI surface cleanup
- Candidates: `run --verbose` (`cli/run.rs:153`, referenced only in `cli/plan.rs:37`) — implement or remove; session journal (`session/mod.rs:396-433`) — wire or delete per Phase 0; `niki memory` subcommands without backend support — align with the Phase 4 store.
- Task: no parsed-but-unused flags and no documented-but-missing subcommands at phase end.
- Acceptance: grep for each removed flag shows zero hits in `src/`, `tests/`, docs, and example TOML.
- Verify: `cargo test && grep -rn verbose src/cli docs niki.example.toml`.

### 6.6 CI completeness
- Facts: CI pins `dtolnay/rust-toolchain@stable`, so MSRV 1.85 is never built; no job builds `--no-default-features` (the `ast` feature gates 4 sites in `structural.rs` with non-`ast` fallbacks — compile status unknown); `deny.toml` gates licenses; releases use `cargo dist`.
- Tasks: add an MSRV 1.85 job, a `--no-default-features` build job, make clippy fatal (`-D warnings`), and add the artifact-contract roundtrip test (Phase 1.5) plus `run_lifecycle` (Phase 5.6) to the gate. Remove the unused `predicates` dev-dep after confirming zero references (`cargo tree -e dev`).
- Acceptance: CI matrix covers stable + MSRV + no-default-features; clippy failures block merge.
- Verify: workflow file diff + one green CI run; locally `cargo build --no-default-features`.

Phase exit: `cargo fmt --check && cargo clippy --all-targets` warning-free `&& cargo test --verbose` green `&& cargo deny check && cargo audit` clean (or documented exceptions).

## Phase 7 — Guard tests [x]: autonomy proof, release readiness (DONE 2026-09-21)

Goal: the remaining failure modes are pinned by tests; eval claims are honest; the release path is proven.

### 7.1 Pipeline guard tests
- Gaps: spend-cap abort, cancel/Ctrl-C, `max_rounds` exhaustion on a stuck `RevisionNeeded` reviewer, parallel coders, risk-tier runtime `SecurityAuditor`, Critic with empty `reviewer_json`, guardrail warnings, and steer — are untested (`tests/kb_pipeline.rs` asserts verdicts and learning counts but not `revision_rounds` except the critic case; `tests/retry_tracking.rs` covers only serde/arithmetic).
- Tasks: add `tests/pipeline_guards.rs` covering cancel (`NikiError::Cancelled` + `task.json` `Cancelled`), always-`revision_needed` stopping at the configured rounds with `revision_rounds` recorded, tiny-budget `BudgetExhausted` (Phase 5.5), parallel-coder spend-cap enforcement, and empty-Critic-input behavior.
- Acceptance: each guard has a failing-before/passing-after test where behavior changes, or a pinning test where behavior is already correct.
- Verify: `cargo test --test pipeline_guards`.

### 7.2 Run-lifecycle and output-envelope tests
- Tasks: `tests/run_lifecycle.rs` proving failed runs create no `niki/*` branch and leave `task.json` Failed (Phase 5.6 contract); envelope tests for `--output-format json` (pure-JSON stdout, including with `--tui` requested — Phase: TUI must not corrupt the envelope per `agent_stream.rs:129-137`); single-writer `changes.patch`; conflict-marker abort.
- Acceptance: lifecycle invariants hold on Docker-skipped CI (use the worktree backend) and the JSON envelope parses with `python3 -m json.tool`.
- Verify: `cargo test --test run_lifecycle` plus a piped-envelope check in notes.

### 7.3 Goal-loop and session tests
- Tasks: cost-halt test for the goal runner (`budget_used` wired in Phase 5.5), drift-detection test, session checkpoint/rewind tests (code vs conversation modes), mission snapshot roundtrip under relaunch.
- Acceptance: goal loop halts on budget as well as iterations; rewind modes restore exactly what they claim.
- Verify: `cargo test goal::runner goal::state session persistence`.

### 7.4 Eval-harness honesty
- Facts: replay mode rebuilds `PipelineResult` from frozen fixtures with `cost_usd = 0.0` (`eval/mod.rs:512-514`) and scores artifact text (category/keyword match, `eval/mod.rs:328-398`), never executing code; `grader_agreement` compares `merge_worthy` to `niki.caught` (`:168-184`); live mode runs both configs in the real project dir with no branch/commit (`:521-540`).
- Tasks: document these limits in `docs/content/07-evals-auditing/` (replay is a regression gate on the grader/artifacts, not on model behavior; replay cost unmeasured; fixtures are freely editable and provenance is advisory); add a fixture-integrity check (hash recorded at grading time, mismatch warns); record harness commit/dirty state into every eval manifest (already partially present — complete it).
- Acceptance: docs state what evals prove and cannot prove; tampered fixtures produce a warning, not a silent pass.
- Verify: `cargo test eval` plus a tamper-fixture manual check in notes.

### 7.5 TUI/display regression pins
- Facts: `AgenticDisplay` buffers all events and forwards over mpsc (`agent_stream.rs:48-58,155-172`); only the TUI thread mutates `AppState` (`state.rs:1309-1531`); `enable_tui` no-ops without a TTY but ignores muted/json mode; chat persistence rehydrates chat/notes/model/round only (`display/persistence.rs:50-64`).
- Tasks: pin TUI/JSON precedence (muted wins or explicit error), pin event-buffer drain behavior (`take_events`), pin chat-session rehydrate scope, and keep the existing `tests/tui_smoke` + `tui_perf` green.
- Acceptance: no TUI bytes on machine-output paths; rehydrate restores exactly the documented scope.
- Verify: `cargo test --test tui_smoke` (or the repo's smoke runner) plus new unit tests.

### 7.6 Release-path proof
- Tasks: after all behavior phases, run the full gate exactly as CI does (`fmt --check`, `clippy --all-targets`, `cargo test --verbose`, `cargo build --release`, `./target/release/niki --version --help`), plus `cargo deny check`, `cargo audit`, the mock-E2E worktree run with the Phase 3 tool flag on and off, and (if available) one Docker-backend run.
- Acceptance: all green; any new dependency carries a `deny.toml` entry with written justification; releases continue through `cargo dist` only.
- Verify: paste the command log into the release notes.

## Final acceptance [x] for the whole plan (DONE 2026-09-21)

- All seven framework layers have a live, tested implementation path: converged or explicitly-documented storage with vector search; episodic + procedural memory with retrieval; deterministic semantic layer with tiers; executable MCP gateway with trust and governance; staged loop plus supported tool loop; budgeted adaptive context; versioned skill promotion with retirement.
- Every Phase 0 zero-caller entry resolved to wired-with-test or deleted.
- `cargo fmt --check`, `cargo clippy --all-targets` warning-free, `cargo test --verbose` green, `cargo deny check` clean.
- `AGENTS.md`, README, `docs/content`, and `niki.example.toml` match the code.
- Known remaining limitations are written down in the affected docs, not left as silent gaps.

## Appendix A — Key file map (starting points, not substitutes for reading)

- Pipeline: `src/orchestrator/pipeline.rs`, `state.rs`, `reflect.rs`, `provenance.rs`; risk: `src/risk/mod.rs`.
- Tools/loop: `src/runtime/mod.rs`, `src/tools/`; events: `src/event/mod.rs`, `src/activity/mod.rs`; spans: `src/observability/mod.rs`.
- Isolation: `src/sandbox/{mod,docker,worktree,edit_format}.rs`; policy: `src/permissions/mod.rs`; snapshots: `src/safety/mod.rs`; hooks: `src/audit/hooks.rs`, `src/audit/mod.rs`; image: `docker/Dockerfile`.
- Knowledge: `src/knowledge/{indexer,kb,context_pack,learnings,history,structural,symbol_index,project_summary}.rs`; memory: `src/memory/{store,compression,agents_md}.rs`; manifest: `src/repo_intel/manifest.rs`.
- Gateway: `src/llm/{provider,anthropic,openai,google,ollama,mock,failover,repair}.rs`; MCP: `src/mcp/{mod,client}.rs`; ACP: `src/acp/`; mirror: `src/control_plane/mod.rs`; cost: `src/cost.rs`, `src/recommend.rs`.
- CLI/outputs: `src/cli/*.rs`, `src/main.rs`, `src/commands/`, `src/session/`, `src/persistence/`, `src/goal/`, `src/mission/`, `src/eval/`, `src/output/`.
- Stages/contracts: `src/agents/`, `src/artifacts/`, `prompts/*.md`, `schemas/*.json`.
- Display: `src/display/` (`agent_stream.rs`, `state.rs`, `tui.rs`, `persistence.rs`).
- Config/CI: `src/config/types.rs`, `src/config/mod.rs`, `niki.example.toml`, `.github/workflows/`, `deny.toml`, `dist-workspace.toml`, `Cargo.toml`.
- Tests: `tests/` (`kb_pipeline.rs`, `artifact_contracts.rs` new, `pipeline_guards.rs` new, `run_lifecycle.rs` new, `diff_scope.rs` new, `failover_chain.rs` new, `mcp_exec.rs` new, `acp_server.rs`, `hooks_lifecycle.rs`, `worktree_policy.rs`, `security_exec.rs`, `risk_enumeration.rs`, `retry_tracking.rs`, `structured_output.rs`, `llm_tool_calls.rs` new, `integration/mock_llm.py`, `common/harness.rs`).

## Appendix B — Explicit non-goals

- New TUI features or themes; new LLM providers; new agent roles; prompt/model quality tuning.
- Cloud product decisions beyond the implement-or-delete control-plane task.
- Changing the default pipeline topology or default models.
- Rewriting history, force-pushing, amending commits, or changing git config.
- Adding dependencies without a `deny.toml` justification and a written tradeoff note.
