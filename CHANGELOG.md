# Changelog

All notable changes to NIKI are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/), and this project adheres to
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.8.0] - 2026-09-23

Agent-harness + runtime release (36 commits since 0.7.0): repo
intelligence, risk-gated Critic pipeline, provenance/KB/history/structural
index, agent runtime rework with sessions/checkpoints/resume, converged
store + project skills, unified run budget, TUI performance/search/input
hardening, five ready gateways, reliability/honesty fixes, and CI/visual
gate repairs. Claims below are backed by tests, mock-LLM e2e, or the VHS
visual gate unless noted as docs/launch material.

### Added
- Repository intelligence (`[repo_intel]`, `niki inspect [--json]`):
  deterministic `RepoManifest` (languages, entry points, tests, build
  files, vendor exclusion, risk cues). Fail-soft; indexing can no longer
  abort a run unless `on_failure = "fail"`.
- Run provenance (`[snapshot]`, `manifest.json` per task,
  `niki status [id] --with-provenance`): repo HEAD/branch/remote, config
  content hash, toolchain versions, result branch + costs.
- Project KB (`niki architecture build`, `.niki/kb/`): snapshot-stamped
  Markdown + provenance-wrapped JSON sidecars, rebuilt from scratch.
- History miner (`.niki/history/`, cache-as-truth with rewrite detection):
  keyword-classified commit learnings rebuilt from cache every run.
- Structural index (`niki index build|query`, `.niki/kb/structural_index/`):
  content-addressed per-file units, AST→regex→coverage backend ladder
  (tree-sitter behind the default-on `ast` Cargo feature; `--no-default-features`
  keeps the regex baseline). Advisory only — grep stays authoritative.
- Bounded Planner context (`[general] max_context_chars`, default 48000):
  manifest + KB + symbol excerpts + learnings, priority-ordered with an
  explicit truncation marker.
- Risk-based pipeline (`[risk]`, `[critic]`): deterministic TaskSpec
  classifier (low/normal/high/security) injects the Critic after the
  Reviewer on Normal+ and forces a SecurityAuditor on High/Security.
  Explicit `[pipeline].stages` topologies are never rewritten.
- Critic stage: narrow verdict-grounding checker (`prompts/critic.md`,
  `schemas/critique.schema.json`); a Reject forces exactly one Reviewer
  retry, then a closing judgment. Recorded, never a gate of its own.
- Post-run reflection (`src/orchestrator/reflect.rs`): `verification_failure`,
  `review_correction`, and `security_fix` learnings into
  `.niki/learnings.jsonl`, flowing back to the Planner via the KB.
- Reviewer test-evidence gate: failures/skips (or zero executed tests) on
  business-logic tests must yield `revision_needed`, never `approved`.
- Agent runtime rework (`src/runtime/`): `AgentSession` / `AgentTurn` /
  `AgentStep` execution loop, bounded priority-sorted `ContextStore` with
  compaction, typed `AgentEvent` stream, `ToolPolicy` + tool registry
  split (`tools.rs`), cancellation tokens, and session checkpoints under
  `.niki/sessions/`.
- `niki resume <session-id>`: resume an interrupted agent session from a
  checkpoint (role/turn/step, artifacts, context fragments, active branch).
- `niki skills` (`list|candidates|promote|retire|show|diff`): two-step
  distillation — Approved green run stages a candidate, human promotes to
  a versioned skill (`SKILL.md` + `metadata.json` + `skills-lock.json`).
  Nothing auto-activates; stale source snapshots are flagged, never served
  as fresh. Served via `skill_list` / `skill_load` alongside
  `~/.agents/skills/`.
- Converged store (`src/store/`, ADR-002): rebuildable hybrid index over
  learnings + role/user memory + run records — keyword (0.45) + trigram
  vector cosine (0.35) + recency (0.10) + authority (0.10). File-backed,
  zero new deps; index size honors `[repo_intel] disk_budget_mb`; deleting
  `<output_dir>/store/` is always safe (live-scan fallback).
- Unified run hysteresis budget (`[budget]` / `RunBudget`): one
  step/cost/wallclock ceiling across retries, repairs, revisions, tool-loop
  steps, and goal iterations. Exhaustion → typed `BudgetExhausted` recorded
  in `task.json`. CLI overrides: `niki run --max-steps --max-usd
  --max-wallclock-secs`. `max_usd` falls back to `spend_cap_usd` when unset.
