# NIKI TUI — Complete Implementation Progress

> **Started:** 2026-08-16
> **Plan:** `research/claude-opencode-tui-unified-plan.md` (14 workstreams W1–W14)
> **Goal:** `.opencode/goals/niki-unified-tui-tool-runtime-a1b2c3.json` (17 tasks)
> **Branch:** `goal/niki-unified-tui-tool-runtime-a1b2c3`
> **Last updated:** 2026-08-17

## Current Status

| Metric | Value |
|---|---|
| Cargo build | ✅ Passes (24 warnings, 0 errors) |
| Cargo test | ✅ 616 pass (370 lib + 246 integration) |
| Previous phases (T1–T15) | ✅ All complete |
| Current goal (17 tasks) | ✅ 17/17 done |
| Remaining | none |

---

## Phase 1 — T1–T6: UX Breakages (Previous session)

All complete. Implemented 2026-08-16/17.

| Task | Description | Status |
|---|---|---|
| **T1** | Fix resize repaint (Event::Resize arm in run_tui) | ✅ done |
| **T2** | Wire slash command menu (show_command_menu nav, command_filter, Enter dispatch) | ✅ done |
| **T3** | Wire `@` file autocomplete (sync_input_overlays, build_candidates) | ✅ done |
| **T4** | Global Esc semantics + indicator (InputAction::Cancel → state.request_cancel) | ✅ done |
| **T5** | Ctrl+C two-press exit + indicator (last_ctrl_c tracking) | ✅ done |
| **T6** | Route mouse/trackpad to active overlay (Event::Mouse routing) | ✅ done |

---

## Phase 2 — T7–T9: Reliability UX (Previous session)

All complete. Implemented 2026-08-17.

| Task | Description | Status |
|---|---|---|
| **T7** | Context transparency bar + auto-compact wiring (ContextBudget 0.6/0.8, context.json) | ✅ done |
| **T8** | Crash-safe incremental pipeline persistence (TaskRecord saved per stage) | ✅ done |
| **T9** | OS-level notifications (notify-rust, PermissionRequest + pipeline completion) | ✅ done |

---

## Phase 3 — T10–T12: Advanced (Previous session)

All complete. Implemented 2026-08-17.

| Task | Description | Status |
|---|---|---|
| **T10** | Surface checkpoints/rewind in TUI (SessionManager undo/redo/rewind, /rewind command) | ✅ done |
| **T11** | Wire [permissions] config into PermissionChecker (allow/ask/deny per-tool model) | ✅ done |
| **T12** | Mid-turn steering (steer_channel, DisplayEvent::SteerChannel, /steer command) | ✅ done |

---

## Phase 4 — T13–T15: Universal Cursor + Typing Indicator (Previous session)

All complete. Implemented 2026-08-17.

| Task | Description | Status |
|---|---|---|
| **T13** | Universal list-cursor abstraction (ListCursor + FocusState in list_cursor.rs) | ✅ done |
| **T14** | Mouse hover-highlight + click-to-select (hover/click in permission/palette/menu) | ✅ done |
| **T15** | Typing/line/mode indicator (status_bar.rs: MODE badge, Ln/Col, Typing…) | ✅ done |

---

## Phase 5 — Unified TUI + Tool Runtime Goal (This session)

17 tasks. 12 done, 5 undone. Branch: `goal/niki-unified-tui-tool-runtime-a1b2c3`

### Reference Research

| Task | Description | Status |
|---|---|---|
| **T1** | Clone 5 reference repos into research/refs/ | ✅ done |

Reference repos: claude-code, claudecode (Rust+ratatui), kilocode, opencode, niki-site

### Event Stream + Stores

| Task | Description | Status |
|---|---|---|
| **T2** | Event stream: typed Event enum + EventBus (broadcast channel, 1024 cap) | ✅ done |
| **T3** | Mission/Session/Agent stores + Mission entity + concurrent tokio execution | ✅ done |
| **T7** | Activity grammar: AgentState enum (12 states) + transitions + attention priority | ✅ done |

**Files:** `src/event/mod.rs`, `src/mission/mod.rs`, `src/activity/mod.rs`
**Tests:** 13 unit tests (3 event + 3 mission + 7 activity)

### Tool Runtime + Tools

| Task | Description | Status |
|---|---|---|
| **T4** | Tool Runtime: ToolRegistry, ToolResult envelope, permission→tool mapping | ✅ done |
| **T5** | 22 baseline tools: read/write/edit/patch/list/glob/grep/bash/test/web_search/web_fetch/task_spawn/task_status/task_cancel/task_create/task_update/task_list/ask_user/approval/skill_list/skill_load/git | ✅ done |
| **T8** | Agent capability separation: per-role tool allowlists (ToolDef.agent_access) | ✅ done |

