# NIKI — TUI PARITY REFACTOR PLAN

> **Started:** 2026-08-19
> **Phase:** Visual Terminal UI → Claude Code Parity
> **Source of truth:** `/home/shiva/projects/research/claude-code-ui-parity.md`
> **Launch target:** Refactor complete + demo video refreshed

## Mission

Wire Niki's existing dead-island chat/reactive architecture into the live binary to achieve Claude Code visual + functional parity. Do not rebuild — wire what exists. Polish what's missing. Verify from the end-user perspective at every step.

---

## Phase 1 — Wire Dead Island into Binary

| # | Task | Status |
|---|------|--------|
| 1.1 | Replace naive `terminal.draw()` in `run_chat` with `engine.render(&state)` + `layout::render_chat` | pending |
| 1.2 | Port `state.rs::AppState::apply_display_event` into live event loop (merge flat vs nested stage models) | pending |
| 1.3 | Wire `input.rs::InputHandler` into `run_chat` key dispatch (replace inline input handling) | pending |
| 1.4 | Wire `components/status_bar.rs` into footer row of `layout::render_chat` | pending |
| 1.5 | Wire slash command menu (`state.rs::Command` registry) into live menu overlay | pending |
| 1.6 | Unify `pages/AppState` and `state/AppState` — eliminate the dead live `pages/mod.rs::AppState` | pending |

Exit criteria: `cargo test` passes; `niki chat` renders through RenderEngine + layout::render_chat; input flows through InputHandler; status bar visible.

---

## Phase 2 — Chat Polish (Claude Code Parity)

| # | Task | Status |
|---|------|--------|
| 2.1 | Disable input + gray border during streaming (match Claude Code behavior) | pending |
| 2.2 | Add context-window gauge to status bar (`Context █░░░░░░░░░ 8%`) | pending |
| 2.3 | Add queued-prompt visual indicator | pending |
| 2.4 | Wire `StreamingMessage` with incomplete-fence tracking into chat rendering | pending |
| 2.5 | Add progressive disclosure: collapse consecutive read-only tool calls | pending |
| 2.6 | Rewrite permission modal to match Claude Code layout (tool line → blue separator → description → dotted separator → options → footer hint) | pending |
| 2.7 | Add `Ctrl+E` (explanation) + `Ctrl+D` (raw params) expandable section to permission modal | pending |

Exit criteria: `cargo test` passes; visual diff against research §2.1 shows matching layout; permission prompt matches spec.

---

## Phase 3 — Bug Hardening

| # | Task | Status |
|---|------|--------|
| 3.1 | Fix bracketed paste: implement PasteBurst detector (buffers Enter during paste bursts) | pending |
| 3.2 | Fix auto-scroll: gate on "near bottom", never force-scroll when user scrolled up | pending |
| 3.3 | Fix IME/CJK composition: report cursor position for OS IME anchoring | pending |
| 3.4 | Fix terminal resize: ensure layout reflows correctly under width/height changes | pending |
| 3.5 | Fix alt-screen exit hygiene: restore raw mode, leave alt screen, release mouse capture on all exit paths (including panic) | pending |
| 3.6 | Fix color detection: handle NO_COLOR, TERM=dumb, COLORTERM=truecolor, 256-color, ANSI-16 fallback, Windows Terminal COLORTERM gap | pending |
| 3.7 | Fix token accounting: ensure context-usage counter matches actual API response (Claude Code #41181 pattern) | pending |

Exit criteria: `cargo test` passes; manual testing on kitty, Ghostty, tmux 3.4+, Windows Terminal, and iTerm2.

---

## Phase 4 — Demo Video Refresh

| # | Task | Status |
|---|------|--------|
| 4.1 | Rewrite `demo.tape` using VHS: multi-step flow (welcome → typing → streaming → tool call → permission → completion) | pending |
| 4.2 | Set VHS options: TypingSpeed 75ms, CursorBlink false, WindowBar Colorful, Padding 20, FontSize 20 | pending |
| 4.3 | Post-process: `gifsicle -O3` for GIF, `ffmpeg -movflags +faststart -pix_fmt yuv420p` for MP4 | pending |
| 4.4 | Targets: GIF ~500KB, MP4 ~200KB, 15–20s, 1200×720 | pending |
| 4.5 | Replace `assets/demo.gif` and `assets/demo.mp4` | pending |

Exit criteria: New demo plays smoothly in GitHub README and landing page; file sizes within target.

---

## Commits (one per phase)

| # | Commit message |
|---|----------------|
| C1 | refactor: wire RenderEngine + layout::render_chat into run_chat |
| C2 | refactor: port state.rs AppState + InputHandler into live binary |
| C3 | feat: Claude Code parity polish — streaming disable, context gauge, queued indicator, progressive disclosure |
| C4 | fix: permission modal rewrite + Ctrl+E/Ctrl+D expandable details |
| C5 | fix: bracket paste, auto-scroll, IME/CJK, resize, exit hygiene, color detection, token accounting |
| C6 | demo: refresh demo.tape + render GIF/MP4 with VHS + gifsicle/ffmpeg |
