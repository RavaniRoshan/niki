# NIKI — ship + perform: EXPERIMENTS

Ablation protocol for harness levers (L0-L10) per PACK.md.
Every lever is evaluated on the DEV split against L0 baseline.
Kept or killed on measured paired bootstrap differences.

---

## Lever Registry

| # | Lever | Mechanism | Flag | Metric | Decision |
|---|---|---|---|---|---|
| **L0** | Minimal strong baseline loop | Read, write, edit, bash, glob, grep, list via `run_tool_loop` | Baseline (`niki agent`) | Control | **KEPT** (Baseline) |
| **L1** | Completion gate & self-verification | Requirement extraction into evidence ledger; verifier call | `--lever-completion-gate` | False-done rate, resolve rate | Pending DEV ablation |
| **L2** | Wall-clock & token budget manager | 85% time wrap-up trigger, per-command timeouts | `--lever-budget-manager` | Timeout rate | Pending DEV ablation |
| **L3** | Loop guard | Detect repeated command failures, oscillate diffs | `--lever-loop-guard` | Stalled run rate | Pending DEV ablation |
| **L4** | Generic environment onboarding | In-memory environment probe (no task files written) | `--lever-onboarding` | First-turn error rate | Pending DEV ablation |
| **L5** | Persistent PTY tool | Long-running interactive commands, screen snapshots | `--lever-pty` | Interactive solve rate | Pending DEV ablation |
| **L6** | Edit robustness | Unique match + fuzzy fallback + instant syntax check | `--lever-edit-robust` | Edit rejection rate | Pending DEV ablation |
| **L7** | Context management | Tool output compaction, typed task state outside transcript | `--lever-context` | Token efficiency | Pending DEV ablation |
| **L8** | Reasoning-effort schedule | High for plan/verify, medium for execution | `--lever-effort-schedule` | Solve per dollar | Pending DEV ablation |
| **L9** | Parallel attempts | Multi-attempt selector for hard tasks | `--lever-parallel` | Pass@N vs cost | **KILLED** (Off by default per PACK.md) |
| **L10** | Per-model profiles | Model-specific prompt variants and parameters | `--lever-model-profiles` | Model specialization | Pending DEV ablation |

---

## Ablation Log

*Ablations will be logged here as DEV split runs conclude.*
