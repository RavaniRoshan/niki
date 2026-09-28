# NIKI Claims Audit

Every public marketing claim must be reproducible from the repository (funnel-plan rule).
This document maps each headline claim to the code that backs it. **Last verified: 2026-09-28 (v0.9.0), partial.**

**What this pass covered, stated plainly — because a claims document that implies
more coverage than it has is the problem it exists to solve.** Re-verified every
row whose evidence this release changed, plus the two open items. **Not**
re-verified from scratch: the packaging, install-path, cost-table and BYOK rows,
which nothing here touched and which the 2026-09-07 pass covered.

Line numbers in this file are a general hazard: they point into a 5,000-line
file edited heavily since 2026-09-07. `tests/claims_audit.rs` now checks that
every `path:line` citation resolves, because a citation nobody can follow is a
claim nobody can check.

## Claims that hold

| Claim | Status | Evidence |
|-------|--------|----------|
| Four role-isolated agents: Planner → Coder → Tester → Reviewer | ✅ | `src/agents/mod.rs` (planner/coder/tester/reviewer); `src/orchestrator/pipeline.rs:97-100` |
| An adversarial **Red** agent can probe the diff *before* the Reviewer (opt-in, **off by default**) | ✅ | `src/orchestrator/pipeline.rs` injects `AgentRole::Red` when `red_blue.enabled`; default `false` (`src/config/types.rs:372`) — enable via `[red_blue] enabled = true` |
| Reviewer works from the prior stage's **artifact**, not shared mutable state | ✅ | `isolation_sources_for()` at `src/orchestrator/pipeline.rs:398` passes prior stage outputs as artifacts; structural guard ensures Red/Reviewer receive artifact-only input |
| **Hermetic by default** — egress blocked unless allowed | ✅ | `network_disabled: true` default at `src/config/types.rs:1382`; `network_allowlist` re-opens egress |
| Container sandbox with dropped caps / read-only mounts | ✅ | `src/config/types.rs` `CapDrop` / read-only rootfs config; `HermeticityViolation` in `src/errors.rs` |
| **git2 is local-only**; working tree untouched unless `--backend worktree` | ✅ | `src/output/git.rs` `Repository::open(repo_path)` (local); worktree backend prints host-privilege warning (documented) |
| Spend cap is **hard-enforced** (aborts before a branch is created) | ✅ | `enforce_spend_cap` (`src/orchestrator/pipeline.rs:1330`), called from `finish_stage` after **every** stage on **every** path. Two paths — the Planner's and the Synthesizer's — previously omitted the call, so the cap was enforced one stage late there; it is now one implementation and one call site |
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
| `--output-format json` emits a stable, pipe-pure envelope | ✅ | `result_envelope` (`src/cli/run.rs`); display mute + captured git stdio so only JSON reaches stdout; parsed by `tests/run_lifecycle.rs` against a real mock run |
| `niki eval` writes a disclosure manifest with every run | ✅ | `eval-manifest.json` written by `src/cli/eval.rs` (date/version/commit/dirty/mode/costs); replayed over the fixture corpus in `tests/kb_pipeline.rs` |
| A verdict carries its **provenance** — a run no independent stage reviewed cannot report an approval | ✅ | `RunOutcome` (`src/artifacts/types.rs`), derived at the single `PipelineResult` build point in `src/orchestrator/pipeline.rs`; the JSON envelope exposes `outcome` and `independently_reviewed`; `tests/run_lifecycle.rs` asserts an unreviewed run never claims a pass and a reviewed one names its reviewer |
| The security audit **informs** the review rather than following it | ✅ | `resolve_stages` / `apply_risk_stages` in `src/orchestrator/pipeline.rs` inject the `SecurityAuditor` ahead of the Reviewer on both paths; its verdict reaches the Reviewer's context, and an explicit `Rejected` sets a hold a later approval cannot overturn. Previously the auditor ran *after* the Reviewer and a rejection applied only when no Reviewer existed — i.e. never in the normal configuration |
| Network egress requires approval outside an explicit bypass | ✅ | `ToolRegistry::permission_denial` (`src/runtime/tools.rs`); `web_fetch`/`web_search` declare `Allow` as a tool default, so the rule lives in the enforcement path. It previously lived in `permissions::resolve_tool`, which no product code called |
| The TUI's drawn geometry is the geometry it hit-tests | ✅ | One source per region — `bands()`, `composer_split()`, `tool_card_block()` and the shared permission/palette geometry in `src/display/`. Each replaced a second, disagreeing computation |
| TUI status grammar is unified; motion is reduced-gated | ✅ | `display/components/status.rs` (Running≠Paused test); `display/motion.rs` unit tests; `tests/visual/` 12 reference frames, re-blessed on the CI runner 2026-09-28 and human-reviewed — the render is environment-dependent, and a locally blessed set fails the gate by 6.6-9.4% of pixels |

