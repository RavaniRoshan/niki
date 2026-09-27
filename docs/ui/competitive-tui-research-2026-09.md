# Competitive terminal-UI research: what makes AI coding CLIs feel fast, premium, "cool"

Research date: 2026-09-27. Sources are file paths in the upstream repos (read at `main`/`dev`) or URLs.
Where I could not read source (Claude Code ships a compiled native binary since v2.x), I mined the
public `CHANGELOG.md` — which is an unusually rich UX bug log — plus the official docs.

---

## 0. Baseline: what niki already has (so recommendations are grounded)

niki is **not** starting from zero. Verified in the working tree:

| Capability | Location | Notes |
|---|---|---|
| Differential cell rendering | `src/display/engine.rs:1-9` | Delegates to ratatui's front/back `Buffer` diff — correct, don't replace |
| Dirty-flag redraw + frame-rate policy | `src/display/engine.rs:107-135,168-186` | `FrameTarget::High`=16 ms / `Low`=33 ms, `mark_dirty_reason` |
| DEC 2026 synchronized output | `src/display/tui.rs:202-206, 288-296, 313-316` | Correctly opened/closed *inside* the loop, with a comment explaining why it is not opened at startup |
| Frame telemetry (mean / p95) | `src/display/engine.rs:16-98`, `src/display/tui.rs:319-326` | 120-sample ring buffer |
| Motion system + reduced motion | `src/display/motion.rs:1-11` | "Every effect is a pure function of `(tick \| elapsed_ms)`" — matches Codex's model |
| Kitty keyboard protocol (I4) | `src/display/tui.rs:197-200` | Shift+Enter disambiguation |
| Terminal truecolor / NO_COLOR detection | `src/display/theme.rs:158-168` | |
| Fuzzy matching (`nucleo` 0.5) | `src/display/components/autocomplete.rs:3,82-115`, `command_menu.rs:3,24` | Only in `@`-mention and `/` menu |
| Rotating tips | `src/display/tips.rs:60-100` | 40+ tips, time-rotated |
| Full page model | `src/display/pages/*.rs` | 15 pages: chat, run, diff, verdict, cost, fleet, agents… |

**The real gaps** (everything in §2–§4 that niki lacks) are:
`command_palette.rs` is a **fixed list with a cursor** — no text input, no fuzzy search, no leader key
(`src/display/command_palette.rs:35-47,143,264`). `status_bar.rs` has one guard clause
(`if width < 10` at `src/display/components/status_bar.rs:15`) — no collapse ladder. There is no vim
mode, no which-key/leader overlay, no self-scheduled animation frames (the loop is a fixed 16/33 ms
poll, so the spinner costs CPU even when nothing is animating), no stream pacer, no
replace-in-place progress rows, and no animated empty state.

---

## 1. Side-by-side feature matrix

Legend: ● full / ◐ partial / ○ absent / n/a.

