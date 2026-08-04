# NIKI Goal System — Implementation Plan

## Overview

Implement a `/goal` persistent autonomous goal runner (ported from OpenCode + KiloCode) that enables NIKI to run long-running, self-directed implementation cycles without human intervention. The system derives verifiable criteria from objectives, decomposes work into sub-tasks, iterates autonomously, and verifies completion honestly.

**Philosophy:** One objective string in → derive criteria, scope, plan → iterate continuously without asking → verify honestly → commit per unit → finish or block. No budget cap. No "want me to continue?".

---

## Architecture

```
.opencode/goals/
  <slug>-<id>.json          # Goal state (persistent)
  session-<session-id>.goal  # Claim file (active session marker)
  completion_log.txt         # Written on completion
```

### Core Components

| Component | File | Purpose |
|-----------|------|---------|
| Goal parser | `src/cli/goal.rs` | Parse `/goal` commands, dispatch to subcommands |
| Goal state manager | `src/goal/state.rs` | Read/write goal JSON, claim files, archive |
| Goal creator | `src/goal/creator.rs` | Iteration 0: survey, derive criteria, scope lock, decompose |
| Goal runner | `src/goal/runner.rs` | Autonomous loop: read state → work → verify → advance |
| Goal CLI | `src/cli/goal.rs` | `niki goal <command>` subcommand |
| Goal config | `src/config/goal.rs` | `[goal]` config section (max_iterations, branch prefix, etc.) |

### Integration Points

- **CLI:** New `goal` subcommand in `src/main.rs` and `src/cli/mod.rs`
- **Config:** New `[goal]` section in `NikiConfig`
- **Pipeline:** Goal runner uses `execute_pipeline()` for implementation tasks
- **Safety:** Goal runner respects `RepoSnapshot` hermeticity proofs
- **Display:** Goal status shown in TUI via `PageId::Goal`

---

## Phase 1: Core Goal Infrastructure (Reliability)

### 1.1 Goal State Manager
- Read/write goal JSON files to `.opencode/goals/`
- Claim file creation/deletion for session ownership
- Goal archive on cancel/complete
- Status transitions: `active` → `paused` → `active` → `complete` | `cancelled`

### 1.2 Command Parser
- Parse `/goal <objective>` → create new goal
- Parse `/goal list` → list all goals
- Parse `/goal status` → show current goal detail
- Parse `/goal pause` → pause active goal
- Parse `/goal resume <id>` → resume paused goal
- Parse `/goal cancel` → cancel active goal
- Parse `/goal check` → run criteria once without iterating

### 1.3 Goal Creator (Iteration 0)
- Slugify objective → kebab-case + 6-char id
- Survey codebase (glob, grep, read) — 30-60 second scan
- Derive 2-5 verifiable success criteria (structural + user-facing + coverage gate)
- Scope lock from `--scope` or survey results
- Decompose into 3-15 sub-tasks via TodoWrite
- Create git branch `goal/<slug>` if in git repo
- Write state JSON to `.opencode/goals/<slug>-<id>.json`
- Create claim file `.opencode/goals/session-<session-id>.goal`

### 1.4 Autonomous Runner Loop
- Read state JSON each iteration
- Check halt conditions (paused/cancelled/max_iterations/scope violation)
- Get current task, work on it, verify
- Advance task, check if all done → run criteria
- Criteria pass → write completion log, delete claim, mark complete
- Criteria fail → record failure, backtrack or escalate, continue iterating

### 1.5 Retry / Backoff for LLM Calls (Reliability)
- Wrap `llm.stream()` in retry loop with exponential backoff + jitter
- 3 attempts max, configurable via `[goal]` config
- Distinguish transient (429, 5xx, timeout) vs permanent (4xx auth, schema violation)
- Surface retry counts in metrics

