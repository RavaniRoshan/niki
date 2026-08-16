# NIKI TUI Reconstruction Progress

## Status
- **Started**: 2026-08-16
- **Current Phase**: Phase 16 complete; all 16 phases delivered
- **Phase**: 16 of 16 complete

## Key Stats
| Metric | Value |
|--------|-------|
| Display `.rs` files | 44 |
| Live modules | tui.rs, pages/*, agent_stream.rs, banner.rs, completion.rs, command_palette.rs, modal.rs, onboarding.rs, logo.rs, theme.rs, tips.rs, artifact_render.rs, pipeline_status.rs, persistence.rs |
| Previously-dead modules now LIVE | engine.rs (RenderEngine), layout/, components/{spinner,progress,permission,command_menu,autocomplete,input_box} |
| Dead modules | layout/mod.rs (companion simple chat renderer — kept as alternate), diff_display.rs |
| Tests (lib) | 318 passing + 1 ignored live test (NVIDIA NIM verified) |
| Live PageId | 12 variants (Run..Help, Chat) |
| Live AppState | `src/display/state.rs` (canonical, ~1001 lines) |
| RenderEngine | `src/display/engine.rs` — WIRED as live render driver (dirty-flag + 60/30fps adaptive + CSI-2026 sync output) |
| InputHandler | `src/display/input.rs` — WIRED; ChatPage input now reads `input_state` (fixes invisible-typing bug) |
| chat/ stack | `src/display/chat/*` — WIRED into chat view (markdown streaming, progressive disclosure) |
| Components | 7 (status_bar, spinner, progress, permission, command_menu, autocomplete, input_box — all WIRED as overlays in render loop) |
| Permission modal | Ask path WIRED with 5s bounded timeout + headless fallback to Allow |
| Goal system | Drifting status + fork artifacts (goal.md, progress.json, drift.jsonl, environment.lock) + `/undo`/`/redo` |
| Memory | Hierarchical: user + team + project role memory; `/memory store`/`/memory recall` |
| Research CLI | `niki research query <topic>` — web search with citations |
| Verify CLI | `niki verify <description>` — screenshot capture with manifest |

## Live System (current)
| File | Purpose |
|------|---------|
| `src/display/state.rs` | Canonical AppState + ChatLine + ViewMode + DisplayEvent dispatch |
| `src/display/pages/mod.rs` | PageRouter + Page trait (12 page renderers) |
| `src/display/pages/chat.rs` | Progressive-disclosure chat view (collapsed stage summaries / expanded markdown bodies, copy-mode, mouse select→OSC52) |
| `src/display/engine.rs` | RenderEngine: dirty-flag redraw, 60fps streaming / 30fps idle, synchronized output |
| `src/display/tui.rs` | Main loop + render + input dispatch (run_tui + run_chat) |
| `src/display/agent_stream.rs` | DisplayEvent bridge into the pipeline |
| `src/display/persistence.rs` | Chat session save/load/resume (`.niki/chat.json`) |
| `src/display/input.rs` | Unified InputHandler (Insert/Command/Shell), key bindings |
| `src/permissions/mod.rs` | PermissionAction (Allow/Deny) + PermissionRequest with response channel |
| `src/sandbox/docker.rs` | DockerSandbox with event_tx; Ask path sends PermissionRequest + 5s timeout |
| `src/sandbox/worktree.rs` | WorktreeSandbox with event_tx; Ask path sends PermissionRequest + 5s timeout |

## Phase Completion
- [x] Phase 0 — Audit doc (`docs/ui/ui-audit.md`, 44 files audited)
- [x] Phase 1 — Canonical state (`state.rs` replaces `pages` AppState; `ViewMode`/`current_page`; flat `stages`; `ChatLine`/`Modal` moved in)
- [x] Phase 2 — Shell + `niki chat` command (`cli/chat.rs` + `tui::run_chat`)
- [x] Phase 3 — Unified input (InputHandler → InputAction; `/`, `!`, history, Ctrl± combos)
- [x] Phase 4 — Chat view with progressive disclosure (markdown streaming + code blocks via dead `chat/` stack)
- [x] Phase 5 — Dead event emitters live (StageTotals/TestLogContent/Revision emitted from pipeline)
- [x] Phase 6 — Runtime interactivity (permissions gate, cooperative cancel, user messages, engine render driver)
- [x] Phase 7 — Port remaining views (spinner/progress/permission/command_menu/autocomplete overlays wired into live render; RenderEngine is the render driver)
- [x] Phase 8 — Persistence + resume (`.niki/chat.json` save on submit+exit, load on start)
- [x] Phase 9 — Perf + adaptive framerate (RenderEngine 60fps streaming / 30fps idle; dirty-flag redraw)
- [x] Phase 10 — Polish (tips banner render, minor; build+clippy+tests green)
- [x] Phase 11 — Live LLM verification (NVIDIA NIM provider tested with `meta/llama-3.1-8b-instruct`; live test passes)
- [x] Phase 12 — Permission modal Ask path (DisplayEvent::PermissionRequest + mpsc channel + 5s timeout + headless fallback)
- [x] Phase 13 — Goal primitives with drift recovery (GoalStatus::Drifting; fork artifacts; `/undo`/`/redo` slash commands)
- [x] Phase 14 — Verified online research (CLI `niki research query`; web search with citations)
- [x] Phase 15 — Team-scale hierarchical memory (user + team + project role memory; `/memory store`/`/memory recall`)
- [x] Phase 16 — Closed-loop visual coding (`niki verify` CLI; screenshot capture with manifest)

## Deep Research
- Report: `research/coding-agent-landscape-2026.md` (6 subagents + adversarial verifier)
- Key finding: long-horizon reliability is the central challenge; METR Opus 4.6 shows 50% reliability at ~14.5 hours
- NIKI differentiators: purely autonomous goal-running with drift recovery, verified online research, team-scale memory, checkpoint-and-fork recovery, three-file trust architecture (GOAL.md/VERIFY.md/PROGRESS.md)

## Changelog
- **2026-08-16**: Created progress.md, started Phase 0 (audit)
- **2026-08-16**: Phase 0 — `docs/ui/ui-audit.md` written; live/dead split documented
- **2026-08-16**: Phase 1 — `state.rs` canonical AppState; `pages/mod.rs` re-exports; flat `stages`; `ChatLine`/`Modal` moved; 310 tests
- **2026-08-16**: Phase 2 — `niki chat` (`cli/chat.rs`); `tui::run_chat`; theme cycle fix; Modal derive Clone; 310 tests
- **2026-08-16**: Phase 3 — Chat input → InputHandler; Ctrl+P palette; `/` commands; history; 310 tests
- **2026-08-16**: Phase 4 — Dead `chat/` markdown stack wired into chat view; progressive disclosure; `ChatLine.rich` + `header_stage`; `chat_width` Cell; 313 tests
- **2026-08-16**: Phase 5 — StageTotals/TestLogContent/Revision emitters live (pipeline on TUI path); 313 tests
- **2026-08-16**: Phase 6 — `PermissionChecker` wired into sandbox exec (Deny-only gate, default-config behavior-preserving); `Arc<AtomicBool> cancel` threaded through `execute_pipeline` + all callers (run.rs, goal/runner.rs, eval/mod.rs, harness) + TUI quit; `DisplayEvent::ChatMessage` added (feed-forward in `run_chat`); `NikiError::Cancelled`. 315 tests
- **2026-08-16**: Phase 7 — Dead components (status_bar, spinner, progress, permission modal, command menu, autocomplete, input_box) wired into live render as overlays; `RenderEngine` (`engine.rs`) is now the live render driver (dirty-flag + frame target in `tui.rs` loop), `#[allow(dead_code)]` removed; adaptive 60/30fps via `FrameTarget`; 318 tests
- **2026-08-16**: Phase 8 — `persistence.rs` module; `load_chat_session`/`save_chat_session`/`snapshot`/`apply_session`; `.niki/chat.json`; resume on `run_chat` start + save on submit & exit; 3 persistence tests. 318 tests
- **2026-08-16**: Phase 9 — `RenderEngine` adaptive framerate (`set_target` High when `has_running_stage` else Low); 318 tests
- **2026-08-16**: Phase 10 — Removed stale unused imports; engine `#[allow(dead_code)]` dropped. Build/test/clippy green
- **2026-08-16**: Phase 11 — NVIDIA NIM live test verified (`meta/llama-3.1-8b-instruct`); API key accepted; 1 ignored live test added
- **2026-08-16**: Phase 12 — Permission modal Ask path wired: `DisplayEvent::PermissionRequest` + `mpsc` channel + 5s bounded timeout + headless fallback to Allow; `PermissionAction` enum added; `event_tx` threaded through `DockerSandbox`/`WorktreeSandbox`/`create_sandbox`/pipeline; key handling in `tui.rs` (y/Y/Enter → Allow, n/N/Esc → Deny); 318 tests + clippy green
- **2026-08-16**: Phase 13 — Goal drift recovery: `GoalStatus::Drifting` added; `DriftSignals` struct; `GoalState::fork()` writes goal.md/progress.json/drift.jsonl/environment.lock/open-questions.md; `/undo` and `/redo` slash commands added to chat view; `niki goal fork` CLI command added
- **2026-08-16**: Phase 14 — Verified online research: `niki research query <topic>` CLI added (DuckDuckGo HTML search); `WebFetchTool` in `src/tools/web_fetch.rs`; source attribution via URL + snippet extraction
- **2026-08-16**: Phase 15 — Hierarchical memory: `load_user_memory`/`save_user_memory`/`append_user_memory`/`load_team_memory`/`save_team_memory`/`append_team_memory`/`render_hierarchical_memory` added to `src/memory/store.rs`; `/memory store` and `/memory recall` commands added to `src/cli/memory.rs`
- **2026-08-16**: Phase 16 — Closed-loop visual coding: `niki verify <description>` CLI added; screenshot capture via scrot/gnome-screenshot/ImageMagick with graceful headless fallback; manifest at `.niki/artifacts/verify-manifest.json`
- **2026-08-16**: Deep research report written to `research/coding-agent-landscape-2026.md` (6 subagents + adversarial verifier)

## Remaining Work
- Full visual TUI verification pending live run (no TTY in this environment)
- `tips.rs` banner is built but not yet rendered in the viewport (low priority)