**Files:** `src/runtime/mod.rs` (22 tool implementations)
**Tests:** 6 unit tests (ToolRegistry, ToolInput, ToolResult, ToolCategory, ToolId)

### Visual Pages

| Task | Description | Status |
|---|---|---|
| **T11** | Chat layout (V1): header, participants, composer, status bar | ✅ done |
| **T12** | Composer polish (V2): multiline, / popup, @ popup, reverse search | ✅ done |
| **T13** | Fleet grid (V3): mission cards, focus, keyboard nav | ✅ done |
| **T14** | Session view (V4): panels, tabs, Esc→Fleet | ✅ done |
| **T15** | Settings + Onboarding + Modals (V5–V7) | ✅ done |

**Files:** `src/display/pages/fleet.rs`, `src/display/pages/session.rs`
**Tests:** 2 unit tests (fleet navigation, session tab cycle)

### Tests

| Task | Description | Status |
|---|---|---|
| **T17** | Full test sweep: cargo test + new unit tests | ✅ done |

**Result:** 455 tests pass (358 lib + 97 integration), 0 failures

### Undone Tasks — NOW COMPLETE

| Task | Description | Status | Impact |
| --- | --- | --- | --- |
| **T6** | LLM tool-calling loop: `tools` param on provider payloads + tool-result→context | ✅ done | `run_tool_loop` in `src/runtime/mod.rs` drives tool calls through the LLM |
| **T9** | Composer internals: Ctrl+R, Shift+Enter, word nav, queued prompts | ✅ done | Keybindings added to `src/display/input.rs` |
| **T10** | Command system: grouped slash commands, aliases, args, palette categories | ✅ done | `CommandCategory`/`group`/`aliases` in `src/commands/mod.rs` |
| **T16** | Persistence: mission-scoped storage, restore on relaunch | ✅ done | `src/persistence/mod.rs` saves/loads `.niki/missions/<id>.json` |

---

## Research Workstreams W1–W14 (Full spec)

From `research/claude-opencode-tui-unified-plan.md`

| WS | Description | Status |
|---|---|---|
| **W1** | Live `/` autocomplete popup (fuzzy filter + frecency) | ✅ done (T2, Phase 1) |
| **W2** | Unified command registry (slash + palette, one definition) | ✅ done (T2, Phase 1) |
| **W3** | Resize responsiveness (Event::Resize arm + idle tick + diff-render) | ✅ done (T1, Phase 1) |
| **W4** | Keyboard model + indicators (Esc/Ctrl+C + Kitty protocol) | ✅ done (T4/T5, Phase 1) |
| **W5** | Onboarding & empty state (status bar + welcome + ? shortcuts) | ✅ done (T15, Phase 4) |
| **W6** | Architecture hardening (double-buffered diff-render + keybind config) | ❌ undone |
| **W7** | Session persistence + crash-safe resume (append/incremental writes) | ✅ done (T16: `src/persistence/mod.rs` mission-scoped JSON store + relaunch restore) |
| **W8** | Checkpoints / rewind / fork (per-edit snapshots + /rewind + /fork) | ⚠️ partial (T10: undo/redo/rewind exist; no auto snapshots, no /fork) |
| **W9** | Context transparency + compaction + memory (in-TUI context bar) | ⚠️ partial (T7: ContextBudget wired; no live TUI visualization) |
| **W10** | Subagent visibility (live per-role progress, token burn-rate) | ⚠️ partial (AgentsPage exists with tabs; no live streaming) |
| **W11** | Permission modes + sandbox + approval (allow/ask/deny + diff preview) | ⚠️ partial (T11: [permissions] wired; no diff preview in approval) |
| **W12** | Plan mode + todos + hooks (read-only mode + TodoWrite + PreToolUse) | ⚠️ partial (planner.md → TaskSpec; no hooks, no live todo tracker) |
| **W13** | Diff review + cost + notifications (inline Accept/Reject + OS notif) | ⚠️ partial (T9: OS notif wired; DiffPage exists but no inline Accept/Reject) |
| **W14** | Reliability internals + mid-turn steering (retry UI + steering) | ⚠️ partial (T12: /steer wired; retry UI not exposed in TUI) |

---

## New Architecture Modules (This session)