- Optional executable tool loop (`[tools] experimental_tool_loop`,
  default off): one bounded research step through `run_tool_loop` before
  the Planner; inherits `[permissions] mode` (Ask tools fail closed
  headless).
- MCP end-to-end path: `from_config` loads `[[mcp.servers]]` + governance;
  live connections retained for `call_tool`; `annotations.readOnlyHint` →
  `read_only` (unmarked tools denied under default read-only governance);
  domain allowlist applies to web-fetch-shaped tools.
- Failover structured-output routing: `FailoverProvider` now propagates
  `supports_structured_output` and routes `request_structured` through the
  chain (structured output no longer silently degrades on failover).
- Hook timeouts: `[hooks] timeout_seconds` (default 30); overlong hooks
  are killed and treated as Noop with a warning (0 = wait forever).
- TUI (goal c81d04): central keybinding table with `[ui.keybindings]`
  overrides + conflict report; transcript search (Ctrl+F); fuzzy `@files`
  ranking with Tab apply; shared `ScrollState` (fixes stuck auto-scroll,
  wires tool-detail modal); `NIKI_TUI_DEBUG` per-frame log; headless
  `tests/tui_perf.rs` render budgets; stage-markdown + processed-diff
  memos; fleet refresh throttle (500ms); mouse motion/SGR always-on with
  Ctrl+E toggle; width-aware markdown tables; OSC-8 hyperlinks gated by
  terminal caps / `NIKI_HYPERLINKS`. Optional `[ui]`, `[ui.tips]`,
  `[ui.transcript]` tables.
- Providers / onboarding: five ready gateways — Ollama first-class
  keyless (wizard option 0, live `/api/tags` probe, auth/doctor
  reachability) plus Zen / Kimi / Kilo (OpenAI-compatible, named
  constructors, `OPENCODE`/`KIMI`/`KILO_API_KEY` env wiring); single-pick
  init wizard that rewrites all four `[agents.*]` provider lines and
  preselects an installed Ollama model; headless `chat --message`
  plain-text reply (was silent TUI teardown).
- `niki smoke --backend`: local smoke path selectable without a container
  runtime (Path A quick-start: Ollama + worktree, no key/container).
- Cinematic README demo: deterministic frame-rendered 80s TUI walkthrough
  (`scripts/render_demo_cinematic.py`); theme `sand()` fixed to warm
  SAND_500 (was cyan), `theme::cyan()` for INFO_BLUE.
- Launch material: PH kit (`docs/launch/` checklist, maker first-comment,
  gallery), social assets (`assets/social/ph-thumbnail.png`,
  `ph-gallery-run.gif`), launch playbook (`plans/noctty-launch-playbook.md`)
  with trust-boundary-aligned copy; TUI extraction plan status
  (`docs/tui/pi-extraction-plan.md`).
- Eval grades: four seeded defect cases graded (100% maintainer agreement).
- CI gates: MSRV (1.85) + `--no-default-features` jobs; clippy `-D
  warnings`; artifact-contract + run-lifecycle tests required before the
  full suite; `STATE_LAYOUT.md` file contract; agent-harness plan docs
  (`plans/niki-agent-harness-plan.md`, ADRs 001/002).

### Fixed
- Hooks: payload write ignores EPIPE so a fast hook that exits without
  reading stdin can no longer mask Block as Noop/Allow (exit-code
  interpretation always runs); 100× stress regression test.
- `[general] max_diff_lines` from `niki.toml` is now honored (it was parsed
  but never merged into the active config).
- TUI: multi-byte cursor panics/caret corruption (char-boundary clamp +
  byte↔char click mapping); 1-column click offset; fleet refresh no longer
  `block_on`s Tokio locks every frame; long-transcript auto-scroll stuck at
  top (write-only `auto_scroll` / bottom-anchored selection math); tool
  detail modal state existed but never painted; permission-modal option
  rows off-by-2+ and clipped (content-driven height); command-menu
  hit-test drift vs filtered count; byte-slice panics on tool cards, stage
  error headers, and chat input echo; long commands truncated to modal
  width.
- Empty-diff runs no longer leave HEAD on an empty `niki/<id>` branch;
  worktree teardown no longer leaks `git fatal()` noise to stderr.
