# NIKI TUI Architecture Audit

Generated: 2026-08-16 | Phase 0 of TUI reconstruction

## Executive Summary

The display codebase (`src/display/`, ~12,300 lines across 44 files) contains **two complete, mutually exclusive UI systems**. The live one is the older page-based architecture. The newer reactive/chat architecture is fully built but never wired into the binary. The reconstruction plan wires the dead island as the canonical system.

---

## Live System (to be retired)

### State: `pages::AppState` (`src/display/pages/mod.rs:155`)
- `current_page: PageId` (12 variants: Run, Pipeline, Agents, Diff, Verdict, Cost, Artifacts, History, Config, Help, TestLog, Chat)
- `run_state`, `stages`, `description`, `branch_name`, `revision_round`, `max_revision_rounds`
- `chat_input`, `chat_cursor`, `chat_copy_mode`, `chat_sel_anchor`, `chat_cursor_pos`, `chat_copied`, `chat_lines`, `chat_log`
- `diff_content`, `report_content`, `cost_json`, `test_log`, `artifacts_dir`
- `config`, `project_path`, `modal`, `onboarding`, `tips`, `show_command_palette`
- `finished`, `start_time`, `paused`, `tick`

### PageRouter (`src/display/pages/mod.rs:410`)
- `HashMap<PageId, Box<dyn Page>>` — 12 pages registered
- `render_current(frame, area, state)` — delegates to `Page::render`
- `handle_key(key, state)` — delegates to `Page::handle_key`

### Event flow
```
pipeline.rs → agents/mod.rs → AgenticDisplay.emit() → mpsc::channel<DisplayEvent>
→ tui.rs:recv_timeout(16ms) → state.apply_event(ev)
→ router.render_current() + chat rendering
```

Single channel, single state, single consumer. All views read the same state.

### TUI loop (`src/display/tui.rs:run_tui`, line 113)
- 33ms min frame interval (~30fps cap)
- `terminal.draw(|f| render(f, &state, &router, &palette))`
- `render()` (line 500): logo(8 lines) + content + status_line(1) + modal/onboarding/palette overlays
- Input dispatch priority: onboarding → modal → palette → Tab toggle → chat special-case → palette/theme → page router
- Exit on `Disconnected` (sender dropped) or quit command

### Render path
- `router.render_current()` dispatches to individual page renderers
- `render_status_line()` (tui.rs:416) — standalone, does not use components/status_bar.rs
- No cell-diffing, no FPS target, no differential redraw

### DisplayEvent definition (`src/display/tui.rs:40`)
```rust
pub enum DisplayEvent {
    Banner { description: String },
    StageStart { role: AgentRole },
    StageToken { role: AgentRole, token: String },
    StageDone { role, summary, input_tokens, output_tokens, cost_usd, latency_ms },
    StageFailed { role, error },
    Revision { round, max, issues },
    DiffContent(String),
    ReportContent(String),
    CostJson(String),
    TestLogContent(String),
    ArtifactsDir(String),
    Final,
    BranchName(String),
    StageTotals { input_tokens, output_tokens, cost_usd, latency_ms },
}
```

### Live display modules
| Module | Lines | Purpose |
|--------|-------|---------|
| `tui.rs` | 587 | Main loop, render, input, event dispatch |
| `pages/mod.rs` | 451 | AppState, PageRouter, Page trait, apply_event |
| `pages/*.rs` (12) | ~3,200 | Page renderers |
| `agent_stream.rs` | 558 | AgenticDisplay, DisplayEvent bridge |
| `banner.rs` | ~150 | Completion banner |
| `command_palette.rs` | 250 | Ctrl+P palette (13 items, cosmetic) |
| `completion.rs` | ~200 | Completion rendering (non-TUI) |
| `modal.rs` | 115 | Confirm/Quit modal |
| `onboarding.rs` | 650 | Onboarding wizard |
| `logo.rs` | 100 | Logo rendering |
| `theme.rs` | 817 | Color palette |
| `tips.rs` | 200 | Tips banner (constructed, never rendered) |
| `artifact_render.rs` | ~130 | Test report summary |
| `pipeline_status.rs` | ~70 | Non-TUI status |

### Dead event emitters
| Event | Handler exists | Emitter exists | Actually called |
|-------|---------------|---------------|-----------------|
| `StageTotals` | pages:357, state:558 | agent_stream.rs (emitter function) | **NEVER** |
| `TestLogContent` | pages:348, state:549 | agent_stream.rs (emitter function) | **NEVER** |
| `Revision` | pages:326, state:528 | agent_stream.rs:384 revision_requested | **NEVER** |

---

## Dead Island (built, never wired)

### State: `state::AppState` (`src/display/state.rs:291`)
- `view: ViewMode` (Chat | Page(PageId)) — no Chat variant in PageId (11 variants)
- `messages: Vec<Message>`, `input_state: InputState` (buffer, cursor, history, mode)
- `show_command_menu`, `command_filter`, `command_selected`, `commands: Vec<Command>`
- `show_permission_modal`, `permission_request: Option<PermissionRequest>`, `permission_selected`
- `context_usage`, `token_count`, `context_limit`, `cost`, `model`
- `pipeline: PipelineState` (nested stages), `run_state`, `revision_round`, `max_revision_rounds`
- `diff_content`, `report_content`, `cost_json`, `test_log`, `artifacts_dir`
- `finished`, `paused`, `tick`, `branch_name`, `description`, `notes`

