# Replicating Claude Code's Terminal UI Quality in Niki (Rust / ratatui)

**Topic:** Make Niki's terminal UI look, feel, and perform as well as Claude Code's terminal UI.
**Date:** 2026-08-04
**Method:** Deep research — 6 parallel research subagents, 1 adversarial verification subagent, 1 resolution re-query (primary-source extraction from the actual Claude Code binary).

---

## Executive Summary

Claude Code's premium feel is not a CSS trick — it is a **differential rendering engine** (React scene graph → layout → 2D cell raster → frame diff → minimal ANSI patch) that redraws only changed cells, paired with synchronized output (DEC 2026) to eliminate flicker, and a carefully restrained **token-based theme** built around a signature clay-orange accent (`#D77757` for the agent/spinner, `#d97757` for brand accents). All of this is reproducible in Niki's existing ratatui stack: ratatui already diffs frames by default, so the correct architecture is a **dirty-flag event loop that redraws only when state changes**, a **verified 3-token theme system** (values extracted from the real binary), and **stable, pinned layout** (fixed input bar, visible-only message virtualization, no layout jumps while streaming). The two things that will most likely make Niki feel "cheap" are whole-screen repaints during streaming and over-use of color; the two that will most likely make it feel "premium" are low-churn updates and a consistent, restrained color token set.

---

## Part 1 — Verified Facts About Claude Code's Terminal UI

### 1.1 Rendering architecture (why it feels smooth)

Confirmed by Anthropic's own rendering engineer (chrislloyd) on HN and by multiple independent source-level analyses:

| Fact | Detail | Confidence |
|---|---|---|
| Pipeline per frame | React scene graph → layout → rasterize to 2D screen → **diff against previous screen** → emit ANSI only for changed cells | High (2+ sources incl. first-party) |
| Originally built on Ink, then rewritten | Ink-based renderer rewritten from scratch; only React retained | High (first-party, issue #769) |
| Differential renderer shipped | "only ~1/3 of sessions see at least a flicker" after rewrite | Medium (single HN comment; usable as anecdote, not KPI) |
| Screen buffer | Double-buffered, packed Int32Array; each cell = 21 bits codepoint + 4 bits fg + 4 bits bg + 3 bits style; zero per-frame allocation | Medium (2 leak analyses agree; JS-internal, no direct Rust equivalent) |
| Frame budget | ~16ms budget, ~5ms scene-graph→ANSI | Medium (first-party comment; do not hardcode) |
| Update throttling | Reverse-engineering reports ~100ms (10 FPS) coalescing + virtualized (visible-only) message list | Medium (single leak-era source; superseded by fullscreen renderer) |
| Fullscreen (alt-screen) renderer | Draws on alternate screen "like vim", **only renders visible messages**, **input box pinned**, sends only changed cells; `CLAUDE_CODE_ALT_SCREEN_FULL_REPAINT=1` env var = full-repaint fallback for ConPTY terminals | High (official docs, verbatim) |
| Flicker root cause (classic renderer) | Scrollback must be "cleared entirely and redrawn" on change → tearing; no incremental scrollback update possible in a terminal | High (first-party + docs) |
| Streaming | Tokens chunked (not token-by-token); React `useDeferredValue` coalesces updates; shimmer on in-progress responses | Medium (reverse-engineering + issue #29213) |

**Anti-flicker mechanism — synchronized output (DEC 2026):** Claude Code probes terminal support at startup and wraps frames in `\x1b[?2026h` ... `\x1b[?2026l`, telling the terminal to buffer and atomically display. Anthropic authored patches upstream in xterm.js (PR #5453, milestone 6.0.0) and tmux (PR #4744). **Caveats (verified):**
- tmux releases through 3.6 **do not implement** synchronized output (official docs say so verbatim) → expect more flicker under tmux ≤3.6.
- xterm.js < 6.0 (thus older VSCode terminals) has **no** DEC 2026 support at all; VSCode gets it only via bundled xterm ≥6.0 (Insiders ~Jan 2026).
- Some terminals (tmux, per third-party analysis) strip DEC 2026 markers unless the client advertises the `sync` terminal feature.
- **Action:** emit DEC 2026 best-effort + capability probe (DECRQM) + full-repaint fallback (mirroring `CLAUDE_CODE_ALT_SCREEN_FULL_REPAINT=1`). Never claim "flicker-free everywhere."

**Design-system primitives worth copying** (transferable, renderer-agnostic):
- Dirty-flag propagation with "blit" (copy unchanged subtrees from previous frame).
- String interning pools (chars/styles/hyperlinks → integer IDs so diffing is integer comparison).
- Virtual message list with overscan + progressive mounting.
- ANSI gotcha: `bold` and `dim` share the reset sequence `\x1b[22m` — they must be isolated in separate spans to avoid one resetting the other.

### 1.2 UI components and layout (what's on screen)

All confirmed in current official docs:

- **Scrolling transcript + fixed input box.** Input pinned at bottom; in fullscreen mode the input never moves while output streams.
- **Input affordances:** `>` prompt; grayed inline suggestion (accept `Tab`/`→`, dismiss by typing); quick-prefix menus: `/` commands/skills, `!` shell, `@` file mentions, `:` emoji, `?` shortcut help.
- **Mode indicator** cycling permission modes (Manual/default, acceptEdits, plan, auto, bypassPermissions) via `Shift+Tab`.
- **Footer badges + status line:** status line is a separate row above footer badges; shows model, directory, git branch, context-usage progress bar, cost, duration. PR status badge = colored-underline hyperlink (green approved / yellow pending / red changes requested / gray draft).
- **Collapsible tool rows:** repeated MCP calls collapse to "Called slack 3 times"; click to expand. "Jump to bottom" floating button with "3 new messages" counts.
- **Overlays/dialogs:** permission prompts, `/model`, `/config`, select & multi-select menus, file autocomplete, reverse-search, task-list toggle (`Ctrl+T`, up to 5 checklist items with pending/in-progress/complete), transcript viewer (`Ctrl+O`, timestamps + model per message).
- **Focus mode** (`/focus`): only last prompt, one-line tool-call summary with edit diffstats, final response.
- **Typography:** CLI body text is monospace and terminal-theme-driven — brand serif/sans fonts (Styrene/Tiempos vs Poppins/Lora — sources disagree, **irrelevant to a terminal TUI**).

### 1.3 Theme system and EXACT colors (primary-source verified)

**Theme mechanism (documented):**
- Themes are ~60–69 named color tokens; ~35 documented. Settings key `theme` in `~/.claude/settings.json` (default `"dark"`); `/theme` picker; `/config set theme light`.
- 7 built-ins: `auto`, `dark` (default), `light`, `dark-daltonized`, `light-daltonized`, `dark-ansi`, `light-ansi`. Standard = 24-bit RGB; ANSI variants = 16 colors; daltonized swap red/green for blue/yellow.
- Custom themes (v2.1.118+): JSON at `~/.claude/themes/*.json` with `name`, `base`, `overrides`; colors accept `#rrggbb`, `#rgb`, `rgb(r,g,b)`, `ansi256(n)`, `ansi:<name>`; directory hot-reloaded.
- `auto` detects background via `$COLORFGBG` or OSC 11 query + BT.709 luminance (`0.2126R+0.7152G+0.0722B`, >0.5 → light).
- Known limitation: `/theme` emits hardcoded RGB truecolor and does **not** inherit the terminal's ANSI palette (issue #39369); ANSI themes are the workaround.
- `/color` command recolors only agent/subagent identifiers, independent of theme.

**EXACT default (dark) theme values — extracted from the real binary** (`~/.local/share/claude/versions/2.1.220`, identical in 2.1.216 & 2.1.218). These resolve the hex conflict from the research phase:

| Token | Hex | Role |
|---|---|---|
| `claude` | **#D77757** (rgb 215,119,87) | Agent identifier + spinner accent — identical in dark AND light |
| `text` / `inverseText` | #FFFFFF / #000000 | Body text (dark theme) |
| `inactive` / `subtle` | #999999 / #505050 | Muted / dim text |
| `success` | **#4EBA65** | Success/summary |
| `error` | **#FF6B80** | Errors |
| `warning` | #FFC107 | Warnings |
| `merged` | #AF87FF | Merged edits |
| `diffAdded` (bg) | **#225C2B** | Added line background |
| `diffRemoved` (bg) | **#7A2936** | Removed line background |
| `diffAddedDimmed` / `diffRemovedDimmed` | #47584A / #69484D | Muted diff bg |
| `diffAddedWord` / `diffRemovedWord` | #38A660 / #B3596B | Inline word diff |
| `selectionBg` | #264F78 | Selection |
| Sub-agent colors (red, blue, green, yellow, purple, orange, pink, cyan) | #DC2626, #6A9BCC, #16A34A, #CA8A04, #827DBD, **#D97757**, #C46686, #0891B2 | Per-agent identifier markers |

**Light theme (same binary):** text #000000, inverseText #FFFFFF, success #2C7A39, error #AB2B3F, warning #966C1E, diffAdded bg #69DB7C, diffRemoved bg #FFA8B4, diffAddedWord #2F9D44, diffRemovedWord #D1454B, `claude` orange #D77757 (same as dark).

**Orange conflict RESOLVED:** both hexes are real but serve different elements — `#D77757` = agent/spinner color (theme objects), `#d97757` = brand/clay accent (`--clay` CSS token, matches Anthropic brand-guidelines skill) and also used for the orange sub-agent color. The blog was not wrong; the brand skill was not wrong.

**Secondary/newer design system:** the binary also embeds a newer `tokens.css` with different semantic values (dark `--text-success` #0ca30c, `--text-danger` #ec7e7e, `--text-git-added` #32d74b, `--text-git-removed` #ff2c56, `--text-git-modified` #ffd014; light #006300, #1e9e3c, #cd2054; diff backgrounds = `color-mix(... 20%, transparent)`). Which system paints the terminal at runtime is unresolved — **treat the named-theme object values as the `/theme`-documented source of truth**.

### 1.4 Rich content rendering (streaming, markdown, diffs)

From source-level reverse engineering (mid-2025 leak era — describes the **older** renderer; library names are forensic evidence, not Anthropic-confirmed):

- **Markdown:** `marked` lexer → ANSI strings; regex fast-path skips lexer for plain text; LRU token cache (~500 entries); tables render via dedicated flexbox component.
- **Syntax highlighting:** `cli-highlight` loaded async (React Suspense) so it never blocks first paint; pre-rendered output emitted through a raw-ANSI node that skips re-tokenization/wrapping.
- **Diffs:** unified-hunk view (NOT side-by-side) with line numbers; green `+` additions, red `-` deletions, gray context; truncates to terminal width; async-loaded with placeholder frame.
- **Streaming effect:** SSE tokens → tiny store; `useSyncExternalStore` subscribers; `Object.is` dedup in `setState` prevents render storms; frame loop throttles to ~16ms; per-cell diff means one new char = cursor move + few bytes. Shimmer animation on in-progress responses; blinking dots/spinners for tool progress.
- **ANSI techniques:** chalk for SGR; OSC 8 hyperlinks; DEC 1049 alt-screen; DECSTBM scroll regions; DEC 2026 synchronized output; SGR mouse tracking; grapheme clustering + `stringWidth`.

**Relevance to Rust:** the JS library names are irrelevant to Niki. Keep the transferable concepts: cache markdown tokens, async + cached syntax highlighting, unified hunk diffs, cursor-move-only streaming updates, OSC 8 links.

---

## Part 2 — Ratatui Recipes (the Rust side, current & authoritative)

All from ratatui maintainers (Joshka, kdheepak) and official docs — **this is the implementation spec for Niki:**

### 2.1 Event loop & rendering architecture
- Ratatui is immediate-mode: you call `terminal.draw(|f| ui(f))` yourself, once per loop iteration. **Do not call `draw()` multiple times per frame** (double-buffer diff assumption).
- **Separate tick rate from frame rate.** Drive `Event::Tick` and `Event::Render` on independent intervals; **defer all redraws to the same tick/event** so you don't redraw per keystroke or block the render thread.
- Animation recipe: time is input to a state function ("given that it's 16ms later, what should I render now"), rendered at ~60fps.
- **Redraw only when something changed (dirty flag).** 60fps is useless if every frame is identical; ~30 FPS is "good enough" for most terminal apps. Static content at 60fps burned 50% of a core in debug / 7% in release — dominated by buffer diffing, not your widgets.
- Diff-vs-redraw tradeoff: above ~30–40% changed characters, the diff cost exceeds a full redraw.
- Build/cache widgets **outside** `draw()` (the `WidgetRef` pattern).
- ratatui ≥0.30.1 adds `Terminal::apply_buffer` (commit incremental buffer writes outside a single `draw` closure), plus `CellDiffOption` / `CellWidth`. Version-pin if you want these.

### 2.2 Color
- **Ratatui does NOT ship terminal capability detection** (no COLORTERM/truecolor probe) — you must detect it yourself and degrade gracefully.
- Use truecolor (`Color::Rgb`) when COLORTERM=truecolor; fall back to 256-color; never assume the 16 ANSI palette is meaningful (users restyle it; contrast is unpredictable).
- Pair color with text/symbols — meaning must survive `NO_COLOR`, `TERM=dumb`, and pipes.

### 2.3 Unicode / emoji
- Cell model: one terminal cell ↔ one width unit; ratatui uses `unicode-width`.
- **`unicode-width` is unreliable for emoji/grapheme clusters** (font + terminal dependent; e.g. 💼 = 5 cols in Windows Terminal as a bug).
- Only portable approach today: treat emoji codepoints at single-cell width (what vim does). `grapheme-width-rs` is prototyped but unstandardized.
- Use the `unicode-truncate` crate for width-aware truncation instead of byte slicing.
- Box-drawing/braille/icon glyphs require a **Nerd Font** (or Alacritty/iTerm2's `builtin_box_drawing`), else they render as "□".

### 2.4 Reference implementations
- **claude-code-rust (srothgan)** — a native ratatui + Crossterm reimplementation of the Claude Code TUI. Legit code reference for patterns. **Caveat (from verification):** its README "Why" section is competitor marketing against the stock TUI; don't cite its performance claims about Claude Code, don't adopt its architecture wholesale (predates CC's fullscreen rework).
- **claude-manager** (crates.io) — ratatui-based, real, useful as architecture reference.
- **OpenCode** — claimed ratatui-based in a single self-published source; **unverified, drop**.

---

## Part 3 — Design Principles for "Premium" Terminal UI

High-confidence, multiply-corroborated principles:

1. **Low-churn rendering is the #1 perceived-quality lever.** Flicker/tearing reads as cheap. Differential rendering (ratatui's default) + DEC 2026 + no whole-screen clears.
2. **Stable layout that doesn't jump.** Pin frame/help/status regions; only mutate the content area while tokens stream. This is the single biggest "feels polished" factor for an LLM agent TUI.
3. **Use color with intention and restraint.** "If everything is a different color, the color means nothing." Prefer a small token set + subtle bold/dim over many hues. One signature accent color (the Claude orange) goes a long way.
4. **Hierarchy via contrast, not decoration.** Use spacing, caps, lines, borders, and contrast to signal importance (r/commandline TUI-design consensus).
5. **Consistent border style.** Rounded `╭╮╰╯` vs square `┌┐└┘` is a deliberate identity choice (Claude Code uses rounded; terminal.shop uses square). Pick one, stay consistent.
6. **Respect color ergonomics:** `NO_COLOR`, `TERM=dumb`, grep-able output, color as a layer never essential.
7. **Accessibility floors:** WCAG AA ≈ 4.5:1 normal text, 3:1 large text — as a design principle, but you can't guarantee it since terminals restyle colors. Check contrast of the pairs you place adjacent.
8. **Progress feedback** prevents "is it working?" anxiety.
9. **Animations:** skeptical guidance — delightful once, annoying after a thousand uses. Use shimmer/spinner sparingly, never view-transition animations.
10. **Perceived speed beats decoration.** Instant transitions, no superfluous visual baggage.

---

## Part 4 — Disagreements, Verification Issues, and Resolutions

### Resolved during research
| Issue | Resolution |
|---|---|
| Claude orange: #D77757 vs #d97757 vs #D4A27F vs #C15F3C | **Extracted from real binary**: #D77757 = agent/spinner; #d97757 = brand clay accent + orange sub-agent color; #D4A27F = docs-site OG metadata (unrelated); #C15F3C = product web palette (unrelated surface). Use #D77757/#d97757 per element. |
| Semantic colors (success/error/diff bgs) single-blog source | **Verified in binary** (v2.1.220, matches blog exactly). Safe to use. |
| "Anthropic patched tmux" → flicker-free? | **No.** tmux ≤3.6 lacks DEC 2026 (docs verbatim). Best-effort emission + probe + fallback. |
| Ink vs custom renderer | First-party says rewritten from scratch (React retained); leak-era "Ink" labels are historical. |
| 16ms frame budget vs 100ms throttle | Both dropped as implementation inputs; use ratatui's dirty-flag/30fps guidance instead. |
| Fullscreen renderer "default"? | Default only for installs after 2026-05-06; earlier users keep classic renderer. It's a "research preview." Two active looks exist; aim at fullscreen look but note its UX costs (loses native scrollback search, copy-on-select). |
| Leak-era internals (marked, cli-highlight, Yoga, CharPool) | Describe the OLD renderer; JS-specific. Keep only transferable concepts; drop library names from the plan. |
| Emojis in TUIs | Genuinely split sources. Middle ground: fine as deliberate branding in a full-screen TUI; keep grep-compatible. |
| 60fps vs 30fps | Reconciled: 60fps only while content actually animates; dirty-flag otherwise. |
| Brand fonts (Styrene/Tiempos vs Poppins/Lora) | Irrelevant — terminal TUI is monospace, theme-driven. Dropped. |

### Carried limitations (accepted, not resolved)
- **Runtime-painted theme unknown:** whether the legacy named-theme object or the newer `tokens.css` design system paints the terminal at runtime is unresolved. The named-theme values match `/theme` docs; use them.
- **Whether `success`/`error` keys apply as text color, background, or both** at runtime — unresolved; choose per your UI (text for status, bg for diffs, as the names suggest).
- **Exact ANSI-16 mapping per token** for the ANSI themes — undocumented.
- **Frame-commit mechanics** (how many ANSI writes per commit) — undocumented; ratatui's single `draw()` handles this correctly by default.
- **claude-chill scroll-rate numbers** (4,000–6,700 events/sec) describe the classic renderer — treat as stale, not a Niki requirement.

---

## Part 5 — Recommendations for Niki (mapped to the current codebase)

Niki already has the right foundation: `ratatui 0.29` (immediate-mode diff renderer), `crossterm` backend, `syntect 5` (syntax highlighting — the Rust `cli-highlight` equivalent), `similar 2` (diffing), and a 11-page router with scrollable pages. Current pages: Run, Pipeline, Agents, Diff, Verdict, Cost, Artifacts, History, Config, Help, TestLog.

### High-impact (do first)
1. **Dirty-flag event loop.** Replace any per-keystroke or per-token `draw()` with: redraw on `Event::Render` (≈30fps cap) or on explicit state change; only the visible page area re-renders (ratatui diffs automatically). Add `cargo flamegraph` profiling.
2. **Token-based theme module** with the verified dark/light palettes from §1.3 (`claude` #D77757 accent, success #4EBA65, error #FF6B80, warning #FFC107, diff bg #225C2B/#7A2936, dim #999999/#505050, sub-agent colors). Centralize in one `Theme` struct; add `--no-color`/`NO_COLOR` and a `dark`/`light`/`auto` setting mirroring Claude's `theme` key. Pair color with symbols (✓/✗/⚠) so meaning survives color stripping.
3. **DEC 2026 synchronized output** on fullscreen entry (best-effort: probe with DECRQM, wrap frame in `\x1b[?2026h`/`\x1b[?2026l`, full-repaint fallback on ConPTY). This is the biggest flicker killer.
4. **Pinned layout during streaming.** Keep the status bar, footer, and input region fixed; only the transcript area scrolls. No layout jumps while tokens arrive.

### Medium-impact
5. **Unified-hunk diffs** with Claude's exact diff colors (already have `similar`; add line numbers, green/red/gray scheme per §1.3).
6. **Virtualize long message lists** (visible-only rendering, overscan) — matches Claude's approach and bounds CPU.
7. **Width-aware text ops:** `unicode-truncate` for truncation; treat emoji as single-width (vim convention); document Nerd Font requirement in the README and detect missing glyphs.
8. **Cached/async syntax highlighting:** run `syntect` once per code block outside `draw()` and cache; never re-highlight per frame.
9. **OSC 8 hyperlinks** for artifact/file paths in the TUI.
10. **Modal/dialog consistency:** rounded borders (`╭╮╰╯`) everywhere or square everywhere — pick and standardize (Claude uses rounded).

### Polish (nice-to-have)
11. **Status line** (model, task dir, git branch, context-usage bar, cost, duration) as a configurable row above the footer.
12. **"Jump to bottom" + new-message count** floating control on the Run/TestLog transcript pages.
13. **Spinner/shimmer** only on the active streaming page; blinking dots for tool progress; keep everything else static.
14. **Agent color coding:** distinct color per agent role (Planner/Coder/Tester/Reviewer) using the verified sub-agent palette.
15. **`auto` background detection** via OSC 11 + BT.709 luminance.

### Anti-recommendations (verified wrong to do)
- Don't hardcode a 16ms or 100ms render interval — use dirty-flag/30fps guidance.
- Don't adopt claude-code-rust's architecture wholesale or cite its README claims.
- Don't use side-by-side diffs (Claude uses unified hunks).
- Don't add view-transition animations or per-keystroke full repaints.
- Don't rely on the 16 ANSI colors for the signature look (truecolor required; document it).

---

## Full Source List

**Official Anthropic / Claude Code docs**
- https://code.claude.com/docs/en/fullscreen
- https://code.claude.com/docs/en/interactive-mode
- https://code.claude.com/docs/en/statusline
- https://code.claude.com/docs/en/settings
- https://code.claude.com/docs/en/terminal-config
- https://code.claude.com/docs/en/terminal-config#create-a-custom-theme
- https://github.com/anthropics/skills/blob/main/skills/brand-guidelines/SKILL.md
- https://github.com/anthropics/claude-code/issues/769#issuecomment-3667315590

**First-party engineer statements / upstream patches**
- https://news.ycombinator.com/item?id=46699072 (chrislloyd comments 46706040, 46701013, 46701325, 46706299)
- https://github.com/xtermjs/xterm.js/pull/5453
- https://github.com/tmux/tmux/pull/4744
- https://github.com/anthropics/claude-code/issues/29213
- https://github.com/anthropics/claude-code/issues/39369

**Primary-source theme extraction**
- Local binary `~/.local/share/claude/versions/2.1.220` (also 2.1.216, 2.1.218) — named theme objects + `tokens.css`
- https://blog.vincentqiao.com/en/posts/claude-code-theme/ (corroborated by binary extraction)
- https://gist.github.com/cameronsjo/34a6fb8ade2b44c8380e1a2adebbac2b (token schema; names only, values unverified)
- https://mobbin.com/colors/brand/claude (product palette — different surface)
- https://www.loftlyy.com/en/anthropic (Anthropic neutrals — different surface)

**Reverse-engineering analyses (leak era, describe older renderer)**
- https://karanprasad.com/blog/how-claude-code-actually-works-reverse-engineering-512k-lines
- https://dev.to/minnzen/i-studied-claudes-leaked-source-and-built-a-terminal-ui-toolkit-from-it-4poh
- https://claude-harness.dev/en/articles/14-terminal-ui
- https://anthhub.github.io/open-claude-code/en/05-ink-rendering.html
- https://petrguan.github.io/claude-code-anatomy/terminal-ui/
- https://moelabs.dev/blog/inside-claude-code-08-terminal-ui/
- https://github.com/777genius/claude-code-working (Ink label — historical)

**Ratatui / Rust (official + maintainer guidance)**
- https://ratatui.rs/concepts/rendering/
- https://ratatui.rs/faq/
- https://ratatui.rs/tutorials/counter-async-app/full-async-events/
- https://ratatui.rs/highlights/v0301/
- https://github.com/ratatui/ratatui/discussions/579
- https://github.com/ratatui/ratatui/issues/1338
- https://github.com/ratatui/ratatui/discussions/1438
- https://github.com/ratatui/ratatui-image
- https://crates.io/crates/unicode-truncate
- https://github.com/srothgan/claude-code-rust (code reference only)
- https://crates.io/crates/claude-manager

**Design principles**
- https://clig.dev/
- https://bettercli.org/design/using-colors-in-cli/
- https://jensroemer.com/writing/tui-design/
- https://brandur.org/interfaces
- https://lucasfcosta.com/blog/ux-patterns-cli-tools
- https://p.janouch.name/article-tui.html
- https://www.w3.org/WAI/WCAG21/Understanding/contrast-minimum.html
- https://news.ycombinator.com/item?id=25304257
- https://www.reddit.com/r/commandline/comments/1t0olhz/how_do_you_guys_design_tui_applications/

**Third-party behavior/analysis**
- https://angular.schule/blog/2026-02-claude-code-scrolling/
- https://chad.cm/thought/2026-5-21-claude-code-ansi-theme