| Premium-feeling feature | Claude Code | OpenCode | Codex TUI | Kimi Code CLI | Gemini CLI |
|---|---|---|---|---|---|
| **Rendering model** | Inline terminal app (not alt-screen by default); optional fullscreen mode (`CC changelog:431,1116`) | Full alt-screen, SolidJS + `@opentui/core` renderer | Ratatui, **custom forked Terminal** + inline viewport, optional alt-screen | Custom in-house `@moonshot-ai/pi-tui`; `Component.render(width): string[]` + `ui.requestRender()` | Ink (React) full alt-screen |
| **Streaming render strategy** | Per-token append; heavy rework for lag (changelog:915,1204,1543) | Solid reactive diff; spinner via `<spinner interval={80}>` (`component/spinner.tsx:11`) | **Newline-gated accumulator + adaptive smooth/catch-up pacer** (`streaming/chunking.rs`, `markdown_stream.rs`) | **Throttled flush at `STREAMING_UI_FLUSH_MS`** with immediate idle flush (`controllers/streaming-ui.ts:456-495`) | Ink render-per-state-change; `StreamingState.{Idle,Responding,WaitingForConfirmation}` (`hooks/useGeminiStream.ts:186-217`) |
| **Flicker prevention** | Fixed "blank screen flashing before first frame" (changelog:130) | opentui internal | **`stdout().sync_update()` around every frame** (`tui.rs:1261-1284`) | pi-tui internal | Ink internal |
| **Tool-call display** | Collapsed rows, `Called slack 3 times` (docs/interactive-mode) | Inline icon+text for cheap tools, left-bordered `BlockTool` panel for heavy ones (`routes/session/index.tsx:1986-2043`) | `history_cell/*`; glyphs `• `, `↳ `, `  └ ` (`history_cell/exec.rs:45-51,70,203`) | Tool cards + `OUTCOME_MAX_LINES=3` glance rows (`constant/rendering.ts`) | **Compact-display allowlist** for high-signal tools, full display otherwise (`ToolGroupMessage.tsx:46-73`) |
| **Diff view** | `/diff` panel, scrollbar, no-wrap long paths (changelog:320,1357) | `feature-plugins/system/diff-viewer.tsx` (37 KB) + file tree, split/unified toggle, hunk nav | `diff_render.rs` (2 745 lines) + `get_git_diff.rs` | `components/media/diff-preview.tsx`, LCS DP diff | Inline diff inside `ToolGroupMessage` |
| **Command palette** | `/` menu, fuzzy, Tab to open (docs/interactive-mode) | `ctrl+p`; `fuzzysort`; **Suggested** section prepended (`component/command-palette.tsx:24,66-73`) | `bottom_pane/command_popup.rs` + `slash_commands.rs`; `nucleo` in workspace deps | `/` registry + `plugins-selector.tsx` | `slashCommandProcessor.ts` + `useSlashCompletion` |
| **Leader key** | ○ (readline + `Ctrl+X Ctrl+*` chords) | ● **`ctrl+x`** leader, `<leader>t/l/m/n/a/c/x/y/u/h` (`config/keybind.ts:38-130`) | ○ (has `keymap.rs` context stack) | ○ | ○ |
| **which-key overlay** | ○ | ● (`feature-plugins/system/which-key.tsx`, 20 KB, dock/overlay layouts) | ● (`shortcut_overlay`, `tui/shortcut_help.rs`) | ○ | ○ |
| **Vim mode in composer** | ● full, incl. `vimInsertModeRemaps` (jj→Esc, 1 s window), mode persists across panels (docs/interactive-mode) | ○ | ● `bottom_pane/textarea/vim.rs`, `vim_commands.rs`, `vim_search.rs`, `chat_composer/vim_history.rs` | ○ | ● `hooks/vim.ts` (50 KB) + `shared/vim-buffer-actions.ts` (52 KB) |
| **Theming** | `/theme` picker, `Ctrl+T` toggles code-block highlighting, light/ANSI themes | **30+ bundled JSON themes**, light/dark mode, `SIGUSR2` hot-reload (`context/theme.tsx:31-47`) | 6 `.tmTheme` assets (`assets/themes/{ada,babbage,curie,cushman,dali,davinci}`) | `currentTheme.palette` + chalk hex | `semantic-colors.ts` + Material themes (web) |
| **Reduced motion** | ○ (n/a, no alt-screen animation layer) | `kv` key `animations_enabled`, spinner falls back to `⋯ text` (`component/spinner.tsx:16`) | ● **OS-level**: macOS `accessibilityDisplayShouldReduceMotion`, Win `SPI_GETCLIENTAREAANIMATION`, Linux xdg `reduced-motion` (250 ms timeout) (`system_motion.rs`) | ○ | ○ |
| **Shimmer / gradient sweep** | ○ | ● `component/bg-pulse-render.ts` (15 KB) | ● per-char cosine color sweep, 2.0 s, band ±5, truecolor-only with DIM/BOLD fallback (`shimmer.rs`) | ● rainbow-dance easter egg (`easter-eggs/dance`) | ○ |
| **Elapsed timer in status** | ● `(esc to cancel, 12s)` equivalent, "deep in thought" after 45 s (changelog:918) | ● | ● `fmt_elapsed_compact` → `0s`/`1m 00s`/`2h 03m 09s` (`status_indicator_widget.rs:74-83`) | ● `goalTimer` interval (`chrome/footer.ts:205,532`) | ● `(esc to cancel, ${elapsed}s)` (`LoadingIndicator.tsx:68-70`) |
| **Rotating contextual tips** | ● **`spinnerTipsOverride`, `tipsFile`, `cooldownSessions`, `priority`** (changelog:1716,5917) | ● `feature-plugins/home/tips-view.tsx` | ● `assets/tooltips.txt` | ● weighted rotation, 10 s, `buildWeightedTips` (`chrome/working-tips.ts:5-25`) | ● "witty phrases" |
| **Status/footer collapse ladder** | ● footer hints auto-derived from `keybindings.json` (changelog:40,276) | ● | ● **explicit priority ladder** documented in `bottom_pane/footer.rs:22-40` | ◐ | ● `isNarrowWidth` → column/row reflow (`LoadingIndicator.tsx:120-124`) |
| **Progress rows replace, not append** | ● changelog:1599 (fixed pile-up with subagents) | ◐ | ● activity cells are keyed & compared for equality before redraw (`chatwidget/streaming.rs:628-637`) | ● `staging-leases.ts`, `subagent-activity-store.ts` | ● `VirtualizedList.tsx` |
| **Permission prompt** | ● tabbed dialogs, per-option comment fields, Esc = No, Shift+Tab = session-allow | ● `routes/session/permission.tsx` (23 KB) | ● `bottom_pane/approval_overlay.rs` (94 KB) + `history_cell/approvals.rs` | ● `dialogs/approval-panel.ts` (15 KB) | ● `ToolConfirmationMessage.tsx` (37 KB) |
| **First-run trust dialog** | ● names the repo root the grant covers (changelog:2455) | ◐ | ● | ● **`TrustPromptComponent`**, "Don't trust" = exit, **strips C0/C1 control chars from workspace text** (`dialogs/trust-prompt.ts`) | ● |
| **Onboarding checklist** | ● (changelog:1048) | ● home logo + rotating placeholders | ● **animated blossom** | ● bordered welcome panel w/ Directory/Session/Model/Version | ● onboarding checklist |
| **Empty state** | ● | ● logo, placeholders "Fix a TODO in the codebase" (`routes/home.tsx:15-19`) | ● 10.8 s animated blossom, click-to-replay, **clock pauses when hidden** | ● bordered welcome box | ● |
| **Keyboard remap** | ● `~/.claude/keybindings.json`; footer hints regenerate | ● every binding user-overridable, `"none"` to unbind | ● `keymap.rs` + `keymap_setup.rs` | ◐ | ● `key/keyBindings.ts` (26 KB) |
| **Terminal capability probing** | ● kitty keyboard, iTerm2/Ghostty/ConEmu progress OSC, Warp hyperlinks, `/terminal-setup` (changelog:979,1022,1309) | ◐ | ● `codex_terminal_detection`, tmux, size monitor, per-terminal reflow caps | ◐ | ◐ |
| **Install story** | `curl \| bash`, brew cask, winget, apt/dnf/apk; npm deprecated (README) | `npm i -g opencode-ai`, curl script, brew | `curl \| sh` + brew cask + npm + 6 platform binaries (README) | **`curl \| bash`, single binary, "no Node.js required"** (README) | `npx`, npm, brew, MacPorts |
| **README hero** | **GIF** (`<img src="./demo.gif" />`) | static PNG screenshot | static PNG splash at 80 % | **GIF** (`./docs/media/intro.gif`, above the fold) | static PNG screenshot |

