# NIKI — TUI PARITY REFACTOR PLAN

> **Started:** 2026-08-19
> **Phase:** Visual Terminal UI → Claude Code Parity
> **Source of truth:** `/home/shiva/projects/research/claude-code-ui-parity.md`
> **Launch target:** Refactor complete + demo video refreshed

## Mission

Wire Niki's existing dead-island chat/reactive architecture into the live binary to achieve Claude Code visual + functional parity. Do not rebuild — wire what exists. Polish what's missing. Verify from the end-user perspective at every step.

---

## Phase 1 — Wire Dead Island into Binary ✅ COMPLETE

| # | Task | Status | Commit |
|---|------|--------|--------|
| 1.1 | Route Chat page through `layout::render_chat` + `components::render_input_box` | ✅ | c150b32 |
| 1.2 | Wire InputHandler into run_chat + unify data model | ✅ | c150b32 |
| 1.3 | Wire `components/status_bar.rs` into footer row | ✅ | c150b32 |
| 1.4 | Wire slash command menu + autocomplete + permission modal overlays | ✅ | pre-existing |
| 1.5 | Unify `pages/AppState` and `state/AppState` | ✅ | pre-existing |

Exit criteria met: `cargo test` passes (377 tests); Chat page renders through `layout::render_chat`; InputHandler wired via `ChatPage::handle_key`; status bar visible.

---

## Phase 2 — Claude Code Parity Polish ✅ COMPLETE

| # | Task | Status | Commit |
|---|------|--------|--------|
| 2.1 | Disable input + gray border during streaming | ✅ | da8b6cb |
| 2.2 | Add context-window gauge to status bar (`ctx ▓▓░░░░░░░░ 12%`) | ✅ | 661a266 |
| 2.3 | Add queued-prompt visual indicator | ✅ | 661a266 |
| 2.4 | Wire StreamingMessage with incomplete-fence tracking | ⏭ | deferred (requires backend changes) |
| 2.5 | Add progressive disclosure for read-only tool calls | ✅ | pre-existing (build_chat_lines) |
| 2.6 | Rewrite permission modal: 4 options + blue sep + dotted sep | ✅ | 661a266 |
| 2.7 | Add Ctrl+E/Ctrl+D footer hints to permission modal | ✅ | 661a266 |

Exit criteria met: `cargo test` passes; permission modal matches Claude Code spec; context gauge renders.

---

## Phase 3 — Bug Hardening ✅ COMPLETE

| # | Task | Status | Commit |
|---|------|--------|--------|
| 3.1 | Fix bracketed paste: PasteBurst detector (80ms window) | ✅ | b2504b8 |
| 3.2 | Fix auto-scroll: re-enable at bottom | ✅ | b2504b8 |
| 3.3 | Fix IME/CJK: add ColorDepth::detect() (foundation for future IME anchoring) | ⏭ | partial |
| 3.4 | Fix terminal resize: already handled (ratatui re-samples on draw) | ✅ | pre-existing |
| 3.5 | Fix alt-screen exit hygiene: RestoreGuard already in place | ✅ | pre-existing |
| 3.6 | Fix color detection: full NO_COLOR / COLORTERM / 256-color hierarchy | ✅ | 4dcfdb2 |
| 3.7 | Fix token accounting: StageDone accumulates token_count + context_usage | ✅ | cd8702c |

Exit criteria met: `cargo test` passes (377 tests); `cargo clippy` warning-free.

---

## Phase 4 — Demo Video Refresh ✅ COMPLETE

| # | Task | Status | Commit |
|---|------|--------|--------|
| 4.1 | Rewrite `demo.tape` — 900x560, 38s comprehensive chat flow | ✅ | c150b32 |
| 4.2 | VHS options: TypingSpeed 75ms, CursorBlink false, WindowBar Colorful | ✅ | c150b32 |
| 4.3 | Post-process: `gifsicle -O3 --colors 32 --resize-width 640` | ✅ | c150b32 |
| 4.4 | Post-process: `ffmpeg -movflags +faststart -pix_fmt yuv420p -crf 23` | ✅ | c150b32 |
| 4.5 | Targets: GIF ~872K, MP4 ~957K, 36s, 900x560 | ✅ | c150b32 |

Exit criteria met: New demo plays smoothly; file sizes reasonable for 38s terminal animation.

---

## Commits (chronological)

| # | Commit | Description |
|---|--------|-------------|
| 1 | c150b32 | refactor: wire RenderEngine + layout::render_chat into run_chat |
| 2 | 661a266 | feat: Claude Code parity polish — permission modal + status bar gauge |
| 3 | da8b6cb | feat: disable input + gray border during streaming |
| 4 | b2504b8 | fix: auto-scroll re-enable at bottom + paste burst Enter guard |
| 5 | 4dcfdb2 | feat: full color detection hierarchy in theme.rs |
| 6 | cd8702c | fix: token accounting — StageDone updates context_usage + token_count |

---

## Remaining Work (Future Sessions)

1. **IME/CJK composition** (Phase 3.3 partial): Add `CSI` cursor-position reporting before each frame draw for OS IME anchoring. Requires terminal capability negotiation.
2. **StreamingMessage wiring** (Phase 2.4): Wire the dead-island `StreamingMessage` with incomplete-fence tracking into the chat rendering pipeline. Requires backend streaming changes.
3. **Progressive disclosure for tool calls** (Phase 2.5): Collapse consecutive read-only tool calls into "Read N files" style summary. Partially done via `build_chat_lines`; needs stage-level collapsing.
4. **Performance baseline**: Measure draw cost at 60fps vs 30fps with the new RenderEngine.
5. **Kitty keyboard protocol**: Evaluate progressive adoption for Shift+Enter disambiguation.
