# AGENTS.md — NIKI

Rust CLI (edition 2024, MSRV 1.85). Multi-agent coding pipeline: Planner → Coder → Tester → Reviewer run in hermetic sandboxes and hand back a `niki/<id>` git branch. Binary is `niki`.

## Build & verify

- Format + lint + test, in this order (mirrors CI in `.github/workflows/ci.yml`):
  - `cargo fmt --check`  (CI uses `--check`; locally `cargo fmt`)
  - `cargo clippy --all-targets`  (must be **warning-free** — CI enforces it)
  - `cargo test --verbose`
  - `cargo build --release` (CI then runs `./target/release/niki --version --help`)
- Single test: `cargo test <test_name>`.
- `git2` uses `vendored-libgit2`, so no system libgit2 is required to build.

## Critical quirk: prompts and schemas are baked into the binary

`src/lib.rs:130-131` compiles `prompts/` and `schemas/` into the binary at build time via `include_dir!`. Editing `prompts/*.md` or `schemas/*.json` has **no effect until you rebuild** (`cargo build`/`cargo run`). If your prompt/schema change "doesn't take", rebuild first. Runtime reads from embedded copies, not the source files.

## Tests & end-to-end

- Unit/integration tests live in `tests/` and `src/**` (`#[test]`). Dev-deps include `wiremock`, `assert_cmd`, `predicates`, `tempfile`.
- The real pipeline needs an LLM. For local e2e without keys or a container runtime, run the mock server and use the worktree backend:
  - `python3 tests/integration/mock_llm.py &` (serves on `:8080`)
  - `./target/release/niki run 'Add health endpoint' --backend worktree --quiet --project <git_repo>`
- `--backend worktree` (git worktree + local process) needs **no** container runtime. The default `docker` backend does, plus the pre-baked image: `podman build -t niki-sandbox:24.04 -f docker/Dockerfile .` (a plain `ubuntu:24.04` lacks git/node/npm/python3 and fails the sandbox's tool check).

## Supply chain (CI gate)

- `cargo deny check` and `cargo audit` run in CI. Adding a dependency whose license isn't in `deny.toml`'s tight allow-list fails CI — extend the list deliberately, don't copy a broad allow-list.
- Releases are produced by `cargo dist` (`dist-workspace.toml`); do not hand-roll release binaries.

## Architecture / extension points

- Entrypoint: `src/main.rs` → `src/cli/`. Core modules: `agents/`, `orchestrator/`, `sandbox/` (Podman/Docker/worktree backends), `llm/`, `runtime/` (tool registry + baseline tools), `artifacts/` (typed + JSON-schema validated), `config/`, `output/`.
- Add a tool: implement the `Tool` trait in `src/runtime/mod.rs`, register in `build_baseline_registry()`, add tests.
- Add a provider: implement `LlmProvider` in `src/llm/provider.rs`; add a match arm in `create_provider()` in `src/llm/mod.rs`. `src/llm/anthropic.rs` is the reference impl.
- Agent prompts are Minijinja templates in `prompts/*.md`; add an agent role → new prompt file + schema in `schemas/` + wire into `src/orchestrator/pipeline.rs`.
- Prefer typed `NikiError` over bare `.unwrap()` on user-facing paths.

## Config & secrets

- `niki.toml` is git-ignored; keys also come from env vars (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GOOGLE_API_KEY`, providers via `<PROVIDER>_API_KEY` / `_BASE_URL` / `_MODEL`). Env vars override `niki.toml`. Never commit secrets; `.niki/` (run artifacts) is also git-ignored.
- Copy `niki.example.toml` → `niki.toml` for a full config reference.

See `CONTRIBUTING.md`, `README.md` (CLI reference, project structure), and `docs/content/` for deeper detail.
