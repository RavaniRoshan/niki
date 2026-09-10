# Pi TUI → Niki extraction plan

Status: ALL PHASES IMPLEMENTED on `goal/pi-tui-extraction-c81d04` (12 commits).
Only documented follow-ups remain: live OSC-11 query round-trip (needs
event-loop integration), `/model` overlay integration, async completion
provider v2 (unneeded — sync path is fast). VHS gate 12/12 green.
Sources: Pi `packages/tui/src` (cloned 2026-09-09, 134 files) vs Niki `src/display` at `master`.
Prior art: `docs/ui/ui-audit.md` is stale (2026-08-16, pre-unification) — superseded by §2 below.

## 0. Implementation record (goal c81d04)

| Task | Commit | Evidence |
|---|---|---|
| TUI-001 unicode cursor | a8849ff | 10 regression tests; fixed 1-col click offset |
| TUI-002 fleet throttle | a8849ff | 500ms loop throttle; deterministic tests |
| TUI-000 bench harness | 860dbcd, b28b8d4 | tests/tui_perf.rs, 7 scenarios |
| TUI-00D debug surface | d194cf1 | display::debug, Cost frame stats |
| TUI-003 keybindings | 2b388df | display::keybindings, generated help, example toml |
| TUI-004 render memo | be230f5 | 47ms → ~0ms memoized rebuilds |
| TUI-010 scroll | 8fb0712 | ScrollState, chaining, detail modal paint |
| TUI-011 search | bf2488c | display::search, badge, reveal |
| TUI-012 completions | c43f949 | nucleo @-files, Tab apply, /model args |
| TUI-013 overlays + TUI-014 matrix | c5be350 | shared geometry, docs/tui/key-matrix.md |
| TUI-020/023 hardening + polish | db3bd7d | byte-slice fixes, command truncation |
| TUI-021 caps + TUI-022 diff memo | 35b8a9c | display::caps, OSC 8 links, 36ms → 12ms diff |
| TUI-030/031/032/040/041 | 12399d1 | display::mouse, reflow tests, tables, VHS 12/12 |

Deviations from the original plan: TUI-013 implemented as shared-geometry
fixes (menu_rect, option_first_row, modal click routing) instead of a
RefCell bounds registry — Niki overlays are modal and already share rect
fns, so a registry added machinery without a consumer. TUI-021 live OSC-11
round-trip deferred (parsers + matrix shipped; the query needs crossterm
event-loop integration). TUI-042 skipped (no async completion source
exists; sync path measures fast). Beyond-plan fixes the work uncovered:
chat follow was broken (auto_scroll write-only), ChatPage selection math
used bottom-anchored offsets, permission clicks were off by 2+ rows,
permission modal clipped options with description/detail, @-autocomplete
selection was inert, tool-detail modal never painted, help overlay showed
14 static rows (now 16 generated).

## 1. Executive assessment

Niki's TUI is already past the architecture Pi would suggest: single canonical
`AppState`, dirty-flag engine on top of ratatui (which diffs cells internally),
CSI-2026 framing, kitty keyboard, bracketed paste, mouse routing, permission
modal, command palette, fuzzy slash menu, streaming-markdown fence handling.
There is no wholesale extraction to do. Value concentrates in ~10 targeted
adaptations: input-correctness (byte-index cursor bugs are real crashes),
scroll/search primitives, a central keybinding table, capability queries for
auto-theme, render memoization for large transcripts, and benchmark/debug
infrastructure. Pi's native modules (clipboard `.node`, images, LaTeX) and its
main-screen renderer must NOT be copied — details in §10.

## 2. Current Niki TUI architecture (verified from source)

- State: `src/display/state.rs` — `AppState` (single source of truth),
  `InputState` (buffer/cursor/history/undo/kill-ring/yank), `PageId` (14
  variants), `DisplayEvent` (~20 variants incl. `PermissionRequest`,
  `SteerChannel`, `ToolCall`/`ToolResult`). `src/display/pages/mod.rs`
  re-exports canonical types; `PageRouter` delegates render/key handling.
- Engine: `src/display/engine.rs` — `RenderEngine` (dirty flag, `FrameTarget`
  High 60fps / Low 30fps, `FrameStats` ring). No parallel cell buffer by design
  (ratatui `Terminal::draw` diffs internally). CSI-2026 wrapped per frame in
  `src/display/tui.rs:285-306`; capability sniff in `tui.rs:1065-1089`.