### `apply_display_event()` (state.rs:454)
Nearly identical to pages `apply_event()`. Differences:
- Uses `self.pipeline.stages` (nested) vs flat `self.stages`
- Uses `theme::warning()` / `theme::text_dim()` for Revision colors (better)
- Lacks `start_time` tracking (first StageStart)

### RenderEngine (`src/display/engine.rs:130`)
- Cell-diffing with front/back buffers, dirty-flag rendering
- `FrameTarget`: High=16ms/60fps, Low=33ms/30fps
- `render(&AppState)` dispatches through `layout::render_chat`/`render_page` + overlays
- **Annotated `#[allow(dead_code)] // compiled but unreachable until chat UI is wired`**

### InputHandler (`src/display/input.rs:25`)
- Three modes: Insert, Command, Shell
- `InputState`: buffer, cursor_pos, history, history_index, mode, autocomplete
- `handle_insert/handle_command/handle_shell` — full editing, history, autocomplete
- Returns `InputAction::Submit(buffer)` on Enter
- Dead: only tested, never imported

### Components (`src/display/components/`, all dead)
| Component | Lines | Purpose | Used by |
|-----------|-------|---------|---------|
| `status_bar.rs` | 88 | Status bar rendering | layout/mod.rs only |
| `spinner.rs` | 198 | Spinner animation | never |
| `autocomplete.rs` | 110 | Autocomplete dropdown | engine.rs only |
| `command_menu.rs` | 128 | Slash command menu | engine.rs only |
| `input_box.rs` | 88 | Input box rendering | layout/mod.rs only |
| `permission.rs` | 112 | Permission modal | engine.rs only |
| `progress.rs` | 77 | Progress bar/gauge | never |

### Chat stack (`src/display/chat/`, all dead)
| Module | Lines | Purpose |
|--------|-------|---------|
| `markdown.rs` | 384 | Incremental markdown renderer |
| `message.rs` | 349 | Message rendering with metadata |
| `streaming.rs` | 191 | Streaming message with incomplete-fence tracking |
| `code_block.rs` | 136 | Code block rendering |
| `mod.rs` | 16 | Re-exports |

### Layout (`src/display/layout/mod.rs`, 422 lines, dead)
- `render_chat(frame, area, state)` — dispatches to message/streaming renderers
- `render_page(frame, area, page_id, state)` — dispatches to page renderers

### Permission system (`src/permissions/mod.rs`, dead)
- `PermissionChecker` with `Permission` enum (Allow/Ask/Deny)
- `check_tool()`, `check_command()`, `auto_approve()`
- Config-driven allow-list/deny-list matching
- **Not referenced by any live code**

---

## Key Differences: Live vs Dead

| Concern | Live | Dead |
|---------|------|------|
| State access | `self.stages` (flat) | `self.pipeline.stages` (nested) |
| Chat rendering | `pages/chat.rs` plain Paragraph | `chat/` markdown + streaming stack |
| Input | inline editor in `chat.rs:353-478` | `input.rs` InputHandler (3 modes) |
| Commands | `command_palette.rs` (13 static items) | `state.rs` Command struct + registry |
| Permissions | display-only modal (never constructed) | `permission.rs` + `PermissionRequest` |
| Render | tui.rs draw loop | `engine.rs` cell-diffing + 60fps |
| Shell | tui.rs hardcoded layout | `layout/mod.rs` header/viewport/input |

---

## Migration Boundary

The dead island is the target architecture. It already contains:
- Canonical AppState with input/command/permission/pipeline/telemetry fields
- Cell-diffing RenderEngine with 60fps streaming
- Full InputHandler with 3 modes + autocomplete
- 7 UI components (status_bar, spinner, autocomplete, command_menu, input_box, permission, progress)
- Markdown streaming chat stack (incremental renderer, incomplete-fence tracking)

What needs wiring:
1. tui.rs → engine.rs RenderEngine (replace render loop)
2. input.rs InputHandler → global input (replace inline editor)
3. components/ → layout shell (status_bar, permission, etc.)
4. chat/ markdown stack → chat view rendering
5. state.rs apply_display_event → port from pages/mod.rs:254-381 (minor differences)
6. `niki chat` command entry point

What needs building:
- Runtime interactivity: real permissions (PermissionChecker gate at sandbox exec), cancellation, user messages mid-session
- Event emitters: StageTotals, TestLogContent, Revision from pipeline.rs
- Progressive disclosure agent nodes

---

## Tests

122 tests in `src/display/`, ~60 on dead code. Dead tests will become live as wiring lands. Keep green throughout.

| Module | Tests |
|--------|-------|
| theme.rs | 817 lines, 14 tests |
| state.rs | 741 lines, 9 tests |
| input.rs | 412 lines, 11 tests |
| engine.rs | 377 lines, 7 tests |
| tui.rs | 587 lines, 2 tests |
| pages/chat.rs | 672 lines, 5 tests |
| pages/diff.rs | 335 lines, 4 tests |
| pages/help.rs | 231 lines, 4 tests |
| onboarding.rs | 650 lines, 11 tests |
| tips.rs | 200 lines, 9 tests |
| logo.rs | 100 lines, 3 tests |
| chat/markdown.rs | 384 lines, 8 tests |
| chat/streaming.rs | 191 lines, 5 tests |
| chat/message.rs | 349 lines, 7 tests |
| chat/code_block.rs | 136 lines, 3 tests |
| components/* | 702 lines, 14 tests |
| layout/mod.rs | 422 lines, 4 tests |
