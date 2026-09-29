# AGENTS.md — NIKI

Rust CLI (edition 2024, MSRV 1.88). Multi-agent coding pipeline: Planner → Coder → Tester → Reviewer run in hermetic sandboxes and hand back a `niki/<id>` git branch. Binary is `niki`.

## Build & verify

- Format + lint + test, in this order (mirrors CI in `.github/workflows/ci.yml`):
  - `cargo fmt --check`  (CI uses `--check`; locally `cargo fmt`)
  - `cargo clippy --all-targets`  (must be **warning-free** — CI enforces it)
  - `cargo test --verbose`
  - `cargo build --release` (CI then runs `./target/release/niki --version --help`)
- Single test: `cargo test <test_name>`.
- `git2` uses `vendored-libgit2`, so no system libgit2 is required to build.

## Tests — use nextest

```
cargo nextest run -j 2      # the whole suite: 1503 tests in ~60s
./scripts/verify-suite.sh   # same thing, with a memory guard
```

Measured on this box: the full suite is 64s warm, one command, all binaries.
`cargo test --tests` is much slower and, run all at once, will not fit in RAM
here. Install once with `cargo install cargo-nextest --locked`.

**Run the full suite in CI, not locally.** The `Tests` job in `.github/workflows/ci.yml`
runs the whole thing on a free `ubuntu-latest` runner and gates the PR, alongside
Integration, TUI Smoke and CodeQL. Locally, run the binaries you touched:

```
cargo nextest run -j 2 -E 'binary(pipeline_guards) + binary(kb_pipeline)'
cargo nextest run -j 2 -E 'test(<substring>)'
```

A full local run links every test binary, which is the largest memory event in
the build and the thing that starves other sessions on this box. It is there for
when you need it before pushing, not as the inner loop.

nextest earns its place twice over. It parallelises *across* test binaries
while `.config/nextest.toml` keeps the heavy and heap binaries serialised by
mechanism — the same split Codex uses. And it runs process-per-test, so a test
that wedges is killed at its configured timeout instead of hanging the run:
that is how a real infinite loop was caught here, as a binary that had been
running ten minutes and should take 1.4 seconds.

Without nextest, `scripts/verify-suite.sh` falls back to one binary at a time
and refuses to start a link when free memory is low — a skip is reported as a
failure, never as a pass.

## Low-RAM / constrained machines

This box can have only a few GiB free, and another session shares it. Never run
the full pipeline (clippy + all tests + release) concurrently — serialize
stages and cap parallelism so peak RSS stays bounded.

The specific failure to avoid: a `cargo` link (peaks 1.5–2 GB) running at the
same time as a live-model sweep, which holds ~1.9 GB. That is what crashed this
box twice. Serialize them.

- Limit compile jobs: `CARGO_BUILD_JOBS=2` (or `cargo … -j 2`) for `build`/`clippy`/`test`. Default jobs = nproc and will OOM on 8 GiB-class hosts.
- Prefer `cargo check` / `cargo clippy` over `cargo build --release` while iterating; only build release for the final verify (release codegen is the largest single peak).
- Do **not** run `cargo test` (whole suite) when RAM is tight. Prefer:
  - one integration binary: `cargo test --test run_lifecycle -j 2 -- --test-threads=1`
  - unit only: `cargo test --lib -j 2 -- --test-threads=1`
  - a single case: `cargo test <test_name> -- --exact --nocapture`
- Cap test threads: always pass `-- --test-threads=1` (or `2` max) on low-RAM hosts; default threads ≈ nproc and each test binary + fixture repo multiplies RSS.
- Never parallelize separate cargo commands (e.g. clippy and test at once); they contend on the target dir lock *and* double peak memory.
- Heavy suites to avoid in full parallel (`--test-threads` high): `tui_navigation`, `tui_perf`, `visual_layout_check`, `kb_pipeline`, `runtime_benchmarks` — run them alone, serially.
- If a cargo/rustc process is near OOM: drop `-j`, drop `--all-targets`, re-run the smallest failing `--test <bin>`; do not re-run the full suite.
- Mock e2e (`mock_llm.py` + one `niki run`) is fine; do not stack multiple mock runs or extra `cargo build` in the same shell.

### The fast loop vs. the gate

The commands above are the *gate*. They are also tens of minutes, which is why they are
not what you reach for when you want to know if an edit compiles. `scripts/dev-loop.sh`
is the inner loop:

- `./scripts/dev-loop.sh check` — fmt + clippy on lib+bins only (skips `--all-targets`, the most expensive lint step)
- `./scripts/dev-loop.sh test <binary>` — one integration binary, `--test-threads=1`
- `./scripts/dev-loop.sh changed` — which binaries a given diff puts at risk
- `./scripts/dev-loop.sh watch` — re-check and re-test on change
- `./scripts/dev-loop.sh fast` / `gate` — the cheap set / the full pre-push gate

### Which binaries must not run concurrently