### 1.6 Structured Error Taxonomy (Reliability)
- Extend existing `EditError` enum with `PipelineError` enum
- `PipelineError` variants: `Timeout`, `SchemaViolation`, `ContextOverflow`, `ProviderError`, `ScopeViolation`, `HermeticityViolation`
- Each error type triggers a different recovery strategy
- Errors surfaced to next agent iteration as typed hints

### 1.7 Staged Evidence Gates (Reliability)
- Order verification: retrieval grounding → syntax gate → target test → full regression
- Skip later gates if earlier ones fail
- Uses existing `syntax_check()` and `run_tests()` sandbox methods

---

## Phase 2: Scalability

### 2.1 Parallel Non-Dependent Stages
- Build stage DAG from pipeline configuration
- Identify independent stages (no shared mutable inputs)
- Dispatch via `tokio::join!` — concurrent execution
- Preserve sequential semantics for dependent stages
- Collect results into existing `artifacts` and `isolation` vectors

### 2.2 Critical-Path DAG Optimization
- Compute longest path through stage DAG
- Surface "critical-path length" metric
- Print savings from parallelism in TUI Cost page and HTML report
- Use critical-path depth as topology selection signal

### 2.3 Adaptive Topology Signals
- Enrich `select_topology()` with critical-path depth, parallelism width (antichain), coupling density
- Map task DAG to one of four topologies in O(|V|+|E|) time
- Configurable via `[goal]` config or pipeline config

---

## Phase 3: Security

### 3.1 Tool-Level Allowlist Per Agent Role
- Add `[security.policy.<role>]` to `NikiConfig`
- Fields: `allowed_commands`, `denied_commands`, `max_exec_seconds`
- `Sandbox::exec()` wrapper rejects commands matching `denied_commands` before launching
- Enforce timeouts via `tokio::time::timeout`
- Default deny-list: `git push --force`, `rm -rf /`, `mkfs`, `dd`, `curl | sh`, `--no-verify` bypass
- Per-role defaults: Tester read-only for writes, Coder can write but not push, Reviewer read-only

### 3.2 Hermetic Execution Proof Enhancement
- Extend `RepoSnapshot` to also record `git reflog` entries
- Prove no rebases/force-updates happened on existing branches
- Add `--strict-safety` flag that fails the run if any invariant breaks
- Today's system only reports; this makes it fail-closed

### 3.3 Audit Log (Immutable Per-Run Record)
- Write per-run audit log to `.niki/audit/<task-id>.jsonl`
- Every tool invocation, decision, and artifact hash recorded
- Useful for compliance and post-mortem analysis
- Structured JSONL format for machine parsing

---

## Phase 4: UX (Onboarding + Tips)

### 4.1 First-Run Onboarding Modal (TUI)
- On first invocation (no `.niki/` directory in cwd), show paginated modal
- Pages: Welcome → Shortcuts → Workflow → Sandbox backends → Help
- "Skip" button persists `onboarded=true` in `~/.niki/state.json`
- "Don't show this again" checkbox per page
- Gate behind `isatty(stdin) && !ci_env` — never show in CI
- Files: new `src/display/onboarding.rs`

### 4.2 Tips Banner (TUI Bottom Stripe)
- 1-line footer below active page, above existing footer
- Rotating tip from curated list (40+ tips)
- Rotation: every 30s or on page navigation
- OSC 8 terminal hyperlinks for clickable links
- Configurable via `[ui.tips] enabled = true|false`
- Gate to TUI mode only — no impact on `--quiet` or piped output
- Files: new `src/display/tips.rs`

---

## Files to Create

| File | Purpose |
|------|---------|
| `src/cli/goal.rs` | CLI subcommand handler for `niki goal` |
| `src/goal/mod.rs` | Goal module root |
| `src/goal/state.rs` | Goal state JSON read/write |
| `src/goal/creator.rs` | Iteration 0: survey, criteria, scope, decompose |
| `src/goal/runner.rs` | Autonomous execution loop |
| `src/goal/config.rs` | `[goal]` config section |
| `src/goal/claim.rs` | Claim file management |
| `src/display/onboarding.rs` | First-run onboarding modal |
| `src/display/tips.rs` | Tips banner rotation |
| `src/errors.rs` | Structured `PipelineError` taxonomy |
| `src/observability/mod.rs` | Structured JSON logging + OTLP export |
| `src/audit/mod.rs` | Immutable per-run audit log |
| `src/cli/goal.md` | Help text for `niki goal --help` |

