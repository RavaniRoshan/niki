# NIKI Claims Audit

Every public marketing claim must be reproducible from the repository (funnel-plan rule).
This document maps each headline claim to the code that backs it. **Last verified: 2026-08-15
(commit `master` pre-v0.4.0).**

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

- [ ] Agent artifact isolation still holds after any `pipeline.rs` refactor.
- [ ] Deny-list contents match the copy (re-run `default_global_deny_list`).
- [ ] `network_disabled` default unchanged.
- [ ] Release assets = 3 `.tar.gz` + `checksums.txt`; `sha256sum -c` passes.
- [ ] `niki --version` prints the launch version on every target.