- Input: `src/display/input.rs` `InputHandler` (Insert/Command/Shell modes);
  `tui.rs` ~700-line dispatch × 2 loops (`run_tui`, `run_chat`). Focus via
  `active_focus()` (`tui.rs:130`) + `FocusState` (`components/list_cursor.rs`).
  Overlays: permission modal, command palette, slash menu, confirm modal, help,
  onboarding, tool detail — each with own `click_index`/`cursor` helpers.
- Content: `display/chat/` (pulldown-cmark renderer + `streaming.rs`
  partial-fence handling), `components/tool_card.rs`, `pages/diff.rs`
  (Codex-style renderer), `pages/chat.rs:1558` content-hash skip for chat lines,
  `layout/mod.rs` chat/page layouts + scrollbar, `theme.rs` (NO_COLOR,
  COLORTERM truecolor, truncate helpers via `unicode-truncate`).
- Terminal: `kitty.rs` (enable-only + `decode_csi_u`), `ime.rs` (DSR cursor
  query for anchoring), OSC-52 copy with tmux passthrough
  (`pages/chat.rs:1594`), mouse capture toggle (Ctrl+E), per-overlay hit tests,
  scrollbar drag, tab/status-bar clicks.
- Gaps confirmed: byte-index cursor ops panic on multi-byte input
  (`state.rs:321-417`, `input.rs:265-275`); `refresh_fleet()` block_ons tokio
  locks every frame (`tui.rs:291,1146`); scrollback search absent (only Ctrl+R
  history search); no central keybinding table; @-file completion is
  prefix-only (`components/autocomplete.rs:79`) while slash menu uses nucleo
  fuzzy (`command_menu.rs:24`); auto-theme has no live query; no benchmarks.

## 3. Pi extraction map (condensed; full table in §3.1)

Take/adapt: frame-scheduling discipline (input fast path), per-component
(width,text) memoization, ScrollView+chaining, scrollback search (simplified),
overlay bounds registry, keybinding manager (user-layer conflicts only),
grapheme/segmenter word nav, kill/undo `lastAction` coalescing audit, async
completion serialization, OSC-11 background + scheme queries, hyperlink gating,
transcript/frame benchmarks, write-log debug flag, hover-no-select lists,
scroll-to-end indicator, focusTarget on mouse dispatch.
Keep Niki's: ratatui cell diff, alt-screen-only model, crossterm key decoding,
APC-marker-free IME (DSR query), notice system, spinner, OSC-52 clipboard.
Skip: main-screen renderer, kitty/iTerm2 images, native clipboard modules,
LaTeX, OSC-133 zones, custom cell buffer, second render thread.

### 3.1 Feature-by-feature comparison

