# NIKI 7-Layer Harness — Ralph-loop Task Checklist

## Rep 1 — plan phases in order
- [x] Phase 0 — Ground-truth re-grep
- [x] Phase 1 — Contract truth (1.1–1.7)
- [x] Phase 2 — Layer 1+2 foundation (2.1–2.5)
- [x] Phase 3 — Layers 4+5 tool loop/MCP (3.1–3.9 all complete)
- [x] Phase 4 — Layers 1+2+7 store/retrieval/skills (4.1–4.6 DONE 2026-09-21)
- [x] Phase 5 — Isolation/hysteresis (5.1–5.7 DONE, gate: fmt+clippy+992 tests green)
  - [x] 5.1 Diff scope (baseline, no host `git add -A -N`, one normalize_patch, worktree new files, partial-apply semantics)
  - [x] 5.2 Interrupt cleanup + teardown (real paths, Drop guards, prune guard, id-collision errors)
  - [x] 5.3 Command policy (deny-wins, BashTool routing, network/extra_packages honesty)
  - [x] 5.4 Hook timeouts ([hooks] timeout_seconds; audit done in 4.6)
  - [x] 5.5 Unified RunBudget (max_steps/max_usd/max_wallclock, BudgetExhausted, spend-cap hole, goal cost halt)
  - [x] 5.6 Failure contract (tests/run_lifecycle.rs, single changes.patch writer, conflict-marker abort, rewind warn, eval--live guard)
  - [x] 5.7 Risk-aware topology (topology after classify, High→MultiAgent, Low+critic docs)
- [x] Phase 6 — Config/docs/CI (6.1–6.6 DONE 2026-09-21: fmt+clippy+deny+audit clean, 995+ tests green)
- [x] Phase 7 — Guards/eval/release (7.1–7.6 DONE 2026-09-21: fmt+clippy+deny+audit clean, 1000+ tests green, release build proven)
  - [x] 7.1 Pipeline guard tests (tests/pipeline_guards.rs: cancel, max_rounds, tiny budget, spend cap, critic skip)
  - [x] 7.2 Run-lifecycle & envelope tests (tests/run_lifecycle.rs: json mode pure stdout, single changes.patch, failed suite verification)
  - [x] 7.3 Goal-loop & session tests (goal runner drift & budget halt, code vs conversation rewind modes, mission persistence)
  - [x] 7.4 Eval-harness honesty & fixture integrity (replay mode honesty docs, fixture hashing, tampering warnings)
  - [x] 7.5 TUI/display regression pins (muted vs TUI precedence, event buffer drain & fork, apply_session exact scope)
  - [x] 7.6 Release-path proof (cargo fmt, clippy -D warnings, deny check, audit, release build & CLI smoke)

## Rep 2 — best-in-class hardening (plan → work → verify)
- [x] Rep-2 competition parity plan (Claude Code league: reliability, E2E, errors, cost honesty)
- [x] Rep-2 tightening work (no speculative features; smallest diff per item)
- [x] Production E2E proof (mock-E2E on/off, release gate, user-testable `niki run`)

## Completion
- [x] ALL_PHASES_COMPLETE
