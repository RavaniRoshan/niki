# NIKI — ship + perform: CHECKLIST

Every row's status was set from a command whose output was printed in the build conversation.
Nothing here is marked WORKS on the strength of reading code.

Status vocabulary: **WORKS** (a named test or probe passed, output printed) · **PARTIAL** (some
behaviour exists, the row's requirement is not met) · **MISSING** (does not exist) ·
**OWNER-VERIFY** (needs a real key, a real terminal, or human judgement; exact steps in
`OWNER_VERIFY.md`) · **BLOCKED** (needs an owner-only action).

Audited 2026-10-05 at `8cd81ce`; P1 re-verified the same day. Counts are measured, not typed:
`1129` unit tests (`scripts/test-fast.sh`), `418` shell tests (`./scripts/test-shell.sh`), `25`
consumer journeys, `131` integration binaries (`ls tests/*.rs`).

---

## Wiring

| Row | P | Status | Evidence / what is missing |
| --- | --- | --- | --- |
| W1 clean machine → first streamed answer | P0 | **OWNER-VERIFY** | Everything automatable is proven. Container leg passed on ubuntu:24.04: install → `niki 0.10.0` → `niki chat -m` streamed a real answer from an OpenAI-compatible endpoint, non-TTY. 25/25 journeys. `scripts/smoke_real_model.sh` exists in a real-provider and a fixture mode; the fixture mode is green in CI and was observed failing against an unreachable provider and a missing key. 418 shell tests, 0 skipped. **Exact steps in `docs/foundation/OWNER_VERIFY.md` §W1**, including both provider shapes and what a pass does not establish. |
| W2 config layering | P0 | **PARTIAL** | Three layers load with env precedence; `niki config explain` shows every tracked value with its layer (`tests/config_explain.rs` 9/9, secrets never printed); **`load()` warns and `load_file_only()` refuses on `validate_against_schema`**, checked against the schema the engine generates — `tests/config_schema_validation.rs` 10/10. Building that exposed a badly-wrong schema (4 sections undeclared, 9 unchecked, 7 incomplete) and **seven wrong test fixtures** naming fields that never existed; the schema is now generated from the structs, 23 sections, 0 gaps. **Still missing: config version migration.** |
| W3 sessions persist / resume / export | P0 | **WORKS** | Persist, resume and atomic writes proven by `state_writes_are_atomic.rs`. `niki session export [<ID>] [--format markdown\|atif] [--out PATH]` added; `tests/session_export.rs` 7/7 drives the real binary, including a message containing its own code fence, a session with no usage, and a refusal that writes no file. |
| W4 one sandbox trait | P0 | **WORKS** | `trait Sandbox` `src/sandbox/mod.rs:157`; `sandbox_teardown` 4/4. |
| W5 approvals real | P0 | **WORKS** | Shell side 20/20 `shell/test/approval.test.tsx` (safest focused, Esc denies). Engine side: `ask_permission` drives the same `PermissionRequest` path; a headless auto-approval is **logged** to stderr and `tracing`; `--permission-mode bypass` requires `--i-understand-bypass` and is refused — before the mode applies and before any model call — without it. `tests/approval_logging.rs` 11/11, including that `manual` still fails closed, an explicit Deny holds in every mode, the Planner stays read-only in every mode, and the other three modes need no acknowledgement. **Not addressed:** the TUI Shift+Tab cycle and the settings sheet still reach `bypass` without a modal. |
| W6 headless | P0 | **WORKS** | `--output-format json`, exit codes, bare⇒exit 2, and now `--max-time`/`--max-cost` (visible aliases on the same budget fields), `niki run -` for a piped task, and `--atif-out PATH` writing an ATIF trajectory on **both** the success and failure paths. Proven by `run_lifecycle` 14/14, `headless_flags` 7/7, `cargo test --lib atif` 8/8, and by a clean-container run with no TTY and no shell. |
| W7 errors and retry | P0 | **WORKS** | 23/23 across `streaming_paths_retry`, `retry_tracking`, `failover_chain`, `truncated_tool_calls`. |
| W8 security | P0 | **WORKS** | Redaction (`redact_secrets`), command policy (`security_exec` 16/16), web allowlist, no telemetry, `cargo deny`/`audit` clean, and a **path-traversal guard that does exist**: `resolve_tool_path` (`src/runtime/tools.rs:506`) refuses `..`, compares against a canonicalised root so a symlinked root cannot lie, canonicalises the deepest existing ancestor so `write` to a new file under a symlinked parent is still caught, and is applied to `read`/`write`/`edit`/`patch`/`grep`; `glob` refuses absolute and `..` patterns. 46 tests in `runtime::tools::tests`, including `traversal_is_refused_rather_than_normalised` and `a_symlink_out_of_the_tree_does_not_launder_a_write`. **The Phase 0 audit recorded this row as PARTIAL with 'no path-traversal guard' — that was wrong**, the grep behind it was scoped to `src/safety/` and missed `src/runtime/`. |
| W9 terminal behaviour | P0 | **WORKS** | 19/19 PTY tests, 413/413 shell tests, 0 skipped. The no-TTY case is now covered too: `shell/test/no-tty.test.ts` (7) asserts zero control bytes, a non-zero exit and an actionable message, and was observed failing with the fix reverted. The PTY suite still drives the shell from source; the compiled binary is covered by the new test and by a clean-container run. |
| W10 speed budgets | P0 | **WORKS** | `shell/test/perf-budgets.test.ts` 5/5 **enforces** the budgets and prints every measurement: first frame ≤150 ms at seven sizes (measured 2.56–4.75 ms), idle render does not grow with repetition (5.185 vs 6.069 ms/frame), per-token cost flat across a 10× transcript (ratio 1.25, budget 1.5), and **memory — measured for the first time**: 0.6 MiB growth / 174.8 MiB resident against 400 / 1024 MiB ceilings. Observed failing when the budget was lowered. `tui_perf.rs` in the engine remains warn-only by its own comment; the enforced budgets live in the shell, where the interface is. |
| W11 MCP bridge | P1 | **WORKS** | `mcp_call_path` 13/13. |
| W12 `niki doctor` | P1 | **WORKS** | J22 passes. Severity corrected and a terminal check added, both measured in clean containers: with git and no container runtime → **6 passed, 21 warnings, 0 failed, exit 0**, warning that `--backend worktree` works; with neither → **2 failed, exit 1**, because then no backend can run. The new `terminal capability` check distinguishes a real terminal, `TERM=dumb` (supported, reduced) and no TTY (warns, names the headless commands). `cargo test --lib cli::doctor` 12/12; 25/25 journeys. |
| W13 uninstall | P1 | **WORKS** | `scripts/uninstall.sh`, proven by `tests/uninstall.rs` — 7/7 against a real fake `$HOME`: removes the binaries, keeps data by default, `--purge` and `--dry-run`, idempotent, honours `NIKI_INSTALL_DIR`, refuses an install dir of `/`. |
| W14 self-update | P1 | **WORKS** | `dist-workspace.toml` ships `niki-update` (opt-in, cargo-dist verifies checksums). `tests/update_is_opt_in.rs` 7/7 asserts the engine never invokes an updater, no workflow runs one automatically, `install.sh` only installs, and `uninstall.sh` removes the updater — with a decoy proving the scanner can fail. |
| W15 full pipeline | P0 | **WORKS** | `full_pipeline_branch` 2/2, including the negative half. `scripts/demo.sh` exit 0. |
| W16 platform matrix | P1 | **PARTIAL** | `aarch64-unknown-linux-gnu` now builds on `ubuntu-24.04-arm`, alongside x64 Linux, both macOS targets and a Windows job that starts the binary. `tests/target_matrix.rs` 5/5 asserts both directions — every shipped target is built, and every built target is shipped — and was observed failing before the workflow was fixed. **Missing:** an explicit unsupported-combination document (musl, Alpine). |
| W17 SKILL.md loading | P1 | **WORKS** | Frontmatter is parsed and `.claude/skills`, `.agents/skills` and `~/.agents/skills` are discovered, with NIKI's own promoted skills winning a collision. No new dependency. `src/skills/mod.rs` module tests 16/16 including a genuine third-party header; `tests/skill_md_compat.rs` 5/5, with one test loading **25 real skills** from this machine's own `~/.agents/skills` and requiring every one to yield a description and a body. |

## Release

| Row | P | Status | Evidence / what is missing |
| --- | --- | --- | --- |
| R1 versioning | P0 | **PARTIAL** | Semver at `Cargo.toml:8` (0.10.0); tag-driven. **Missing:** conventional-commit changelog generation; no `v0.10.0` tag exists. |
| R2 CI per PR | P0 | **PARTIAL** | fmt, clippy, nextest, cargo-deny, cargo-audit, MSRV, matrix, windows, demo, and now a `shell` job that runs the 406-test Ink suite and the fixture-mode first-answer smoke. **Missing:** npm audit; **116 of the 120 pre-existing `uses:` lines are SHA-pinned** — the 4 added by the new job are; the rest of the file still uses tags. |
| R3 build matrix | P0 | **PARTIAL** | 5 targets in `dist-workspace.toml`. **Missing: musl everywhere** — all Linux targets are gnu. Compiled shell runs on ubuntu and debian, **fails on alpine**. |
| R4 archives | P0 | **PARTIAL** | cargo-dist archives + checksum, asserted by `dist_install.rs`. **Missing:** attestations, SBOM, NOTICE, version in the filename. |
| R5 installers | P0 | **PARTIAL** | `scripts/install.sh` is real: checksum-verified, no sudo, idempotent, `--version` pinned, 12 tests. `scripts/uninstall.sh` added and proven. **Missing:** repo-level `install.ps1`, uninstall on Windows. |
| R6 stable URLs | P0 | **PARTIAL** | README points at a raw `install.sh`. **Missing:** `docs/INSTALL.md`; no `latest/download` URL; Pages ships docs only. |
| R7 npm | P1 | **MISSING** | `shell/package.json` is private, no launcher, no per-platform optional deps. |
| R8 Homebrew | P1 | **PARTIAL** | `homebrew/niki.rb` real and cross-checked. **Blocked:** the tap repo is owner-only. |
| R9 Docker / GHCR | P1 | **MISSING** | No image workflow; `docker/Dockerfile.agent` does not install NIKI. |
| R10 winget / scoop | P2 | **PARTIAL** | `scoop/niki.json` real. `winget/RavaniRoshan.niki.yaml` is a 4-line stub with no URL, installer or hash. |
| R11 docs | P0 | **PARTIAL** | README, docs site, CONTRIBUTING, SECURITY, CODE_OF_CONDUCT, templates exist. **Missing:** a generated results section. **Every README number is hand-typed and one is already stale** — "946 unit tests" against 1112 measured. |
| R12 rehearsal | P0 | **MISSING** | No `workflow_dispatch`, no draft, no approval gate. A tag push creates a public non-draft release immediately. |
| R13 rollback | P1 | **MISSING** | Nothing. |
| R14 signing | P2 | **BLOCKED** | Needs paid certificates. Document Gatekeeper/SmartScreen behaviour instead. |

## Evaluation

| Row | Status |
| --- | --- |
| Harbor installed | **MISSING** — not on this box |
| `bench/` rig | **MISSING** — no directory, no adapter, zero occurrences of "ATIF" in the repo |
| DEV/SEALED split | **MISSING** |
| Baselines | **MISSING** — `mini-swe-agent` and `terminus-2` are both built into Harbor 0.24.0 |
| Budget | **BLOCKED** — needs the owner's USD figure and model choice |

## Foundation gates (all re-run 2026-10-05)

| Gate | Result |
| --- | --- |
| `cargo fmt --check` | exit 0 |
| `cargo clippy --all-targets --workspace -j 2` | exit 0, 0 warnings |
| `scripts/test-fast.sh` | 1112 passed, 0 skipped |
| `npx vitest run` | 403 passed, 3 skipped |
| `cargo deny check` | advisories, bans, licenses, sources all ok |
| `cargo audit` | exit 0, 11 allowed warnings |
| `cargo test --test journeys consumer_journeys_all_pass` | 25 passed, 0 failed, 0 skipped |
| `cargo build --release -j 2` | exit 0, 7m15s |
| `./target/release/niki --version --help` | `niki 0.10.0` |
| `scripts/demo.sh` | exit 0, branch created, verdict Approved |

**Known red gate:** `tests/claims.rs::every_niki_command_in_every_documented_surface_exists` fails
because the tracked mission file documents `niki bench`, which does not exist. The gate is
correct. Owner decision: leave it alone and build `niki bench` in P2 rather than narrowing the
scan.