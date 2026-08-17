# NIKI TUI — Command UX & Long-Session Reliability: Implementation Progress

> Plan: `.kilo/plans/1786894348770-niki-tui-ux-reliability-plan.md`
> Research: `research/claude-opencode-tui-unified-plan.md`
> Started: 2026-08-16

## Status
- **Phase 1 (UX breakages):** COMPLETE
- **Phase 2 (reliability UX):** COMPLETE
- **Phase 3 (advanced):** COMPLETE
- **Phase 4 (universal cursor + typing indicator):** COMPLETE

## Task checklist
- [x] **T1** — Fix resize repaint (run_tui missing `Event::Resize` arm) — done: `Event::Resize` arm calls `engine.mark_dirty()` (tui.rs:497–501)
- [x] **T2** — Wire slash command menu — done: `show_command_menu` nav branch, `command_filter` sync, Enter dispatch via `get_selected_command` + `ChatPage::handle_key` (tui.rs:353–414, command_menu.rs:90–108)
- [x] **T3** — Wire `@` file autocomplete — done: `sync_input_overlays` calls `build_candidates` + `project_files` (chat.rs:481–516, autocomplete.rs:78–91)
- [x] **T4** — Global `Esc` semantics + indicator — done: `InputHandler::handle_insert` returns `InputAction::Cancel` → `state.request_cancel` with notice (input.rs:43, chat.rs:381–384)
- [x] **T5** — `Ctrl+C` two-press exit + indicator — done: `last_ctrl_c` tracking + `request_cancel`; `/undo`/`/redo` git path preserved (tui.rs:331–352)
- [x] **T6** — Route mouse/trackpad to active overlay — done: `Event::Mouse` now routes to `show_command_menu` / `show_command_palette` / `show_permission_modal` / Chat (tui.rs:480–516)
- [x] **T7** — Context transparency bar + auto-compact wiring
- [x] **T8** — Crash-safe incremental pipeline persistence
- [x] **T9** — OS-level notifications (input needed / run complete)
- [x] **T10** — Surface checkpoints/rewind in TUI
- [x] **T11** — Wire `[permissions]` config into PermissionChecker
- [x] **T12** — Mid-turn steering
- [x] **T13** — Universal list-cursor abstraction — done: `ListCursor` + `FocusState` in `list_cursor.rs` (`prev`/`next`/`submit`/`hover`/`click`, list_cursor.rs:13, 23, 79–130) shared by the command palette (`CommandPalette.cursor`, command_palette.rs:14), the slash menu (`command_menu::cursor`, command_menu.rs:32) and the permission modal (`permission::cursor`, permission.rs:27)
- [x] **T14** — Mouse hover-highlight + click-to-select — done: `Event::Mouse` routes via `active_focus` in priority order permission → palette → slash menu → chat (tui.rs:115–127, 509–602); hover (`Moved`/`Drag`) moves the highlight through `ListCursor::hover`, left-press activates through `ListCursor::click`; hit-tests: `permission::click_index` (permission.rs:54), `command_palette::click_index` (command_palette.rs:235), `command_menu::click_index` (command_menu.rs:109)
- [x] **T15** — Typing / line / mode indicator — done: `status_bar.rs` shows MODE badge, `Ln X, Col Y`, `Typing…`, and transient notice line (status_bar.rs:71–99)

## Verification
- `cargo build` green after each batch
- `cargo test` (display module + existing tui_navigation) stays green
- Manual (TTY): resize, `/` filter, `@`, Esc/Ctrl+C flows, panel arrow+click nav, footer indicator

## Changelog
- 2026-08-16: removed stale 16-phase progress.md; started implementation from finalized plan.
- 2026-08-17: T1–T6, T13–T15 implemented. `clear_stale_notice` in render tick; `click_index` for menu click-to-select; status_bar mode/line/col/typing indicator. `cargo build` + `cargo test` (318 lib tests + 8+7+11+26+5+18+12 integration) green.
- 2026-08-17: T7–T9 implemented. T7: `ContextBudget` added to `PipelineState` and `PipelineResult`; `update_context_budget` updates from accumulated metrics, writes `context.json`, triggers `compress_context` at 80% threshold (200k capacity). T8: `task_dir` passed into `execute_pipeline`; `TaskRecord` saved incrementally after each stage via `save_task_record`; status Running → Completed/Failed/Cancelled. T9: `notify-rust` dependency; `display/notify.rs` module; `PermissionRequest` handler in `apply_display_event` now sets `show_permission_modal = true` and emits notification; pipeline completion/failure/cancellation notifications emitted through `AgenticDisplay::show_completion` + run.rs error path. All 543 tests pass.
- 2026-08-17: T10–T12 implemented. T10: `SessionManager` helper methods (`undo`, `redo`, `rewind`, `load_current`, `save_current`, `checkpoint_labels`, `load_or_create_current`); `current_git_commit` helper; `CURRENT_SESSION_ID` const; checkpoint created after Planner stage in `execute_pipeline`; `/undo`/`/redo` in `chat.rs` replaced with SessionManager calls; `/rewind` added to chat page + default commands + CLI registry + `CommandAction::Rewind`. T11: `build_permission_checker` accepts `&NikiConfig`; merges `PermissionRuleConfig` rules with permission-string→enum mapping (`Permission::Ask` default); `&NikiConfig` threaded through `create_sandbox`, `DockerSandbox::create`, `WorktreeSandbox::create`; call sites updated; test fixed. T12: `steer_channel: Arc<Mutex<Option<String>>>` on `AppState`; `DisplayEvent::SteerChannel` variant; created in `execute_pipeline`, emitted to TUI; `run_agent`/`run_stage`/`run_role` accept steer param and poll between streaming chunks; `/steer` command in chat page + command palette. `cargo build` + `cargo test` pass.
- 2026-08-17: Phase 4 complete (T13/T14 promoted from partial → done). New `src/display/components/list_cursor.rs` exports the universal `ListCursor` (`prev`/`next`/`submit`/`hover`/`click`/`set_count`, wrapping + clamping) and `FocusState` (Chat / CommandMenu / CommandPalette / Permission). `command_palette.rs` now stores `cursor: ListCursor` instead of a bare `selected` (public `new`/`handle_key`/`render_command_palette` unchanged, plus `selected()`, `hover()`, `click()`, `popup_rect()`, `click_index()`); `command_menu.rs` gained `filtered_commands`/`filtered_count`/`cursor` and its `click_index` is now bounded by the visible row count; `permission.rs` renders the three options one-per-row driven by `ListCursor`, with `modal_rect`, `click_index`, `action_for` (Allow once/Allow always → Allow, Deny → Deny) and no change to the `DisplayEvent::PermissionRequest` protocol. `run_tui` gained `active_focus()` and its `Event::Mouse` arm now routes to the active overlay in priority order (permission → palette → slash menu → chat copy-mode): `Moved`/`Drag` hover-highlights the row under the pointer, left-press selects (slash menu) or activates (palette, permission response). Permission modal also got Up/Down/`k`/`j` nav with Enter confirming the highlighted option; slash-menu Up/Down now wrap through `ListCursor`. T15 footer (mode badge + `Ln X, Col Y` + `Typing…` + notice) verified unchanged. `cargo build` + `cargo test` green: 337 lib tests (incl. 6 `list_cursor`, 3 `command_palette`, 2 new `command_menu`, 2 new `permission`, 1 `active_focus_priority`) + 246 integration tests, 0 failures.
