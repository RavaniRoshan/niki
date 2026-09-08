# NIKI Claims Audit

Every public marketing claim must be reproducible from the repository (funnel-plan rule).
This document maps each headline claim to the code that backs it. **Last verified: 2026-09-07
(goal/readiness-b7d1e4, post-0.7.0-merge; prior verification 2026-08-15 pre-v0.4.0).**

## Claims that hold

| Claim | Status | Evidence |
|-------|--------|----------|
| Four role-isolated agents: Planner → Coder → Tester → Reviewer | ✅ | `src/agents/mod.rs` (planner/coder/tester/reviewer); `src/orchestrator/pipeline.rs:97-100` |
| An adversarial **Red** agent can probe the diff *before* the Reviewer (opt-in, **off by default**) | ✅ | `src/orchestrator/pipeline.rs` injects `AgentRole::Red` when `red_blue.enabled`; default `false` (`src/config/types.rs:359`) — enable via `[red_blue] enabled = true` |
| Reviewer works from the prior stage's **artifact**, not shared mutable state | ✅ | `isolation_sources_for()` at `src/orchestrator/pipeline.rs:233` passes prior stage outputs as artifacts; structural guard at `:159` ensures Red/Reviewer receive artifact-only input |
| **Hermetic by default** — egress blocked unless allowed | ✅ | `network_disabled: true` default at `src/config/types.rs:908`; `network_allowlist` re-opens egress |
| Container sandbox with dropped caps / read-only mounts | ✅ | `src/config/types.rs` `CapDrop` / read-only rootfs config; `HermeticityViolation` at `src/errors.rs:20` |
| **git2 is local-only**; working tree untouched unless `--backend worktree` | ✅ | `src/output/git.rs` `Repository::open(repo_path)` (local); worktree backend prints host-privilege warning (documented) |
| Spend cap is **hard-enforced** (aborts before a branch is created) in v0.4.0+ | ✅ | `src/orchestrator/pipeline.rs` `enforce_spend_cap` checks cumulative stage cost after every stage; `general.spend_cap_usd` in `README.md:175-177` |
| BYOK, no telemetry | ✅ | README security posture `README.md:166-179`; no analytics calls by design |
| Secret redaction (incl. `?key=` / Google keys) | ✅ | `CHANGELOG.md` 0.3.0 Security; redaction in report/artifact rendering |

## New claims since v0.4.0 (verified 2026-09-07)

| Claim | Status | Evidence |
|-------|--------|----------|
| `niki plan` researches without executing; `niki run --plan` executes the reviewed spec | ✅ | `src/cli/plan.rs` delegates dry-run; `plan_override_json` skips Planner (`pipeline.rs`); E2E-verified with mock LLM (plan→no branch, approve→branch) |
| Failing test suites block the branch unless `--force` | ✅ | Red-suite gate in `src/cli/run.rs`; E2E-verified block/force/green paths; `tests/` assert gate semantics |
| Tests carry oracle provenance (`spec`/`derived`/`property`) | ✅ | `schemas/test_report.schema.json` + `OracleSource` in `artifacts/types.rs`; tester/reviewer prompt rules |
| Unpriced models warn instead of silently costing $0.00 | ✅ | `is_unpriced()` + `unpriced*` report marker + `PRICE_TABLE_AS_OF` freshness test (`src/cost.rs`) |
| Approval tool denies by default; ask tool never invents answers | ✅ | `AskUserTool`/`ApprovalTool` in `src/runtime/mod.rs`; non-TTY deny/fail covered by tests |
| Lifecycle hooks (`[hooks.commands]`) can block runs fail-closed | ✅ | `HookBus::from_map` + pipeline wiring (`PreTaskStart/PreAgentStart/PostAgentStop`); `tests/hooks_lifecycle.rs` (4 integration tests) |
| `--output-format json` emits a stable, pipe-pure envelope | ✅ | Display mute + captured git stdio; verified single-line JSON parse against mock runs |
| `niki eval` writes a disclosure manifest with every run | ✅ | `eval-manifest.json` (date/version/commit/dirty/mode/costs); live-verified on 23-case replay |
| TUI status grammar is unified; motion is reduced-gated | ✅ | `display/components/status.rs` (Running≠Paused test); `display/motion.rs` unit tests; `tests/visual/` 12 reference frames at 0.00% self-diff |

## Claims that were OVERSTATED — fixed in copy

The deny-list does **not** block plain `git push` or arbitrary `rm`. It blocks a specific
set. The original copy ("`git push`, `rm -rf`, `curl|sh` are blocked by policy") was too broad.

| Original claim | Reality (`default_global_deny_list`, `src/config/types.rs:116`) | Corrected copy |
|----------------|------------------------------------------------------------------|----------------|
| "`git push` … blocked" | Blocks `git push --force` / `git push -f` only | "force-push is blocked" |
| "`rm -rf` … blocked" | Blocks `rm -rf /` and `rm -rf /*` only | "`rm -rf /` (root) is blocked" |
| "`curl|sh` blocked" | Blocks `curl \| sh`, `curl \| bash`, `wget \| sh`, `wget \| bash` | accurate — keep |

**Action taken:** show-hn.md, social.md, ph-assets.md, and README now use the
narrowed wording. The Homebrew/Scoop/Winget "Windows" claims were removed (Windows is not
built — see `release.yml`, which produces 3 Unix targets only).

## Claims to re-verify before each launch

- [x] (2026-09-07) Agent artifact isolation still holds — re-verified:
  `isolation_sources_for()` now mirrors wiring exactly (Synthesizer sees
  Planner+Coder, SecurityAuditor sees Planner+Coder; Red sees evidence-only
  projections via `red_evidence_json`). Record ≠ aspiration anymore.
- [x] (2026-09-07) Deny-list contents match the copy.
- [x] (2026-09-07) `network_disabled` default unchanged (`true`).
- [ ] Release assets = 3 `.tar.gz` + `checksums.txt`; `sha256sum -c` passes.
  **OPEN — see Phase 1c: v0.6.0 tag shipped zero assets.**
- [ ] `niki --version` prints the launch version on every target.