| Pi capability | Pi approach | Niki equivalent | Niki location | Gap | Value | Diff | Priority | Verdict | Reason |
|---|---|---|---|---|---|---|---|---|---|
| Coalesced frame scheduling + immediate render on input | `requestRender` 16ms throttle, `requestImmediateRender` for keys (`tui.ts:952-1004`) | Dirty flag + High/Low targets, 16ms poll | `display/engine.rs`, `display/tui.rs:268-312` | Exists, improve: no input fast path | Med | S | P1 | Adapt | Draw synchronously on key events instead of waiting for next tick |
| Per-component memoization | `(width,text)` caches + per-frame render cache (`text.ts`, `box.ts`, `layout.ts:68-81`) | Content-hash skip for chat lines only (`pages/chat.rs:1558-1565`) | `display/chat/`, `display/components/` | Medium: components re-render every frame | High on large transcripts | M | P1 | Adapt | Cache message/markdown renders keyed (width, hash) |
| Differential flush | Line-range (main) / per-row (alt) writes | Ratatui cell diff inside `Terminal::draw` | `display/engine.rs:1-11` (deliberate) | None | — | — | — | Skip | Reimplementing duplicates ratatui |
| Main-screen renderer | Streams into scrollback | Alt-screen only | `display/tui.rs:182-195` | None (by design) | — | — | — | Skip | Persistence covers resume; scrollback renderer adds complexity |
| ScrollView + chaining + transient scrollbar | `scroll-view.ts`, remainder protocol, `routeWheel` | Manual `scroll_offset`, static scrollbar | `display/layout/mod.rs:74-86`, `display/state.rs` | Missing structure | High | M | P1 | Adapt | `ScrollState` struct + innermost-first chaining; keep ratatui widgets |
| Search in scrollback | ANSI-stripped corpus + index + reveal (`alt-screen-search.ts`) | Ctrl+R history search only | `display/pages/chat.rs:946` | Missing | High for long sessions | L | P1 | Adapt | Plain-text corpus over `chat_lines`; skip grapheme-span mapping v1 |
| Overlay compositor | Anchor/size/focusOrder/hit bounds (`tui.ts:387-416`) | Ad-hoc overlays + `active_focus` chain | `display/tui.rs:130`, overlays listed §2 | Exists, improve | Med | M | P1 | Adapt | Bounds registry reusing `FocusState`; no new focus stack |
| Mouse system | Motion modes, capture protocol, click counting, drag-select+copy | Capture toggle, per-overlay `click_index`, scrollbar drag, OSC-52 copy | `display/tui.rs:624-1014`, `pages/chat.rs:1594` | Exists, improve | Med | S/M | P2 | Adapt | Hover-no-select, multiplexer-aware motion, release filtering |
| Key decoding | ESC timeouts (10/50/100ms SSH), Kitty negotiation + modifyOtherKeys fallback, layout guards | Crossterm decoding + kitty enable-only | `display/kitty.rs` | Mostly covered by dep | Low-Med | S | P2 | Mostly skip | Add ambiguity doc + manual key test; no custom stdin buffer |
| Keybinding manager | Flat semantic IDs, user-override conflict detection (`keybindings.ts`) | Scattered matches | `display/tui.rs`, `pages/chat.rs`, `help_overlay.rs` | Missing | High leverage | M | P1 | Adapt | Central table + `niki.toml` overrides; contexts stay receiver-side |
| Editor core | Grapheme-safe ops, kill ring, undo coalescing, segmenter word nav, paste markers | Same concepts, byte-index bugs | `display/state.rs:286-594`, `display/input.rs:277-328` | Correctness bug | Critical | S | P0 | Adapt+fix | Batch 1; skip paste markers (burst window suffices) |
| Autocomplete | Provider contract, serialized async, fd/file/slash/path sources | Prefix-only @ files; nucleo fuzzy slash menu | `components/autocomplete.rs:79`, `command_menu.rs:24` | Exists, improve | High | M | P1 | Adapt | Nucleo fuzzy for @ files + arg completions; async trait later |
| Markdown | Lexer + hand renderer, partial-fence + pending-latex hacks, width-aware tables | pulldown-cmark + same fence hack (`streaming.rs:20`) | `display/chat/` | Converged; tables weaker | Med | S/M | P2 | Adapt | Width-aware tables, OSC-8 links gated on capability |
| Capability detection | Env sniff + OSC-11 bg + 2031 scheme + cell-size queries | Env sniff only (NO_COLOR/COLORTERM/kitty/sync) | `display/theme.rs:108-145`, `kitty.rs`, `tui.rs:1065` | Missing live queries | High (auto-theme correctness) | M | P1 | Adapt | OSC-11 bg + scheme query; skip images/sixel |
| Terminal images | Kitty/iTerm2 encode + LRU + crop | None | — | Not appropriate | Low | XL | — | Skip | Agent sessions don't need images; heavy protocol surface |
| Native clipboard | `.node` prebuilds per OS | OSC-52 + tmux passthrough | `pages/chat.rs:1594` | None that matters | Low | XL | — | Skip | OSC-52 suffices; native modules alien to `cargo dist` |
| LaTeX | Pure-TS Unicode transpiler | None | — | Niche | Low | L | — | Skip | Rare in coding sessions |
| IME anchoring | Zero-width APC cursor marker | DSR position query | `display/ime.rs` | Different mechanism, same goal | — | — | — | Keep Niki's | No change |
| Bracketed paste | Enable + re-wrap + `paste` event | Enable + `Event::Paste` + burst window | `display/tui.rs:186-190,1021` | None | — | — | — | Keep | No change |
| Benchmarks | NullTerminal churn + 2500-component transcript bench | None | — | Missing | High (guards perf work) | M | P1 | Adapt | Headless render harness measuring frames/scenario, no new deps |
| Debug observability | Write log, redraw reasons, counters | `FrameStats` unexposed | `display/engine.rs:19-100` | Exists, improve | Med | S | P1 | Adapt | `NIKI_TUI_DEBUG` + surface `FrameStats` |
| Selection lists | Centered window, hover-no-select | Palette/menu select on hover paths | `command_palette.rs`, `command_menu.rs` | Polish | Low-Med | S | P2 | Adapt | Hover must not move selection |
| Flash/scroll-end affordances | Flash container, scroll-to-end indicator | `set_notice` TTL system | `display/state.rs:1047` | Equivalent | Low | S | P2 | Keep + minor | Add scroll-end indicator only |
| OSC-133 zones / title / progress | Prompt markers, `setTitle`, OSC 9;4 | None | — | Niche | Low | S | P3 | Skip | Optional later |