---

## 2. Transferable techniques for a ratatui app, ranked by impact ÷ effort

### Tier 1 — do these first. Each is small, and each is a *specific named thing* the competitors do.

**1. Replace the fixed poll loop with a self-scheduling frame requester.**
This is the single biggest structural gap. niki's loop wakes every 16/33 ms and burns CPU on the
spinner whether or not anything moved (`src/display/tui.rs:272-347`). Codex instead hands every
widget a cloneable `FrameRequester` and lets widgets decide when the next frame is due:

```rust
// codex-rs/tui/src/status_indicator_widget.rs — a widget re-arms its own next frame
let interval_ms = if animations_enabled && (effects.progress || effects.shimmer) { 32 } else { 1_000 };
self.frame_requester.schedule_frame_in(Duration::from_millis(interval_ms));
```

and the scheduler coalesces every request into **one** draw at the earliest deadline
(`codex-rs/tui/src/tui/frame_requester.rs:96-127`). Effect: when the user is staring at an idle
prompt, cost is zero, not 30 fps. Niki's `motion.rs` docstring already promises "one engine for every
effect, no ad-hoc ticks" — the *scheduling* half of that promise is missing.

**2. The adaptive stream pacer (smooth / catch-up with hysteresis).**
Not transferable verbatim — niki's stages are discrete events, not token deltas — but the *control
policy* is, and it is the answer to "why does Codex never look laggy":

```
ENTER_QUEUE_DEPTH_LINES: 8      // or depth 8 …
ENTER_OLDEST_AGE:        120ms  // … or age 120ms → leave Smooth, whatever the other metric
EXIT_QUEUE_DEPTH_LINES:  2
EXIT_OLDEST_AGE:         40ms
EXIT_HOLD:               250ms  // hysteresis
REENTER_CATCH_UP_HOLD:   250ms  // no gear-flapping
SEVERE_QUEUE_DEPTH_LINES: 64
SEVERE_OLDEST_AGE:        300ms
```
— `codex-rs/tui/src/streaming/chunking.rs:84-116`; the two-gear model is documented at
`chunking.rs:1-80`; `DrainPlan::Single` vs `DrainPlan::Batch(queued)` at
`streaming/commit_tick.rs:151-176`.

For niki the equivalent is a **stage-output pacer**: niki's `agent_stream.rs` should buffer a stage's
output and commit it in a paced drip (1 chunk per ~16 ms tick) while a backlog counter drains, rather
than dumping the whole buffered blob into the transcript the instant the stage finishes. Two
behaviours get fixed at once: a fast-finishing stage no longer causes a 400 ms freeze, and a slow
stage never lags behind reality.

**3. Newline-gated commit boundaries.**
`MarkdownStreamCollector` buffers raw source and only hands the renderer the prefix up to the last
newline, leaving the partial line buffered — so a `**bold` split across two deltas never renders as
half-markup. `codex-rs/tui/src/markdown_stream.rs:1-10, 20-31`. niki's `chat/markdown.rs` (23 KB) and
`chat/streaming.rs` should adopt the same shape.

**4. The footer collapse ladder.**
Codex documents the exact priority order it falls back through, and it is copy-pasteable
(`codex-rs/tui/src/bottom_pane/footer.rs:22-40`):

