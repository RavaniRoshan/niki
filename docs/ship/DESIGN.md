# NIKI — ship + perform: DESIGN

Approved by the owner on 2026-10-05. Phase 0 audit and this note were produced read-only;
every claim below is backed by a command whose output was printed in that conversation.

Audit baseline: branch `master` @ `8cd81ce`. Build box: 7 GiB RAM, 16 cores, podman 4.9.3,
Harbor not installed. Audited 2026-10-05.

---

## 1. What the foundation actually is (measured, not read)

| Gate | Command | Result |
| --- | --- | --- |
| fmt | `cargo fmt --check` | exit 0 |
| clippy | `cargo clippy --all-targets --workspace -j 2` | exit 0, 0 warnings |
| unit | `scripts/test-fast.sh` | 1112 run, 1112 passed, 0 skipped |
| shell | `npx vitest run` | 22 files, 403 passed, 3 skipped |
| supply chain | `cargo deny check` | advisories, bans, licenses, sources all ok |
| audit | `cargo audit` | exit 0, 11 allowed warnings |
| journeys | `cargo test --test journeys consumer_journeys_all_pass` | 25 passed, 0 failed, 0 skipped |
| release | `cargo build --release -j 2` | exit 0, 7m15s |
| product | `scripts/demo.sh` | exit 0, branch `niki/06519d74`, verdict Approved |

Integration binaries re-run: `sandbox_teardown` 4/4 · retry paths 23/23 · `security_exec` 16/16 ·
`supply_chain_policy_has_teeth` 4/4 · `mcp_call_path` 13/13 · `full_pipeline_branch` 2/2 ·
`run_lifecycle` 14/14 · `protocol_contract` 10/10 · `serve_protocol` 14/14 · `foundation_docs` 3/3 ·
`test_groups` 8/8 · risk/verifier/artifact/revision 19/19.

## 2. Blocking defects, in the order they will be fixed

1. **The release binary is not portable.** `debian:bookworm-slim` →
   `GLIBC_2.39 not found`. Built on Ubuntu 24.04, so Ubuntu 22.04, Debian 12 and RHEL-family
   all fail. Fix: a static musl target, which has no glibc dependency at all.
2. **`tests/claims.rs` is red on `master`.** `claim_surfaces()` collects every `.md` under
   `docs/`, and the tracked mission file documents `niki bench`, which does not exist. The gate
   is correct. Owner decision recorded: **leave the gate alone**; build `niki bench` in P2.
3. **The deterministic fixture provider is unreachable.** `fixture-runtime` is off by default
   and no workflow enables it, so `niki serve --fixture` does not exist and
   `shell/test/fixture-loop.test.tsx` silently skips its three tests. Fix: build the test engine
   with the feature and add a gate that fails loudly when it is missing.
4. **No CI workflow runs the shell suite.** All 403 shell tests are ungated on a PR.
5. **A tag push creates a public, non-draft release.** `v-release.yml` has no
   `workflow_dispatch`, no draft and no approval gate.
6. **The compiled shell writes escape garbage and exits 0 with no TTY.** The PTY suite does not
   catch this because it drives the shell from source, never the compiled binary.
7. **`niki doctor` exits non-zero on a clean containerless box** while its own fix text says the
   worktree backend works there.

## 3. Wiring gaps, in priority order

`smoke_real_model.sh` → W6 flags (`--atif-out`, `--max-time`, `--max-cost`, stdin) →
compiled-shell no-TTY behaviour → W13 uninstall → W14 opt-in self-update → W2 config validation
and value provenance → W3 session export (markdown + ATIF) → W5 approval focus → W8 path-traversal
guard → W10 first-frame / idle-CPU / RSS budgets → W12 doctor terminal check → W16 arm64 CI build
→ W17 SKILL.md frontmatter.

## 4. Harbor adapter

- Dataset `terminal-bench/terminal-bench-2-1`, 89 tasks, `sha256:7d7bdc1c…` — the only 2.x with an
  official leaderboard.
- Baselines `mini-swe-agent` and `terminus-2`, both built into Harbor and both already `atif=True`.
- Shape: `BaseInstalledAgent`; `install()` writes a pinned static NIKI engine into the container;
  `run()` execs a new headless `niki agent` built on the existing `run_tool_loop`; writes ATIF;
  reports real tokens and cost; honours max-time and max-cost. No Node, no Python, no shell in
  the container.
- **The loop already exists.** `build_baseline_registry()` (`src/runtime/tools.rs:3147`) ships
  `read, write, edit, glob, grep, list, bash`, so L0 needs no new tools.
  `run_tool_loop{,_with,_spending}` (`src/runtime/tools.rs:3543/3666/3698`) is a budgeted tool
  loop, today reachable only through the experimental research stage.
  `LoopOptions.submit_artifact` is the natural hook for L1's completion gate, and `LoopSpend`
  already bills a loop that failed.
- Rules: ≥5 trials per task, multiplier 1.0, no `override_timeout_sec` / `override_cpus` /
  any resource override, an ATIF trajectory for every rewarded trial.

## 5. Levers

L0 expose the loop headless → L1 completion gate (`submit_artifact`) → L2 budget manager →
L3 loop guard → L4 generic in-memory probe → L5 PTY → L6 edit robustness → L7 context →
L8 effort schedule → L10 profiles. L9 parallel attempts stays off. Every lever: a flag, a metric,
a kill criterion, and a paired DEV ablation recorded in `EXPERIMENTS.md`.

## 6. Release design

Keep cargo-dist for the engine. Add our own short `install.sh` / `install.ps1` for the two-binary
bundle. Add a `workflow_dispatch` **draft** release with the publish step behind manual approval.
Pin every action by commit SHA.

Bun-compile spike, measured on the prebuilt `shell/dist/niki-shell`:

| Image | Result |
| --- | --- |
| ubuntu:24.04 | exit 0 |
| debian:bookworm-slim | exit 0 |
| alpine:3.20 | `missing dynamic library` |

The compiled shell is glibc-dynamic, not musl. Ubuntu and Debian are supported; Alpine is not, and
that will be documented rather than silently broken.

## 7. Standing rules for this work

- No task-specific knowledge anywhere in the engine, prompts, profiles or fixtures. A CI
  contamination test greps for benchmark identifiers and canary strings.
- No timeout, resource or config overrides in evaluation runs.
- Tuning on DEV only; at most three SEALED runs, each logged.
- No number reaches the README or any doc unless a script generated it from a stored result file.