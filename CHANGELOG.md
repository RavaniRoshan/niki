# Changelog

All notable changes to NIKI are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/), and this project adheres to
[Semantic Versioning](https://semver.org/).

## [0.5.0] - 2026-08-19

Claude Code UI parity + bug fixes + demo refresh.

### Added
- Claude Code–style permission modal (4 options, blue separator, dotted separator, Ctrl+E/Ctrl+D hints)
- Context-window gauge in status bar (`ctx ▓▓░░░░░░░░ 12%`) with color thresholds
- Queued-prompt indicator in status bar
- Full color detection hierarchy: `ColorDepth::detect()` (NO_COLOR → ANSI-16 → 256-color → truecolor)
- Paste burst detector: 80ms Enter-as-newline guard for bracketed paste
- Model-aware context limit registry (`update_context_limit_for_model`, 8K–1M)

### Changed
- Chat page now routes through `layout::render_chat` + dead-island `build_chat_lines`
- `render_input_box` takes `&AppState`; border/bg dims during active streaming
- Auto-scroll re-enables when scrolled to bottom (was permanent-off)
- Token accounting: `StageDone` accumulates `token_count`; `context_usage` = token_count / context_limit
- Removed dead `render_messages`, `msg_content`, `msg_role` from `layout/mod.rs`

### Fixed
- Auto-scroll stuck-off after user scrolls up
- Bracketed paste Enter-from-multiline submitting prematurely
- Context-window gauge missing from status bar
- Permission modal had only 3 options (now 4: Allow once/always, Deny, Deny always)

### Demo
- Rewrote `demo.tape` (900×560, 38s comprehensive chat flow)
- Rewrote `scripts/render_demo_terminal.py` (Claude Code–style capsule, spinner, gauge)
- GIF: 872K (`gifsicle -O3 --colors 32 --resize-width 640`)
- MP4: 957K (`ffmpeg -movflags +faststart -pix_fmt yuv420p -crf 23`)

## [0.4.0] - 2026-08-18

Launch cut.

### Added
- **Distribution narrowed to the three platforms we build and verify:** Linux (x86_64)
  and macOS (Intel + Apple Silicon) via Homebrew, a checksum-verified `curl` installer
  (`scripts/install.sh`), and GitHub release downloads.
- `docs/claims-audit.md`: every headline marketing claim traced to the code that backs it.

### Changed
- Homebrew formula now installs the release `.tar.gz` archives (with SHA256) for the three
  supported targets; Windows (Scoop/Winget) deferred — docs now say "planned".
- Honesty pass on sandbox claims: the deny-list blocks `git push --force`/`-f`, `rm -rf /`,
  and `curl|sh` / `wget|sh` pipes — **not** plain `git push` or non-root `rm`. Copy updated.

### Security
- Repo hardening: Dependabot, CodeQL, secret scanning + push protection, branch protection
  on `master`, `FUNDING.yml`.

## [0.3.3] - 2026-08-15

Release hygiene.

### Changed
- Release assets are now per-target `.tar.gz` archives plus a `checksums.txt` (SHA256) —
  the standard Rust-CLI distribution shape.
- CI caches `cargo-audit` / `cargo-deny` binaries instead of re-installing each run.

## [0.3.2] - 2026-08-15

CI / supply-chain hardening.

### Changed
- Removed the OpenSSL dependency: `reqwest` uses `rustls-tls`, `git2` vendors `libgit2`
  (no system OpenSSL needed for the macOS x86_64 cross-build).
- `deny.toml` license allow-list extended (ISC, CDLA-Permissive-2.0) so the supply-chain
  gate passes; CI workflows bumped to v7.

## [0.3.1] - 2026-08-14

Launch-hardening cut (the product IS the demo).

### Added
- **Verification in the loop is now real.** The Tester actually executes your test
  suite inside the sandbox (auto-detected `cargo test` / `npm test` / `pytest` /
  `go test` / `rspec`, or set `[agents.tester] test_command`) and records the
  real exit code + output as `artifacts/test_execution.json` and a
  `## Verification` section in `report.md` — before the branch exists.
- Sandbox image now includes a Rust toolchain (`cargo`/`rustc`) so Rust projects
  can be verified in-sandbox.

### Changed
- `[docker] network_disabled` is the default (egress blocked by default) — this
  was already the code default but is now documented as the differentiator it is.
  `network_allowlist = ["*"]` re-opens egress.
- `role_glyph` now renders a distinct glyph for the Planner so it no longer
  collides with the Reviewer.

### Security
- `SECURITY.md` updated: default-blocked egress is shipped, not a roadmap item.

## [0.3.0] - 2026-08-13

Launch cut. Focus: distribution, onboarding, and trust — the agent engine is unchanged.

### Added
- Multi-provider support: configure different LLM providers per agent role
- `general.spend_cap_usd` — a per-run spend ceiling. Exceeding it prints a clear
  warning so autonomous runs can't run away on cost.
- Explicit config-trust warnings: `[session]`, `[compaction]`, `[mcp]`, `[permissions]`
  are parsed but **not yet wired**; NIKI now says so at load instead of silently
  ignoring them.
- `assets/logo.svg` recolored to the teal brand (`#0d9488`) to match the TUI.
- `docs/benchmarks.md` (honest eval-harness notes).

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
