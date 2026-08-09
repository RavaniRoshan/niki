# Changelog

All notable changes to NIKI are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/), and this project adheres to
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Prompts and JSON schemas are now **embedded in the binary**, so the released
  executable runs `niki run` from any directory without the source tree present.
- `load_asset` API for resolving bundled assets.

### Changed
- **License changed from BUSL-1.1 to Apache-2.0** — NIKI is now fully open
  source.
- Google provider now sends the API key via the `x-goog-api-key` header instead
  of a URL query parameter (avoids leaking the key into logs/proxies).
- The command security deny-list is now **enforced for every agent role**
  (previously only the default policy, letting the coder/reviewer roles run
  dangerous commands such as `curl | sh`, `mkfs`, `dd`, `rm -rf`).
- `display::artifact_render::truncate` is now Unicode-safe (no longer panics on
  multi-byte characters).
- SIGPIPE is ignored so piping output to `head`/`less` no longer crashes the CLI.

### Fixed
- `cargo test` (lib unit tests) now compiles — missing `Color` import in the
  chat display tests.
- Goal criteria no longer interpolate the objective string into a shell command
  (removed a command-injection vector).

### Security
- Secret redaction now covers Google API keys (`AIza…`) and `?key=` / `&key=`
  URL parameters in addition to existing patterns.

## [0.2.0] - 2025-08
- Initial public beta: Planner → Coder → Tester → Reviewer pipeline, Podman/Docker
  and git-worktree backends, BYOK multi-provider support, security auditor,
  parallel coders, TUI, and dashboard.