## Files to Modify

| File | Change |
|------|--------|
| `src/main.rs` | Add `Goal` subcommand |
| `src/cli/mod.rs` | Add `pub mod goal` |
| `src/config/types.rs` | Add `[goal]`, `[security]`, `[ui.tips]` config sections |
| `src/sandbox/mod.rs` | Add allowlist enforcement in `exec()` |
| `src/sandbox/worktree.rs` | Add allowlist enforcement |
| `src/sandbox/docker.rs` | Add allowlist enforcement |
| `src/safety/mod.rs` | Extend `RepoSnapshot` with reflog proof |
| `src/orchestrator/pipeline.rs` | Add parallel stage dispatch, DAG metrics |
| `src/agents/mod.rs` | Add retry/backoff around `llm.stream()` |
| `src/display/tui.rs` | Add `PageId::Goal`, onboarding trigger |
| `src/display/pages/mod.rs` | Add onboarding page |
| `prompts/planner.md` | Add goal context injection |
| `Cargo.toml` | Add dependencies (serde_json for audit, chrono for timestamps) |

---

## Implementation Order

1. **Phase 1 (Reliability)** — Goal infrastructure + retry + structured errors + staged gates
2. **Phase 2 (Scalability)** — Parallel stages + DAG optimization
3. **Phase 3 (Security)** — Allowlist + hermetic proof + audit log
4. **Phase 4 (UX)** — Onboarding + tips banner
5. **Phase 5 (Cleanup)** — Tests, dead code resolution, documentation

---

## Recommendations

### Start With
1. **Goal infrastructure (1.1–1.4)** — This is the foundation everything else builds on. Without it, NIKI can't run autonomous cycles.
2. **Retry/backoff (1.5)** — Highest reliability win for the effort. Transient LLM errors are the #1 cause of failed autonomous runs.
3. **Onboarding modal (4.1)** — High UX impact, low risk, no architecture changes.

### Defer
1. **Adaptive topology (2.3)** — Requires mature DAG analysis; can use static topology until Phase 2 is stable.
2. **OTLP observability (1.6)** — Nice-to-have; structured JSON logs are sufficient initially.
3. **Audit log (3.3)** — Important for compliance but can ship after core goal system is stable.

### Avoid This Cycle
1. **CE-MCP** — The research flagged 16 new attack classes; too risky without a thorough threat model.
2. **AST-targeted edits** — Mixed results in research; fuzzy cascade is sufficient for now.
3. **Capability tokens with HMAC** — Overkill for v1; tool-level allowlist is the right minimal step.

---

## Open Questions — RESOLVED

| # | Question | Answer |
|---|----------|--------|
| 1 | Parallel stages failure semantics | **Fail-fast** — if any parallel branch fails, fail the entire run immediately |
| 2 | Tool allowlist default | **Permissive deny-list** — allow all commands, deny-list specific dangerous ones |
| 3 | Onboarding trigger | **First `niki` invocation** — show on first command of any kind |
| 4 | Tips banner source | **Hardcoded in source** — 40+ tips in Rust source, no external dependency |
| 5 | Goal state storage | Project-local `.opencode/goals/`, fall back to global |
| 6 | Git branch strategy | New branch `goal/<slug>` for isolation, merge on completion |
| 7 | Max iterations default | **30** — configurable via `--max-iterations` |
| 8 | Claim file mechanism | Marker files for simplicity; PID-based for multi-agent safety in v2 |
| 9 | Completion audit | Include `cargo test` and `cargo clippy` as default criteria for Rust projects |
| 10 | Integration with pipeline | Goal runner wraps `execute_pipeline()` as its work unit |