> Start with the fullest left-side hint plus the right-side context. When the queue hint is active,
> prefer keeping that queue hint visible, even if it means dropping the right-side context earlier;
> the queue hint may also be shortened before it is removed. When the queue hint is not active but the
> mode cycle hint is applicable, drop "? for shortcuts" before dropping "(shift+tab to cycle)". If
> "(shift+tab to cycle)" cannot fit, also hide the right-side context… Finally, try a mode-only line
> (with and without context), and fall back to no left-side footer if nothing can fit.

Two sub-lessons: (a) the footer is *pure rendering* from a `FooterMode` enum — it never decides what
to show; (b) footers are classified as **instructional** ("press again to quit") vs **contextual**
(status line, model, branch), so a transient instruction can never permanently displace context.
niki's `status_bar.rs` has exactly one width guard and would benefit from the same explicit ladder.

**5. Per-character shimmer with a terminal-capability fallback.**
`codex-rs/tui/src/shimmer.rs:19-70`: a 2.0 s cosine band of half-width 5 sweeps across the characters
of a status header; each char gets a blend from `default_bg()` toward `default_fg()` plus `BOLD`. When
`supports_color` says no 16m, it degrades to `DIM` / normal / `BOLD` by intensity. The clock is a
`OnceLock<Instant>` at process start, so **every** shimmer in the app is phase-locked — they pulse in
unison instead of beating against each other. niki's `motion.rs` has `shimmer_pos` and `lerp_color`
already; the missing pieces are the phase lock and the truecolor capability branch.

**6. Shimmer phase restarts only when the text changes.**
```rust
// codex-rs/tui/src/status_indicator_widget.rs
pub(crate) fn update_header(&mut self, header: String) {
    if self.header != header { self.header = header; self.header_started_at = Instant::now(); }
}
```
with an explicit test, `changed_summary_restarts_shimmer_but_repeated_summary_keeps_phase`. A status
line that updates every second and re-triggers its own animation on each update looks like a seizure;
this one-line guard is the difference.

**7. Leader key + which-key.**
OpenCode's model: one leader (`ctrl+x`), then `<leader>t` theme, `<leader>l` sessions, `<leader>m`
model, `<leader>n` new session, `<leader>a` agents, `<leader>c` compact, `<leader>e` external editor —
`packages/tui/src/config/keybind.ts:38-130`. Every binding is a schema-validated, user-overridable
definition with a `description`; `"none"` unbinds. The which-key overlay
(`feature-plugins/system/which-key.tsx`) shows grouped, scrollable key hints with dock/overlay layouts
and a 0.3-height-ratio panel clamped to 8–16 rows.

niki already has `keybindings.rs` (20 KB) with ids and descriptions
(`src/display/keybindings.rs:249-268`) — the data model is *already right*; it is missing the leader
prefix resolution and the transient hint popup. This is a low-effort, high-"cool" win.

**8. Make the command palette searchable and fuzzy.**
niki's `command_palette.rs` is a fixed list with a cursor. OpenCode uses `fuzzysort` with a
**double-scored** query — title *and* description — and a custom `scoreFn`
(`packages/tui/src/ui/dialog-select.tsx:165-168`), then prepends a **"Suggested"** category so the two
or three most likely commands are always one Enter away
(`component/command-palette.tsx:24,66-73`). niki already depends on `nucleo`; the palette just needs
to grow a query field and reuse `autocomplete.rs:82-115`'s scoring.

**9. Replace-in-place progress rows.**
Claude Code's fix, verbatim: *"per-second progress ticks now replace their predecessor instead of
piling up in the transcript"* (changelog:1599). Codex achieves it structurally by comparing the
candidate cell against the active one before swapping it in:
```rust
// codex-rs/tui/src/chatwidget/streaming.rs:628-637
if self.transcript.active_cell.as_ref().and_then(|a| a.as_any()
        .downcast_ref::<StreamingAgentTailCell>()).is_some_and(|a| a == &cell) { return false; }
self.transcript.active_cell = Some(Box::new(cell));
self.bump_active_cell_revision();
```
For niki this is the highest-value fix during long runs: a 20-minute pipeline with a per-second
elapsed tick appends 1 200 rows today. Stage cards must be keyed by stage id and updated in place.

**10. Terminal-aware resize reflow caps.**
`codex-rs/tui/src/resize_reflow_cap.rs:19-25`: VS Code 1 000 rows, Windows Terminal 9 001, WezTerm
3 500, Alacritty 10 000, everything else a conservative default. Rationale in the docstring is the
right one: *"Replaying more rows than the terminal retains wastes work and can make interactive resize
feel worse without giving the user more usable history."* Also `transcript_reflow.rs` exists to rebuild
the transcript from **source** on resize rather than from cached cell positions.

### Tier 2 — high value, more work.