## Claims that were OVERSTATED — fixed in copy

The deny-list does **not** block plain `git push` or arbitrary `rm`. It blocks a specific
set. The original copy ("`git push`, `rm -rf`, `curl|sh` are blocked by policy") was too broad.

| Original claim | Reality (`default_global_deny_list`, `src/config/types.rs` (`default_global_deny_list`)) | Corrected copy |
|----------------|------------------------------------------------------------------|----------------|
| "`git push` … blocked" | Blocks `git push --force` / `git push -f` only | "force-push is blocked" |
| "`rm -rf` … blocked" | Blocks `rm -rf /` and `rm -rf /*` only | "`rm -rf /` (root) is blocked" |
| "`curl|sh` blocked" | Blocks `curl \| sh`, `curl \| bash`, `wget \| sh`, `wget \| bash` | accurate — keep |

**Action taken:** show-hn.md, social.md, ph-assets.md, and README now use the
narrowed wording. The Homebrew/Scoop/Winget "Windows" claims were removed in
September 2026 on the grounds that Windows was not built.

**That reason is no longer true, and the claim is now understated rather than
overstated.** `dist-workspace.toml` lists `x86_64-pc-windows-msvc` among the
release targets, the published v0.8.0 release ships a Windows `.zip` plus
Scoop and Winget manifests, and a `Windows (build + smoke)` CI job compiles and
runs it. Re-adding the Windows install claim is a copy change for whoever owns
the install docs, not something to assert from here.

## Claims to re-verify before each launch

- [x] (2026-09-07) Agent artifact isolation still holds — re-verified:
  `isolation_sources_for()` now mirrors wiring exactly (Synthesizer sees
  Planner+Coder, SecurityAuditor sees Planner+Coder; Red sees evidence-only
  projections via `red_evidence_json`). Record ≠ aspiration anymore.
- [x] (2026-09-07) Deny-list contents match the copy.
- [x] (2026-09-07) `network_disabled` default unchanged (`true`).
- [x] (2026-09-28) Release assets exist and match what the installers
  reference. **This item was written against a stale expectation**: it asked for
  "3 `.tar.gz` + `checksums.txt`", but dist publishes `.tar.xz` with a per-asset
  `.sha256`, and the published v0.8.0 release carries 21 assets across 5
  targets. Verified against the live release API. The same `.tar.gz`/`.tar.xz`
  mismatch had made the `Manifest parity` CI job check only the Windows `.zip`
  URLs and report success while four of six release URLs went unverified.
- [x] (2026-09-28) `niki --version` prints the build version: `niki 0.9.0`.
  This verifies the *local* binary; per-target verification needs a run of each
  published asset, which is a release-time check, so the strong form stays open.
- [ ] **OPEN since 2026-09-28:** the marketing screenshots in
  `assets/screenshots/` do not show what the product does, and nothing uses
  them. `diff.png` is named for the diff view and shows the **first-run
  onboarding modal** over it; `cost.png` has the same problem. Both have body
  text running past the right edge of the window chrome, so the crop is wrong as
  well as the content. They are referenced by neither the README nor the
  marketing site — only by `research/marketing-asset-pipeline.md`, which
  describes how they *should* be generated. So today they are orphaned *and*
  misleading, which is worse than absent: a reader who finds one concludes the
  product looks like that.
  Not fixed here: producing marketing-styled captures (window chrome, shadows)
  needs the VHS + freeze pipeline the research document already prescribes, and
  the same environment-dependence that makes the visual gate unrunnable locally
  applies. The verified real frames for the current UI are
  `tests/visual/reference/` (12, re-blessed on the CI runner and
  human-reviewed 2026-09-28) — those are what a screenshot should be drawn from.

- [ ] **OPEN since 2026-09-28:** the `Visual regression (VHS)` references are
  re-blessed and green, but the gate cannot be made green from a developer
  machine — the render differs from the CI runner's by 6.6-9.4% of pixels. The
  documented recovery is `gh workflow run ci.yml -f regen=true` plus a human
  looking at the frames. It belongs in the contributor guide; anyone who hits it
  will otherwise conclude the gate is broken.
