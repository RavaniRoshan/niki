# Changelog

All notable changes to NIKI are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/), and this project adheres to
[Semantic Versioning](https://semver.org/).

## [0.3.0] - 2026-08-18

Launch cut. Focus: distribution, onboarding, and trust — the agent engine is unchanged.

### Added
- `niki init` wizard: detects the container runtime, pulls the published sandbox
  image, validates the API key, and runs a smoke task (target: first branch in < 5 min).
- Prebuilt sandbox image published to `ghcr.io/ravaniRoshan/niki-sandbox` — no manual
  `podman build` required.
- `general.spend_cap_usd` — a per-run spend ceiling. Exceeding it prints a clear
  warning so autonomous runs can't run away on cost.
- Explicit config-trust warnings: `[session]`, `[compaction]`, `[mcp]`, `[permissions]`
  are parsed but **not yet wired**; NIKI now says so at load instead of silently
  ignoring them.
- `assets/logo.svg` recolored to the teal brand (`#0d9488`) to match the TUI.
- `docs/benchmarks.md` (honest eval-harness notes) and a static landing page.

### Changed
- Package manifests (Homebrew, Scoop, Winget) now target `v0.3.0`; Winget license
  corrected `BUSL-1.1` → `Apache-2.0`.
- `[mcp]` now defaults to `enabled = false` (the MCP client is not yet wired; the
  previous default implied protection that did not exist — see security audit S14).
- Worktree backend now prints an explicit "runs on your host with your privileges"
  warning, since it has no VM isolation (security audit S6).

### From 0.3.0-pre
- Prompts and JSON schemas are **embedded in the binary** (runs from any directory).
- License changed from BUSL-1.1 to Apache-2.0.
- Google key sent via `x-goog-api-key` header (not URL param).
- Command deny-list enforced for every agent role.
- `display::artifact_render::truncate` Unicode-safe; SIGPIPE ignored.

### Fixed
- `cargo test` (lib unit tests) compiles; goal criteria no longer interpolate the
  objective into a shell command.

### Security
- Secret redaction covers Google API keys and `?key=` / `&key=` URL parameters.

## [0.2.0] - 2025-08
- Initial public beta: Planner → Coder → Tester → Reviewer pipeline, Podman/Docker
  and git-worktree backends, BYOK multi-provider support, security auditor,
  parallel coders, TUI, and dashboard.