**11. Animated empty state from an SDF-rasterized vector.** Codex's "blossom"
(`codex-rs/tui/src/empty_state_animation.rs`) embeds SVG paths for the Codex and OpenAI marks, bakes
them into a 160-column signed distance field at startup with cutouts preserved
(`empty_state_animation/geometry.rs:1-12`), then samples the field per cell on a **50 ms** frame clock
(`empty_state_animation.rs:34`) with a 10.8 s sequence that settles into a final full-colour pose, a
fade-out, greetings picked from a 40-line list (`empty_state_animation/greetings.rs:19+`), and
click-to-replay. The single most important line for niki is the *clock* discipline: **"Visible time
pauses while hidden"** and `pause_clock()` is called from the render path
(`app.rs`, `render_chat_widget_frame`). niki already has a fixed-point ASCII logo (`logo.rs`) — it
could get the same treatment at a fraction of the fidelity, or ship a branded spinner instead.

**12. OS-level reduced-motion detection.** `system_motion.rs` reads macOS
`accessibilityDisplayShouldReduceMotion`, Windows `SPI_GETCLIENTAREAANIMATION`, and the Linux
`org.freedesktop.portal.Desktop` `reduced-motion` setting — the Linux probe is wrapped in a **250 ms
timeout** so a missing session bus cannot delay startup, and an unavailable preference preserves
configured behaviour. niki has `NIKI_REDUCED_MOTION` (`motion.rs:17-19`); the portal read is ~25 lines
and the timeout is the part people get wrong.

**13. Status row that owns its own cadence and never wraps.** Codex's status indicator renders on one
line: `● Working (12s • esc to interrupt) · 1 background terminal running`. Details wrap under a dim
`"  └ "` prefix, capped at 3 lines with an ellipsis, and hook status moves to its own line on overflow
*rather than displacing the controls* — verified by the snapshot test
`hook_status_reflows_without_displacing_controls_or_details`. The docstring on `update_inline_message`
is the design rule: *"Passing verbose status prose here can cause frequent width truncation and hide
the more important elapsed/interrupt hint."* Claude Code hit the same bug (changelog:1102: "Fixed
spinner wrapping onto several lines when the current task's label is long").

**14. Per-tool compact-display allowlist.** Gemini's `COMPACT_OUTPUT_ALLOWLIST`
(`packages/cli/src/ui/components/messages/ToolGroupMessage.tsx:46-55`) lists exactly the 9 tools that
get a one-line compact form (edit, glob, read, ls, grep, web_fetch, write, web_search,
read_many_files) and shows everything else in full. It is 10 lines of code and it is the right
default: you cannot hand-tune 200 tools, and you do not need to — the high-signal ones are known.

