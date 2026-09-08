# Changelog

All notable changes to NIKI are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/), and this project adheres to
[Semantic Versioning](https://semver.org/).

## [Unreleased] — 0.7.0: vision-complete pipeline

Mega-plan execution (30 commits): plan-mode approval gates, oracle
integrity, honest cost metering, independence hardening, session control
plane, headless CI contract, trust posture, automation, and TUI unification.
All user-facing claims below are covered by tests, mock-LLM end-to-end runs,
or the VHS visual gate (`tests/visual/`, 12 reference frames at 0.00%
self-diff).

### Added
- Plan mode: `niki plan` researches without executing and writes reviewable
  `plan.md`; `niki run --plan <id>` executes the approved spec, skipping the
  Planner LLM call. Dry runs no longer create empty branch refs.
- Session CLI: `niki session list/show/checkpoints/undo/rewind` with
  code+conversation/both restore modes and a dirty-tree guard.
- User slash commands: `.niki/commands/*.md` (filename → `/name`) with
  `description:`/`aliases:` frontmatter, `[commands] extra_dirs` sharing,
  and `niki commands list/show/expand`.
- Headless contract: `niki run --bare` (no memory/MCP/knowledge-URLs),
  `--output-format json` stable envelope (incl. error path), stdout
  pipe-purity (display mute + captured git stdio), OTLP trace export
  (`--otel-endpoint`, no new dependencies).
- Oracle integrity: `oracle_source` (spec/derived/property) on every test
  case, reviewer oracle-check rule, no-healing rule, opt-in
  `[agents.tester] mutation_command` gate; red-suite/mutation failures block
  the branch unless `--force` (recorded as NOT verified).
- Independence hardening: Red receives evidence-only diffs, isolation records
  match wiring, topology selection reason recorded and reported, shared-model
  review warnings.
- Honest meter: cached-input/reasoning token splits, unpriced-model warnings
  (`unpriced*` in reports), price-table freshness test, failover
  cache-bust warnings, history-driven `niki recommend --project`.
- Trust posture: `[permissions] mode` + `--permission-mode`, `disable_worktree`
  kill-switch, `fail_closed_headless` flag, loud headless Ask fallback.
- Lifecycle hooks: `[hooks.commands]` wired to PreTaskStart/PreAgentStart/
  PostAgentStop/PostTaskStop, fail-closed block semantics.
- GitHub automation: keyless nightly eval gate, human-gated `@niki` review
  workflow (injection-safe, loop-guarded).
- Eval credibility: per-case costs, disclosure manifest on every run,
  `niki eval grade` maintainer judgments with agreement metric, ablation
  protocol doc.
- Observability: `trace.jsonl` spans per run (honestly derived timeline),
  `niki audit` compliance bundles.
- MCP Streamable-HTTP remote transport (JSON + SSE, session affinity).
- Onboarding: `niki init` alias, `init --scan` AGENTS.md drafter,
  `.niki/rules/` binding conventions, per-provider model aliases,
  per-agent `effort` presets, actionable key errors, guided empty states.
- TUI unification: one status grammar, 100% theme-token production code,
  normalized headers, global page jumps in both event loops, palette
  fleet/session rows, empty-state triads.
- TUI motion system: primitives + caret blink, Done slide-in, notice
  slide-in, progress shimmer, modal pulse, verdict pulse — all reduced-motion
  gated (frames pixel-identical with the flag on).

### Fixed
- `GEMINI_API_KEY` vs `GOOGLE_API_KEY` chat fallback miss.
- `smoke`/`doctor` pointed at nonexistent `niki init` (now a real alias).
- Unwired `schemas/review_feedback.schema.json` removed.
- `backend = "podman"` doc strings corrected (docker|worktree only).
- `safety_proof.json` scope documented as git-only (was overclaimed).
- Approval tool auto-approved everything; ask tool invented answers.
- `[permissions]` table silently dropped by config merge.
- MCP `JsonRpcResponse` never parsed (`jsonrpc` field rename bug).
- Dry-run empty branch refs; planner-skip metrics crash.
- Global key `h`/`s`/`l` dead-ends; blank session screen; `$-0.00`.

### Changed
- `safety_proof.json`, spend-cap, and deny-list copy narrowed to match code.
- `niki recommend` static-pairings truth in docs (history-driven per-project
  observed spend added to the command itself).

## [0.6.0] - 2026-08-21

Claude Code parity — interaction, trust, and ecosystem surface.

### Added
- Visible, draggable scrollbar in the chat viewport (ratatui `Scrollbar`, thumb + track, click/drag-to-jump)
- Mouse hover system: `HoverTarget` hit-tests across stage headers, messages, status bar, tab bar, modals, help, fleet; click flash + double-click word select in the input box
- Scroll-wheel routing on every page (chat, pages, permission/palette/menu overlays) with auto-follow pause
- Kill ring + yank: `Ctrl+Y` yanks the most recent kill, `Alt+Y` cycles (yank-pop); `Ctrl+W/U/K` now delete into the ring
- Input undo/redo: `Ctrl+Z` / `Ctrl+_` undo, `Ctrl+Y` yank; every edit is snapshot-able
- Multi-line composer: input area grows to ~1/3 of the screen for multi-line prompts (`render_input_box_multiline` now live)
- Protected paths & destructive-command enforcement: `.git`, `.ssh`, `/etc`, `rm -rf`, `sudo`, `curl`, `git push`, etc. always prompt regardless of mode
- Shared skills portability: reads `~/.agents/skills/` (override via `knowledge.skills_dir`), zero-migration
- Kitty keyboard protocol (progressive adoption, I4): enable/disable around the session + CSI-u decoding for Shift+Enter disambiguation
- Expanded slash-command set: `/status`, `/permissions`, `/plan`, `/version`, `/rename`, `/fork`, `/branch`, `/usage`, `/effort`, `/mcp`, `/skills`, `/copy`, `/export-md`, `/btw`, `/code-review`, `/security-review`, `/add-dir`, `/loop`, `/voice`

### Changed
- Chat render now virtualizes to the visible window and renders a live scrollbar
- Permission modal detail panel (Ctrl+D) and scope selector (Tab) wired to the live permission flow

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