## 4. Target architecture (Niki-native)

```
pipeline/orchestrator events → DisplayEvent (existing, extend)
→ AppState + UI stores (existing; + ScrollState, KeyBindings, Capabilities)
→ components (existing modules; pure render fns over &AppState)
→ ratatui layout (existing; + overlay bounds registry)
→ RenderEngine → ratatui Terminal (cell diff) → crossterm backend
```

New traits (small, composable; no Pi class hierarchy):
- `Scrollable`: `scroll_by/lines/content_height/viewport_height` — implemented by
  chat view, tool detail, pages; enables chaining without a widget framework.
- `Searchable`: `search_corpus() -> Vec<String>` + `reveal(match)` — chat first.
- `CompletionProvider`: `trigger_chars`, `suggest(prefix) -> Vec<Item>`,
  `apply(...)` — sync v1 (nucleo ranking), async serialization v2.
- `CapabilityProbe`: `query_background()`, `query_scheme()` — async one-shot at
  startup; results feed `theme::set_mode`.
- `OverlayBounds`: every overlay reports `Rect` after render for hit-testing;
  replaces per-overlay coordinate math over time.
Testing: existing unit tests + headless render harness (new `tests/tui_bench/`
or `#[bench]`-style harness without criterion) + VHS tapes for new states only.

## 5. Roadmap (dependency-aware)

### Phase 0 — Baseline + instrumentation
- TUI-000 Bench harness (headless frames: initial / 5k-line transcript /
  streaming 200 tokens / resize ×10). Files: `tests/tui_perf.rs` (new),
  `src/display/pages/chat.rs`. Deps: none. M. Arch. Accept: prints ms/frame per
  scenario; CI asserts no-panic + budget vs recorded baseline. Tests: harness
  itself.
- TUI-00D Debug surface (`NIKI_TUI_DEBUG`: raw-write log + full-redraw
  reasons; expose `FrameStats` via status/chat debug). Files: `engine.rs`,
  `tui.rs`. S. Accept: env-gated logs, zero overhead when off.

### Phase 1 — Rendering + input foundation
- TUI-001 Unicode-safe `InputState` cursor [IN PROGRESS]. Why: panics on
  non-ASCII input. Files: `state.rs:320-417`, `input.rs:264-328`. S. Accept:
  multibyte insert/move/delete/word/yank/line_col no-panic + byte-correct
  cursor; regression tests.
- TUI-002 Fleet refresh off hot path [IN PROGRESS]. Why: per-frame
  `block_on` on tokio locks. Files: `state.rs:1109`, `tui.rs:291,1146`. S.
  Accept: refresh ≤2Hz in loops, immediate on nav; deterministic throttle tests.
- TUI-003 Keybinding table. Why: unlocks every later input change; scattered
  matches unmaintainable. Files: new `display/keybindings.rs`, `tui.rs`,
  `pages/chat.rs`, `help_overlay.rs`, config docs. M. Deps: none. Accept:
  defaults table, `niki.toml` overrides, conflict report (user-layer only),
  help overlay generated from table. Tests: match/override/conflict units.
- TUI-004 Message render memoization. Why: transcript re-render dominates
  frames. Files: `display/chat/`, `pages/chat.rs`. M. Deps: TUI-000 (proves
  need). Accept: (width,hash) cache; ≥3× faster 5k-line render. Tests: cache
  hit/invalid units + bench delta.

### Phase 2 — Interaction primitives
- TUI-010 `ScrollState` + chaining (chat → detail → page, innermost-first;
  `contain` vs `chain`). Files: new `display/scroll.rs`, `state.rs`,
  `tui.rs` wheel path, `layout/mod.rs`. M. Deps: TUI-003 (wheel bindings).
- TUI-011 Scrollback search panel (plain corpus over `chat_lines`,
  next/prev/wrap, reveal scrolls). Files: new `display/search.rs`,
  `pages/chat.rs`. L. Deps: TUI-010.
- TUI-012 Fuzzy @-files + slash-arg completions via nucleo. Files:
  `components/autocomplete.rs`, `command_menu.rs`. M. Deps: none (parallel ok).
- TUI-013 Overlay bounds registry + focusTarget on mouse dispatch. Files:
  `tui.rs` mouse path, overlays. S/M. Deps: none.
- TUI-014 Key ambiguity doc + manual key matrix test. Files:
  `docs/`, `tests/manual` or script. S. Deps: TUI-003.

### Phase 3 — Agent-native components (justify each; build only what's here)
- TUI-020 `ToolCallView` virtualization (cap rendered output lines w/ expand;
  feeds TUI-004 cache). `components/tool_card.rs`, `pages/chat.rs`. M. Deps:
  TUI-004.