| Module | File | What it provides |
|---|---|---|
| Event Bus | `src/event/mod.rs` | `EventBus` (broadcast channel), 30+ typed events (MissionStarted, ToolCompleted, ApprovalRequired, etc.) |
| Mission Stores | `src/mission/mod.rs` | `Mission`, `Session`, `Agent` entities + `MissionStore`, `SessionStore`, `AgentStore` + `Stores` composite |
| Activity Grammar | `src/activity/mod.rs` | `AgentState` enum — 12 states (Idle→Thinking→Searching→Writing→Complete/Error) with icons, labels, transitions |
| Tool Runtime | `src/runtime/mod.rs` | `ToolRegistry`, `ToolResult` envelope (status/summary/data/artifacts/diagnostics), `Tool` trait, `ToolDef`, 22 tool implementations |
| Fleet Page | `src/display/pages/fleet.rs` | Fleet dashboard — 2-column mission cards, keyboard nav, cost/elapsed/attention |
| Session Page | `src/display/pages/session.rs` | Session view — 7 tabs (Conversation/Agents/Tools/Diff/Tests/Approvals/Evidence) |
| Persistence | `src/persistence/mod.rs` | `MissionSnapshot` serde + `.niki/missions/<id>.json` save/load/list/delete/restore (path-traversal sanitized) |

---

## Test Summary

| Module | Tests | Status |
|---|---|---|
| event::tests | 3 | ✅ |
| mission::tests | 3 | ✅ |
| activity::tests | 7 | ✅ |
| runtime::tests | 6 | ✅ |
| display::pages::fleet::tests | 1 | ✅ |
| display::pages::session::tests | 1 | ✅ |
| session::tests (existing) | 7 | ✅ |
| All other lib tests | 330 | ✅ |
| Integration tests | 97 | ✅ |
| **Total** | **455** | ✅ |

---

## Changelog

- **2026-08-16:** Started implementation. Phase 1 (T1–T6) implemented. Resize fix, slash menu, @ autocomplete, Esc/Ctrl+C indicators, mouse routing.
- **2026-08-17 (morning):** Phase 2 (T7–T9) implemented. Context transparency, crash-safe persistence, OS notifications. Phase 3 (T10–T12) implemented. Checkpoints/rewind, permissions wiring, mid-turn steering.
- **2026-08-17 (afternoon):** Phase 4 (T13–T15) implemented. Universal ListCursor, mouse hover/click, typing/mode indicator. 455 tests pass.
- **2026-08-17 (evening):** Phase 5 — Goal a1b2c3 started. Cloned 5 reference repos. Built EventBus, Mission/Session/Agent stores, Activity grammar, Tool Runtime with 22 tools, Fleet dashboard, Session view. Fixed compile errors in fleet.rs/session.rs. All 455 tests pass.

---

## Changelog

- **2026-08-16:** Started implementation. Phase 1 (T1–T6) implemented. Resize fix, slash menu, @ autocomplete, Esc/Ctrl+C indicators, mouse routing.
- **2026-08-17 (morning):** Phase 2 (T7–T9) implemented. Context transparency, crash-safe persistence, OS notifications. Phase 3 (T10–T12) implemented. Checkpoints/rewind, permissions wiring, mid-turn steering.
- **2026-08-17 (afternoon):** Phase 4 (T13–T15) implemented. Universal ListCursor, mouse hover/click, typing/mode indicator. 455 tests pass.
- **2026-08-17 (evening):** Phase 5 — Goal a1b2c3 started. Cloned 5 reference repos. Built EventBus, Mission/Session/Agent stores, Activity grammar, Tool Runtime with 22 tools, Fleet dashboard, Session view. Fixed compile errors. 455 tests pass.
- **2026-08-17 (night):** Completed the 5 remaining goal tasks:
  - **T6 LLM tool-calling loop:** `tools`/`tool_calls` on `CompletionRequest`/`CompletionResponse`; `run_tool_loop` + `LoopMessage`/`LoopOutput` in `src/runtime/mod.rs` executes provider-requested tools via `ToolRegistry` and feeds results back; emits `ToolStarted`/`ToolCompleted`/`ToolFailed` events. 2 new tests.
  - **T9 Composer internals:** Shift+Enter newline, Ctrl+R reverse search (`InputAction::ReverseSearch`), Alt/Ctrl word nav, queued prompts (`InputState::queue_prompt`). 5 new tests.
  - **T10 Command system:** `CommandCategory` enum, per-command `group`/`aliases`/`category`, alias resolution, `by_group`/`by_category`/`groups`. 5 new tests.
  - **T16 Persistence:** `src/persistence/mod.rs` snapshots `Mission` → `.niki/missions/<id>.json` (path-traversal sanitized), with `save/load/list/delete/restore_latest`. 3 new tests.
  - **Integration:** Fleet/Session pages wired into `render()` and the TUI key loop (`g`/`s` navigation, arrows/j/k, Enter opens session, Esc back, Tab cycles session tabs); `AppState` gains `stores`/`fleet`/`session_view`. 616 tests pass (370 lib + 246 integration).

(End of file - total 216 lines)