`.config/test-binary-groups` is the **single source of truth** for this. One list,
two consumers:

- `scripts/test-layer.sh` reads it directly (the plain-cargo runner);
- `scripts/gen-nextest-groups.py` compiles it into `.config/nextest.toml` as
  `[[profile.default.overrides]]` with `test-group = 'heavy' | 'heap'`, so
  `cargo nextest run` serialises exactly the same binaries.

Both groups must stay non-empty: `[test-groups.heavy] max-threads = 1` with no
overrides is valid TOML that serialises **nothing**, and that is precisely how the
heavy binaries came to run fully parallel in CI. `tests/test_groups.rs` fails if
the list and the generated overrides drift, if a listed binary has no
`tests/<name>.rs`, if a binary is in two groups, or if `test-layer.sh` grows a
second hardcoded list. Run `python3 scripts/gen-nextest-groups.py` after editing
the list; `--check` is wired into the gate.

## Critical quirk: prompts and schemas are baked into the binary

`src/lib.rs:138-139` compiles `prompts/` and `schemas/` into the binary at build time via `include_dir!`. Editing `prompts/*.md` or `schemas/*.json` has **no effect until you rebuild** (`cargo build`/`cargo run`). If your prompt/schema change "doesn't take", rebuild first. Runtime reads from embedded copies, not the source files.

## Tests & end-to-end

- Unit/integration tests live in `tests/` and `src/**` (`#[test]`). Dev-deps include `wiremock`, `assert_cmd`, `tempfile`.
- The real pipeline needs an LLM. For local e2e without keys or a container runtime, run the mock server and use the worktree backend:
  - `python3 tests/integration/mock_llm.py &` (serves on `:8080`)
  - `./target/release/niki run 'Add health endpoint' --backend worktree --quiet --project <git_repo>`
- `--backend worktree` (git worktree + local process) needs **no** container runtime. The default `docker` backend does, plus the pre-baked image: `podman build -t niki-sandbox:24.04 -f docker/Dockerfile .` (a plain `ubuntu:24.04` lacks git/node/npm/python3 and fails the sandbox's tool check).

## Supply chain (CI gate)

- `cargo deny check` and `cargo audit` run in CI. Adding a dependency whose license isn't in `deny.toml`'s tight allow-list fails CI — extend the list deliberately, don't copy a broad allow-list.
- Releases are produced by `cargo dist` (`dist-workspace.toml`); do not hand-roll release binaries.

## Architecture / extension points

- Entrypoint: `src/main.rs` → `src/cli/`. Core modules: `agents/`, `orchestrator/`, `sandbox/` (Podman/Docker/worktree backends), `llm/`, `runtime/` (tool registry + baseline tools), `artifacts/` (typed + JSON-schema validated), `config/`, `output/`, `repo_intel/` (deterministic manifest), `risk/` (tier classifier), `knowledge/` (KB, history miner, structural index, context pack), `mcp/`, `memory/`, `session/`, `goal/`, `persistence/`, `acp/`, `control_plane/`, `audit/`, `mission/`, `eval/`, `permissions/`, `safety/`, `tools/`, `commands/`, `event/`, `activity/`, `cost.rs`, `recommend.rs`.
- Add a tool: implement the `Tool` trait in `src/runtime/mod.rs`, register in `build_baseline_registry()`, add tests.
- Add a provider: implement `LlmProvider` in `src/llm/provider.rs`; add a match arm in `create_provider()` in `src/llm/provider.rs`. `src/llm/anthropic.rs` is the reference impl.
- Agent prompts are Minijinja templates in `prompts/*.md`; add an agent role → new prompt file + schema in `schemas/` + wire into `src/orchestrator/pipeline.rs`. New `AgentRole` variants require arms in `role_prompt`/`parse_role`/`isolation_sources_for`/`run_role`, `artifact_json_name`, display theme/labels, `memory/store.rs`, `recommend.rs`, and `cli/run.rs::role_filename` (see the Critic wiring as reference).
- Risk-gated stages live in `apply_risk_stages()` (`pipeline.rs`): explicit `[pipeline].stages` is never rewritten; the Critic runs once post-loop via `run_bookkept_stage()`, never inside the revision loop.
- Provenance (`orchestrator/provenance.rs`) and reflection (`orchestrator/reflect.rs`) are best-effort: warn, never fail the run.
- Prefer typed `NikiError` over bare `.unwrap()` on user-facing paths.

## Config & secrets

- `niki.toml` is git-ignored; keys also come from env vars (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GOOGLE_API_KEY`, providers via `<PROVIDER>_API_KEY` / `_BASE_URL` / `_MODEL`). Env vars override `niki.toml`. Never commit secrets; `.niki/` (run artifacts) is also git-ignored.
- Copy `niki.example.toml` → `niki.toml` for a full config reference.

See `CONTRIBUTING.md`, `README.md` (CLI reference, project structure), and `docs/content/` for deeper detail.