- TUI-021 Capability probes → auto-theme correctness (OSC-11 bg, scheme
  query, hyperlink gating for links/artifacts). `theme.rs`, new
  `display/capabilities.rs`. M. Deps: none (parallel ok).
- TUI-022 Diff viewer large-file budget (windowed hunks, perf target from
  TUI-000). `pages/diff.rs`. M. Deps: TUI-000.
- TUI-023 Approval/permission polish (hover-no-select, detail toggle
  discoverability). `components/permission.rs`. S. Deps: TUI-013.
- Deferred to later demand: AgentCard/Timeline, MCPServerView, SessionSwitcher
  (fleet/session pages cover current needs — do not build speculatively).

### Phase 4 — Advanced terminal
- TUI-030 Multiplexer-aware mouse + selection polish. M. Deps: TUI-013.
- TUI-031 Resize debounce + reflow tests. S.
- TUI-032 Width-aware markdown tables. S/M. Deps: TUI-004.

### Phase 5 — Polish + performance
- TUI-040 Large-transcript budget enforcement (bench deltas per PR). S.
- TUI-041 VHS tapes for search/palette/permission states only. S.
- TUI-042 Async `CompletionProvider` v2 (only if TUI-012 proves too slow). L.

## 6. Top 10 (ranked by impact+frequency+perf+leverage+extensibility − complexity+risk+maintenance)

1. TUI-001 unicode cursor — crash fix on every keystroke path. P0.
2. TUI-003 keybinding table — leverage for all input work; user customization. P0.
3. TUI-004 render memoization — transcript perf dominates long sessions. P1.
4. TUI-000 bench harness — makes 3, 20, 22 measurable; blocks perf claims. P1.
5. TUI-010 scroll chaining — daily mouse UX in nested views. P1.
6. TUI-011 scrollback search — long-session navigation. P1.
7. TUI-021 capability probes — auto-theme wrong today on unknown terms. P1.
8. TUI-012 fuzzy @-files — matches slash-menu quality users already see. P1.
9. TUI-002 fleet throttle — removes per-frame blocking I/O. P0 (batch 1).
10. TUI-020 tool output virtualization — streaming tool spam is the common
    jank source. P1.

## 7. Quick wins (S, low risk)

TUI-00D debug surface; TUI-013 bounds registry slice (record rects first,
route later); TUI-023 permission polish; hover-no-select lists; scroll-to-end
indicator; TUI-031 resize debounce; TUI-014 key matrix doc.

## 8. Deep changes (careful refactoring)

TUI-003 (touches every input path — land with table-generated help as proof);
TUI-010 (wheel/click routing rewrite); TUI-004 (cache invalidation bugs are
subtle — key strictly on (width, content hash), never on time); TUI-011
(corpus indexing on live-updating transcript).

## 9. Performance plan

Measure with TUI-000 harness, all headless (no terminal needed):
- Initial render (empty + typical session fixture).
- 5k-line transcript full render; target: ≤50ms p95 after TUI-004 (record
  baseline first, require ≥3× improvement, no absolute fantasy numbers).
- Streaming: 200 token appends; target: no frame >33ms idle budget.
- Scroll: 200× `scrollBy(1)`; target: O(viewport) work per step (viewport
  windowing, never O(transcript)).
- Resize ×10 widths; target: no panic, bounded recompute (memoization keyed
  on width invalidates once per width).
- Large diff (2k hunks): target renders windowed, input stays responsive.
Success = measured deltas vs checked-in baseline, not adjectives.

## 10. Risks / do NOT copy

- Kitty/iTerm2 image protocols + LRU/transmission bookkeeping: heavy,
  terminal-specific, zero agent-session value.
- Native `.node` clipboard/modifier modules: breaks `cargo dist` hermetic
  builds; OSC-52 + crossterm suffice.
- Main-screen streaming renderer: conflicts with alt-screen model; Niki
  persistence solves resume better.
- Custom cell buffer / second render thread: duplicates ratatui; data-race
  surface for no gain.
- Full class-hierarchy port (Component/Focusable/Container): TypeScript
  inheritance → Rust traits only where justified (§4); keep existing fn-based
  components.
- LaTeX transpiler, OSC-133 zones, Sixel: niche or absent value.
- `modifyOtherKeys` fallback + custom stdin buffer: crossterm owns decoding;
  custom framing risks regressions for marginal coverage.
- Building all §6 agent components speculatively: fleet/session/tool
  cards/pages already cover needs; add only TUI-020 scoped work.
