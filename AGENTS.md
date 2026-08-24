# AGENTS.md — NIKI

Rust CLI (edition 2024, MSRV 1.85). Multi-agent coding pipeline: Planner → Coder → Tester → Reviewer run in hermetic sandboxes and hand back a `niki/<id>` git branch. Binary is `niki`. Surfaces: `niki run` (pipeline), `niki chat` (interactive TUI, the default when invoked bare), `niki acp` (Agent Client Protocol JSON-RPC server over stdio for IDEs like Zed), `niki voice` (push-to-talk STT).

## Build & verify

- Format + lint + test, in this order (mirrors CI in `.github/workflows/ci.yml`):
  - `cargo fmt --check`  (CI uses `--check`; locally `cargo fmt`)
  - `cargo clippy --all-targets`  (must be **warning-free** — CI enforces it)
  - `cargo test --verbose`
  - `cargo build --release` (CI then runs `./target/release/niki --version --help`)
- Single test: `cargo test <test_name>`.
- `git2` uses `vendored-libgit2`, so no system libgit2 is required to build.

## Critical quirk: prompts and schemas are baked into the binary

`src/lib.rs:131-132` compiles `prompts/` and `schemas/` into the binary at build time via `include_dir!`. Editing `prompts/*.md` or `schemas/*.json` has **no effect until you rebuild** (`cargo build`/`cargo run`). If your prompt/schema change "doesn't take", rebuild first. Runtime reads from embedded copies, not the source files.

## Tests & end-to-end

- Unit/integration tests live in `tests/` and `src/**` (`#[test]`). Dev-deps include `wiremock`, `assert_cmd`, `predicates`, `tempfile`.
- The real pipeline needs an LLM. For local e2e without keys or a container runtime, run the mock server and use the worktree backend:
  - `python3 tests/integration/mock_llm.py &` (serves on `:8080`)
  - `./target/release/niki run 'Add health endpoint' --backend worktree --quiet --project <git_repo>`
- `--backend worktree` (git worktree + local process) needs **no** container runtime. The default `docker` backend does, plus the pre-baked image: `podman build -t niki-sandbox:24.04 -f docker/Dockerfile .` (a plain `ubuntu:24.04` lacks git/node/npm/python3 and fails the sandbox's tool check).
- TUI has two black-box suites that drive the real binary offline (no model calls):
  - `./tests/tui_smoke/run.sh --build` — tmux-based smoke suite (`cases/*.sh` assert on rendered pane text; CI runs it via `.github/workflows/tui-smoke.yml`).
  - `pytest -c pytest_headless.ini` — headless PTY harness (`tests/headless_tui.py`, "tuiwright"). Needs Python + pytest.

## Supply chain (CI gate)

- `cargo deny check` and `cargo audit` run in CI. Adding a dependency whose license isn't in `deny.toml`'s tight allow-list fails CI — extend the list deliberately, don't copy a broad allow-list.
- Releases are produced by `cargo dist` (`dist-workspace.toml`); do not hand-roll release binaries.

## Architecture / extension points

- Entrypoint: `src/main.rs` → `src/cli/`. Core modules: `agents/`, `orchestrator/`, `sandbox/` (Podman/Docker/worktree backends), `llm/`, `runtime/` (tool registry + baseline tools), `artifacts/` (typed + JSON-schema validated), `config/`, `output/`.
- Interaction layer: `display/` is a large module — RenderEngine (`engine.rs`) + `pages/` (chat, diff, fleet, help, …) + `components/` (input_box, command_menu, permission, status_bar, …). Notable pieces: Codex-style side-by-side diff renderer (`diff_display.rs`), fuzzy slash-command menu (`components/command_menu.rs`, backed by `nucleo`), which-key help overlay (`help_overlay.rs`), kitty keyboard/graphics protocol (`kitty.rs`), voice capture (`voice.rs`). Custom slash commands are user-defined markdown files parsed by `commands/mod.rs`.
- IDE integration: `acp/` implements an Agent Client Protocol JSON-RPC 2.0 server over stdio (`protocol.rs` = types, `server.rs` = dispatch loop); `niki acp --project <dir>` drives the pipeline from an IDE.
- MCP: `mcp/mod.rs` holds the client **and** the up-front trust gate (`McpTrustStore`). Project MCP servers are untrusted by default; trust decisions (with config fingerprinting) persist to `.niki/mcp_trust.json`.
- Permissions: `permissions/mod.rs` owns permission modes and protected paths / destructive-command enforcement (always prompt, regardless of mode).
- Add a tool: implement the `Tool` trait in `src/runtime/mod.rs`, register in `build_baseline_registry()`, add tests.
- Add a provider: implement `LlmProvider` in `src/llm/provider.rs`; add a match arm in `create_provider()` in `src/llm/mod.rs`. `src/llm/anthropic.rs` is the reference impl. Providers can also expose `transcribe()` for STT (used by `niki voice`).
- Agent prompts are Minijinja templates in `prompts/*.md`; add an agent role → new prompt file + schema in `schemas/` + wire into `src/orchestrator/pipeline.rs`.
- Prefer typed `NikiError` over bare `.unwrap()` on user-facing paths.

## Config & secrets

- `niki.toml` is git-ignored; keys also come from env vars (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GOOGLE_API_KEY`, providers via `<PROVIDER>_API_KEY` / `_BASE_URL` / `_MODEL`). Env vars override `niki.toml`. Never commit secrets; `.niki/` (run artifacts, incl. `mcp_trust.json`) is also git-ignored.
- Copy `niki.example.toml` → `niki.toml` for a full config reference. Newer knobs: `general.language` (STT hint for `niki voice`), `knowledge.skills_dir` (shared skills dir, default `~/.agents/skills/`), `[ui] reduced_motion` / mouse-capture toggle.

See `CONTRIBUTING.md`, `README.md` (CLI reference, project structure), and `docs/content/` for deeper detail.