- Version/logo strings use `CARGO_PKG_VERSION` (showed stale `v0.4.0`).
- `providers check`: all OpenAI-compatible slugs use named constructors
  (groq/together/deepepseek previously reported `OPENAI_API_KEY`); Ollama
  health check resolves an installed model (was permanent 400 + empty model).
- Init wizard rewrites all four `[agents.*]` provider lines to the picked
  provider (template Anthropic defaults no longer survive and break fresh
  machines); Ollama pick preselects an installed coding model (was
  hardcoded `qwen2.5-coder`, which 404s when only tagged variants exist).
- Solo coder gets one bounded repair attempt on patch-apply failure
  (exact error + verbatim-SEARCH rules; same spend-cap/hooks/metrics
  accounting as the first attempt); `code_diff` search/replace schema bans
  regex/anchors/paraphrase (propagates to all coder prompts).
- Artifacts writer keeps every attempt (`coder.json`, `coder-2.json`, …)
  instead of overwriting — failed attempts stay inspectable.
- Failover no longer reports `supports_structured_output = false` (trait
  default); structured output routed through the chain with circuit
  breakers.
- VHS/visual CI: onboarding tapes force the modal via
  `NIKI_FORCE_ONBOARDING` (CI auto-suppress broke references); ttyd +
  ffmpeg installed for tape rendering; render engine tests headless
  (`TestBackend` — no TTY on runners).
- Release/CI plumbing: package manifests (Homebrew/Scoop/Winget) pinned to
  v0.7.0 with real sha256 (v0.4.0 Windows zip never existed / 404);
  artifact actions aligned to v7 (v8 tag does not exist); `cargo dist`
  `allow-dirty` for hand-maintained artifact pins; `@niki` review workflow
  `needs-keys` gate moved off job-level `if:` (workflow failed to load);
  `audit` job restored after being swallowed into a comment; VHS/pillow
  install order fixed for runners.
- Mock-pipeline e2e (kb_pipeline / critic path) drops the `nodejs` binary
  alias from the sandbox tool check (CI has `node` only).

### Changed
- README / trust copy: sequential stages intentionally share one execution
  sandbox so the diff persists Coder → Tester → Reviewer; independence is
  at the LLM-session layer. Committed branches are never repointed or
  rewritten; the host working tree receives the finished diff for review.
  `readonly_rootfs` documented as optional and off by default; sandbox
  claim narrowed to CapDrop ALL + network disabled + optional read-only
  rootfs.
- `extra_packages` clarified: despite the name, nothing is installed —
  entries extend the startup `command -v` checklist against the pre-baked
  image.
- `network_allowlist` honesty: per-domain filtering is NOT implemented —
  container egress is all-or-nothing; only `"*"` opens egress; a non-empty
  domain list warns at startup and behaves as block-all.
- Init wizard rewritten as a single-pick menu (was 11 sequential prompts).
- Config examples: `[ui]` tips/transcript nested tables; `[compaction]`
  default threshold 80% + `auto_compact`; pipeline topology values
  lowercased (`auto`/`multiagent`/`singleagent`).
- Launch copy aligned with trust boundaries (`docs/launch-audit.md`,
  first-comment).

### Security
- Deny-list always wins over overlapping allow entries (coder `rm` still
  cannot `rm -rf /`; `git diff` allow no longer bypasses a `git` deny).
- Diffs scoped to agent-produced changes: pre-existing dirty/untracked
  host files stay out of `changes.patch` and the commit; new agent files
  appear via scoped intent-to-add (both backends); edit-format application
  is all-or-nothing per stage.
- Same-task worktree collision fails loudly instead of deleting a
  concurrent run's directory; Drop/panic paths still tear down worktrees
  and containers (best-effort); stale prune never removes a live worktree.
- Failed runs create no `niki/*` branch and leave `task.json` Failed with
  the error; conflict markers abort branch creation instead of committing.
- MCP: unmarked tools denied under default read-only governance (deny-by-
  default, never assumed safe); web-fetch tools gated by domain allowlist;
  untrusted servers error instead of connecting implicitly.
- Hooks timeout kills overlong processes so a hung hook cannot stall or
  mask a Block decision forever.

## [0.7.0] - 2026-09-08

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