**15. Output caps as first-class constants.** Kimi keeps them in one file with rationale comments
(`apps/kimi-code/src/tui/constant/rendering.ts`): `RESULT_PREVIEW_LINES=3`,
`SHELL_OUTPUT_PREVIEW_LINES=10`, `THINKING_PREVIEW_LINES=2`, `OUTCOME_MAX_LINES=3`,
`OUTCOME_GLANCE_SAMPLES=3`, `RETRY_DETAIL_MAX_CHARS=160` ("so huge provider error bodies — occasionally
whole HTML error pages — can't flood the activity pane"), plus memory caps
(`MAX_SUBAGENT_ACTIVITY_STEPS=20`, `SUBAGENT_TOOL_OUTPUT_MAX_CHARS=8000`). Note
`MAX_FINAL_OUTPUT_LABEL_CHARS` with the comment: *"Display width alone does not bound memory — ANSI
sequences and zero-width graphemes add unbounded code units within a single column."* niki's
`tool_card.rs` / `tool_detail.rs` should adopt this discipline.

### Tier 3 — polish.

- **OSC 133 semantic zones.** Kimi prefixes `OSC 133 A/B/C` markers to the first/last line of each
  transcript message so FinalTerm-style shell integration can show *which prompt a block belongs to*
  (`constant/rendering.ts:4-8`). Cheap in niki, genuinely delightful for anyone who uses shell
  integration.
- **Non-blocking worktips.** OpenCode: the tip only renders if `visibleWidth(tip) <= availableWidth`
  (`startup-loading.tsx` shows the same discipline for the loading pill — show nothing until 500 ms,
  keep it for a minimum of 3 s). Kimi: same, in `MoonLoader.updateDisplay()`.
- **Animate the trust/onboarding prompt too, but strip control characters from workspace-supplied
  text.** Kimi's `TrustPromptComponent` calls `sanitizeForDisplay()` which drops every C0/C1 code
  point, because *"the trust prompt renders before the workspace is trusted, so a planted `.mcp.json`
  must not inject terminal control sequences into it"* (`dialogs/trust-prompt.ts`). niki reads repo
  content before approval too.
- **A signature motion.** Kimi's spinner is a **moon phase** — `['🌑','🌒','🌓','🌔','🌕','🌖','🌗','🌘']`
  at 120 ms, with a braille fallback at 80 ms (`constant/rendering.ts`, last lines). OpenCode's welcome
  header and model pill can "rainbow dance". A rotating moon phase in niki's status row is ~10 lines
  and is the single cheapest "wait, that's custom" signal in this whole report.

---

## 3. The perceived-speed problem

### What actually makes a terminal app feel slow

Ranked by how often it is the real culprit, with the evidence:

1. **Redraw storms.** Every widget independently deciding "I changed, redraw!" produces N redraws
   per logical update. Codex's fix is architectural: nothing draws directly. Widgets send an `Instant`
   to a scheduler task; the scheduler collapses them and emits **one** `()` on a broadcast channel
   (`tui/frame_requester.rs:96-127`). The docstring says the point is "keeping animations and status
   updates smooth without redrawing more often than necessary."
2. **Redrawing more often than a human can see.** `frame_rate_limiter.rs:1-8`: *"Widgets sometimes call
   `FrameRequester::schedule_frame()` more frequently than a user can perceive. This limiter clamps
   draw notifications to a maximum of 120 FPS to avoid wasted work."* Implementation is 37 lines:
   a `last_emitted_at: Option<Instant>`, `clamp_deadline()` pushes a requested deadline forward to
   `last + 8_333_334ns`, `mark_emitted()` records it. Note the subtlety proven by
   `test_late_draw_still_limits_the_next_frame`: **a late draw still clamps the next one** — a
   deadline that arrives overdue does not buy you a free burst.
3. **Re-processing the whole conversation on every update.** This is Claude Code's most-repeated perf
   bug, and it is the single most instructive item in their changelog:
   - "hook progress and sub-agent activity no longer re-process the whole conversation on every update" (819)
   - "transcript updates no longer re-process the whole conversation to build the collapsed tool-use summaries" (1023)
   - "cutting redundant UI re-renders" (1630)
   - "the panel no longer re-parses the whole reply on every update" (VSCode, 168)
   - "reducing per-keystroke rendering work" (1543)
   The invariant: **O(delta), not O(transcript), per update.** niki's `state.rs` is 69 KB and
   `pages/chat.rs` is 94 KB — worth auditing for any `messages.iter().map(...)` in a hot path.
4. **Keystroke latency competing with animation frames.** "keystrokes no longer occasionally wait a
   frame behind spinner or streaming repaints" (1204). The fix is to keep the frame budget such that
   a frame never costs more than a fraction of the input poll interval, and — Codex's structural
   answer — to give the composer a fast path that does not recompute the transcript.
5. **Unbounded history replay on resize.** Covered in §2 item 10; the failure mode is a multi-second
   freeze, which reads to the user as a crash.
6. **Repaint granularity.** "adding or removing a prompt line (Shift+Enter) now repaints as fast as
   typing a character **instead of re-rendering the visible transcript**" (1116). That is a dirty-*region*
   win, not a dirty-*flag* win. niki has the flag; the region is next.
7. **Backpressure against a slow terminal.** "output no longer falls further behind while the terminal
   catches up" (631). Related: "held Backspace being ignored on terminals that send Ctrl+H … when
   keystrokes arrive in large bursts (slow SSH/mosh links)" (1964). If the PTY write blocks, the render
   loop must not be the thing that keeps appending to a queue.
8. **Unbuffered / unsynchronized writes.** Codex wraps the *entire* draw in
   `stdout().sync_update(|_| { … })` — crossterm's `SynchronizedUpdate`, i.e. **DEC private mode 2026**
   (`tui.rs:1261-1284`, `use crossterm::SynchronizedUpdate` at `tui.rs:20`). niki does this correctly
   (`src/display/tui.rs:288-296`).
9. **Foreground-blocked startup.** "sessions on slow or heavily loaded machines sometimes exiting
   with 'unrecoverable interface error' when the first spinner appeared" (514), and "a blank screen
   flashing before the first frame" (130). niki's initial draw paints an empty `Paragraph` before the
   first real frame (`src/display/tui.rs:216-226`) — that is the blank-flash pattern Claude Code fixed.

**Blocking IO in the render path** is the one item on this list I found *no* evidence of any of these
projects getting wrong — and niki explicitly already guards it: *"Throttled: the mission-store
round-trip is pure overhead at 30-60fps; the grid only needs ~2Hz freshness in the loop"*
(`src/display/tui.rs:299-300`). Keep that comment; it is the right instinct.

### The frame budget, concretely

| Cadence | Codex | niki | Note |
|---|---|---|---|
| Commit tick while streaming | 8.33 ms (`app.rs:446` = `tui::TARGET_FRAME_INTERVAL` = `frame_rate_limiter::MIN_FRAME_INTERVAL`; `tui.rs:87`) | 16 ms | 60 fps is fine; the *pacer* matters more than the rate |
| Animated status row | 32 ms, self-armed (`status_indicator_widget.rs`) | 16 ms global tick | 32 ms = 31 fps for a shimmer is plenty and halves the work |
| Idle elapsed counter | 1 000 ms, self-armed (same file) | 33 ms global tick | **30× too often today** |
| Empty-state blossom | 50 ms (`empty_state_animation.rs:34`) | n/a | |
| Spinner (OpenCode) | 80 ms braille (`component/spinner.tsx:11`) | — | |
| Spinner (Kimi) | 80 ms braille / 120 ms moon (`constant/rendering.ts`) | — | |
| Max draw rate | 120 fps hard cap | none | |

---

## 4. First-run / onboarding: the 30-second experience

**Claude Code.** `claude` in an empty repo → login prompt (OAuth or `ANTHROPIC_API_KEY`, which "skips
the login prompt and asks you to approve the key instead" — docs/overview) → **trust dialog** that
names the repository root the grant covers (changelog:2455) and lists the project MCP servers being
enabled → welcome splash art, which they had to fix twice: "the welcome splash art overflowing the
default 80×24 macOS Terminal window" (3080), and "the welcome banner keeping its old panel widths
after a combined width+height terminal resize" (2614). Working state is shown in the footer, including
"Not logged in · Run `/login`" as a first-class status (241). `Ctrl+L` is documented as a redraw
escape hatch for when the display "becomes garbled or partially blank".

**OpenCode.** Logo + prompt, with **rotating placeholders** that teach the product:
`["Fix a TODO in the codebase", "What is the tech stack of this project?", "Fix broken tests"]`, and
shell-mode variants `["ls -la", "git status", "pwd"]` (`routes/home.tsx:15-19`). Home footer carries a
tips view. `npx`/npm/curl install; a `SIGUSR2` reloads themes without restart.

**Codex TUI.** `codex` → **"Sign in with ChatGPT"** as the recommended path (README) → the animated
blossom with a randomized greeting from a 40-line list → typed input. Ten `.tmTheme` syntax themes
available from the first prompt.

**Kimi Code CLI.** `kimi` → **bordered welcome panel** with logo, Directory, Session, Model, Version,
MCP summary; if logged out the model line reads `not set, run /login or /provider` in the *warning*
colour (`chrome/welcome.ts:47-77`). Below 24 columns it degrades to a 4-line plain block rather than
clipping. Then `run /login inside the CLI` and choose OAuth **or** an API key — the CLI never
auto-opens a browser on first paint. **"Don't trust" on the trust dialog exits the process**
(`dialogs/trust-prompt.ts:34-38`) and is asked again next launch. Install is
`curl -fsSL https://code.kimi.com/kimi-code/install.sh | bash`, marketed as
**"Single-binary distribution… no Node.js setup"** and **"Blazing-fast startup. The TUI is ready in
milliseconds"** (README). That startup claim is a headline feature, not a footnote.

**Gemini CLI.** `npx @google/gemini-cli` with zero install is the *first* Quick Install option in the
README — the fastest possible time-to-first-prompt of the five. Then an onboarding checklist and
`/auth` selection across Gemini API key, Vertex AI, or Google OAuth.

**Common shape, and the lesson for niki:** every one of them answers the same four questions in the
first 30 seconds — *am I logged in, what project am I in, what model/branch is active, and what do I
type next?* And four of five make the answer to the last one **typeable examples or a keyboard hint
strip**, not a paragraph of prose.

---

## 5. Demo-ability: what's in the hero, and what niki should do

### What each project actually ships

| Project | Hero asset | Type | Position |
|---|---|---|---|
| Claude Code | `./demo.gif` | terminal recording | line 12, above "Get started" |
| Kimi Code CLI | `./docs/media/intro.gif` | terminal recording | line 7, **above the "What is" section** |
| Codex | `.github/codex-cli-splash.png` at 80 % width, centred | static screenshot | line 3, first thing in the README |
| OpenCode | `packages/web/src/assets/lander/screenshot.png` | static screenshot | line 44, links to opencode.ai |
| Gemini CLI | `/docs/assets/gemini-screenshot.png` | static screenshot | line 11 |

**Split decision: 2 GIFs, 3 PNGs.** The two that ship a GIF are the two whose *motion* is the product.
Kimi's GIF is above the fold because its moon-phase spinner and the rainbow-dance welcome are what
people screenshot. Claude Code's GIF shows the tool-call rows appearing and a diff landing. The three
that ship a PNG have a static visual identity worth showing precisely because it doesn't animate.

**niki already has the harder asset.** `assets/demo.gif`, `assets/demo.mp4`,
`assets/demo-real-chat.gif`, and a deterministic PIL renderer (`scripts/render_demo_cinematic.py`,
1280×840 @ 10 fps, 800 frames) whose stated purpose is *"zero terminal capture => zero flicker"*
(`demo.tape` header). That is a better pipeline than any of the five have. The gap is **curation, not
production**.

What a pipeline demo must show that a chat-loop demo cannot:

1. **The fan-out.** niki's differentiator is Planner/Coder/Tester/Reviewer running in isolated
   sandboxes. A demo that shows one chat transcript looks like Claude Code with extra steps. The demo
   must show **N stage lanes running concurrently**, with the pipeline header advancing
   Planner→Coder→Tester→Reviewer. This is the shot nobody else can take.
2. **The adversarial turn.** Show a Tester **failure** and the Coder being sent back — an
   unsanitized red/blue diff. A demo where everything passes first try proves nothing and looks fake.
   (`demo.tape` already scripts "Bash-fail".)
3. **The handoff, not the process.** The money frame is `niki/a7f3c2` — a real branch, a real diff,
   your tree untouched. Cut to it early. Do not make the viewer watch 90 s of stages to get there.
4. **The permission modal.** It is the one thing that signals "this thing is *in* your repo." Already
   in the tape.
5. **Zero-config proof.** The single highest-leverage GIF frame for a solo OSS project is
   `curl … | sh` → first prompt in under N seconds. Kimi sells exactly this in prose
   ("Blazing-fast startup"); niki should sell it in a 3-second terminal capture. niki's install story
   is already the strongest of the five in kind (homebrew + scoop + winget + `install.sh`, see
   `homebrew/niki.rb`, `scoop/niki.json`, `winget/RavaniRoshan.niki.yaml`).

Shape: keep the 80 s cinematic MP4 for the README hero, cut a **6–8 s silent loop** (moon-phase
spinner → three lanes lighting up → diff → branch name) for the social card, and put a **static
screenshot** in the docs site where a PNG loads faster. Match Kimi's placement: hero above the
"what it is" text.

---

## 6. Honest assessment

**Where niki structurally cannot match, and shouldn't try to.**

- **Streaming token feel.** Codex's streaming subsystem is 78 KB in `streaming/controller.rs` alone,
  plus `chunking.rs`, `commit_tick.rs`, `table_holdback.rs`, `code_fence.rs`, `markdown_stream.rs`,
  `markdown_render/streaming.rs`. It handles mid-token width changes, unclosed code fences, partially
  streamed markdown tables, and re-render-from-source on resize. niki produces **bounded, structured
  stage output**, not a token firehose. The honest conclusion: niki should build the **pacer and the
  dirty-region discipline** (§2 items 2, 6, 9) and skip the fence/table holdback machinery entirely.
  A pipeline that emits 4 clean stage blocks never needs it.
- **The animated empty state.** Codex bakes SVG paths into a 160×N SDF and samples per cell at
  50 ms. That is a real graphics project. niki's pragmatic 80 % version: a branded spinner
  (moon-phase, ~10 lines), a rotating greeting line, and the existing ASCII logo — static, settled on
  first paint.
- **The composer.** Codex's `chat_composer.rs` is **511 919 bytes**; `textarea.rs` is 171 KB. They
  support 10 000-character prompts, custom vim remaps, cross-session history search, external-editor
  round-trips and rewind. niki should implement **vim normal-mode verbs** on a simple editor
  (`h j k l w b 0 $ gg G x dd dw ci cw yy p P > < u .`) and stop there. That is 300 lines and it is
  the single most-copied feature in the category.
- **Vim *parity*.** Claude Code's changelog shows vim bugs in 20+ consecutive releases (changelog
  lines 48-50, 148-151, 271-272, 599) including `dj`/`dk` acting on part of a line and `cw` on a
  one-letter word changing the next word. Full Vim semantics is a maintenance treadmill. Ship
  normal-mode verbs; do not claim Vim.
- **Terminal compatibility breadth.** Claude Code has a `/terminal-setup` command, kitty keyboard
  probing, iTerm2/Ghostty/ConEmu progress OSC, Warp hyperlink bugs, rxvt-unicode cursor leaks, VS
  Code detection, and per-terminal resize-reflow caps. niki has the Kitty I4 protocol and DEC 2026
  already. Ship those two and be explicit in the README about what you test.
- **The scale of the polish surface.** Codex's `bottom_pane/` is 505 files, `chatwidget/` is 573.
  Chasing parity file-for-file is a multi-year project and would consume the team.

**The pragmatic path to "excellent anyway."** Four things, in order:

1. **Fix the two things a user notices in the first 10 seconds.** niki's loop polls at 33 ms
   forever (`src/display/tui.rs:282,349`) and the command palette is a cursor you cannot type into
   (`src/display/command_palette.rs:143,264`). Convert to a `FrameRequester` with a 120 fps limiter
   and give the palette a query field backed by the `nucleo` scorer already in `autocomplete.rs:82`.
   Everything below is optional; these two are not.
2. **Make long runs not accumulate scrollback.** In-place stage cards + paced commits (§2 items 2, 9).
   This is the difference between "impressive GIF" and "I'd actually keep this open for 40 minutes."
3. **Spend the effort on the two signature things.** A branded spinner and a real footer collapse
   ladder. Nobody screenshots a footer; everybody screenshots a spinner, and a footer that degrades
   gracefully at 60 columns is what makes a resize demo look engineered.
4. **Then the leader key + which-key.** It is the cheapest path to "this feels like a real tool,"
   the data model already exists in `keybindings.rs`, and it demos beautifully on camera: press
   `ctrl+x`, the hints fan out, the operator picks one without the mouse.

The uncomfortable truth in this whole report is that the visible 5 % of these codebases — the
spinner, the shimmer, the blossom, the which-key — is what gets screenshotted, and it is roughly 3 %
of the engineering. The other 97 % is the frame limiter, the coalescing scheduler, the dirty-region
repaint and the O(delta) update path, and it is 100 % invisible until it is missing. niki has already
done a real fraction of that 97 %. It has not done any of the 5 % yet. That is the actual gap, and it
is a much cheaper one to close than it looks.
