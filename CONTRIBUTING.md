# Contributing to NIKI

Thanks for your interest in improving NIKI! This document explains how to get set up and what we expect from contributions.

## Getting started

```bash
git clone https://github.com/RavaniRoshan/niki.git
cd niki
cargo build --release
```

NIKI requires **Rust 1.85+** (edition 2024) and a container runtime
(**Podman** recommended, or **Docker**) for the default sandbox backend.
The `--backend worktree` path needs neither.

## Development workflow

1. Fork and create a feature branch: `git checkout -b fix/my-change`.
2. Make your change. Keep `cargo fmt` clean and `cargo clippy` warning-free.
3. Run the tests: `cargo test`.
4. Open a pull request describing the *why* and the *what*.

## Code style

- Run `cargo fmt` before committing.
- Keep `cargo clippy --all-targets` free of warnings (CI enforces this).
- Prefer typed `NikiError` over bare `.unwrap()` on user-facing paths.
- Add tests for new behavior; do not lower coverage without reason.

## Extending Niki

### Adding a tool

1. Implement the `Tool` trait in `src/runtime/mod.rs`.
2. Register it in `build_baseline_registry()` with a `ToolDef` (id, description, category, risk level, permissions).
3. Add tests for the tool's `execute()` method.
4. Update the tool list in `docs/content/` if it changes the public API.

### Writing prompts

Agent prompts live in `prompts/*.md` as Minijinja templates. Each template receives a context object with task description, file contents, schema, and prior artifacts. The template renders into a system prompt sent to the LLM.

To modify an agent's behavior, edit its prompt file. To add a new agent role, create a new prompt file, a JSON schema in `schemas/`, and wire it in `src/orchestrator/pipeline.rs`.

### Customizing pipeline topology

Add a `[pipeline]` section to `niki.toml` to define your own ordered stages. Each stage binds an `AgentRole` to a provider/model. See `docs/content/06-configuration/02-general-and-pipeline.mdx` for the full reference.

### Adding a provider

Implement the `LlmProvider` trait in `src/llm/provider.rs`. Add a match arm in `create_provider()` in `src/llm/mod.rs`. See `src/llm/anthropic.rs` for a reference implementation.

## Reporting bugs

Use the bug-report issue template. Include: OS, NIKI version (`niki --version`),
the exact command, the sandbox backend used, and a redacted log. **Never paste
API keys or secrets** — NIKI redacts them, but be careful in issue text.

## Security

Found a vulnerability? **Do not open a public issue.** See
[SECURITY.md](SECURITY.md) for private disclosure.

## License

By contributing, you agree your contributions are licensed under the
[Apache License 2.0](../LICENSE), the same as the project.
