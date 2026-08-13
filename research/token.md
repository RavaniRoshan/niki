# NIKI Design Tokens — `token.md` (single source of truth)

> **Provenance (read me):** You pasted an image for color extraction, but this model
> cannot read image input (clipboard/image read failed). Per your 3-try rule, this file
> was reconstructed from the **actual repo assets** — the implemented `Midnight Teal`
> palette in `src/display/theme.rs` and `assets/logo.svg` — which is the real, shipped
> brand. If your pasted image intended *different* colors, tell me and I'll regenerate.
>
> **Intended use:** This file is the canonical source for every design token in NIKI
> (TUI, README, landing page, marketing kit, sandbox assets). Today the TUI reads
> `src/display/theme.rs` directly; promote this file to a build-time source (e.g. a
> `tokens/token.md` consumed by a small generator that emits `tokens.rs`) so one edit
> flows everywhere. See research report §"Design tokens" for the migration plan.

## Tier 1 — Primitives (raw values, private to the theme module)

These are the only hardcoded hex values. Everything below resolves to these.

```
# Midnight Teal primitives
TEAL_500   = #0d9488   # brand primary (dark)
TEAL_600   = #0f766e   # brand primary (light)
AMBER_500  = #f59e0b   # warm accent (dark)
AMBER_600  = #d97706   # warm accent (light)
CYAN_500   = #22d3ee
PURPLE_500 = #a78bfa
CLAY_500   = #f59e0b

# Status
SUCCESS_500 = #34d399   SUCCESS_600 = #059669
ERROR_500   = #f87171   ERROR_600   = #dc2626
WARNING_500 = #fbbf24   WARNING_600 = #d97706

# Neutrals — dark
BG_DEEP_DARK   = #010409
BG_BASE_DARK   = #0d1117
BG_ELEVATED_DARK = #161b22
BG_SURFACE_DARK = #1c2128
BORDER_BASE_DARK = #30363d
BORDER_DIM_DARK  = #21262d
FG_DARK       = #e6edf3
FG_DIM_DARK   = #8b949e
FG_BRIGHT_DARK= #f0f6fc
FG_SUBTLE_DARK= #6e7681

# Neutrals — light
BG_DEEP_LIGHT   = #0f172a
BG_BASE_LIGHT   = #f8fafc
BG_ELEVATED_LIGHT = #ffffff
BG_SURFACE_LIGHT = #f1f5f9
BORDER_BASE_LIGHT = #cbd5e1
BORDER_DIM_LIGHT  = #e2e8f0
FG_LIGHT       = #1e293b
FG_DIM_LIGHT   = #64748b
FG_BRIGHT_LIGHT= #0f172a
FG_SUBTLE_LIGHT= #94a3b8

# Agent role hues (dark)
AGENT_RED_DARK    = #ff6b6b
AGENT_BLUE_DARK   = #38bdf8
AGENT_GREEN_DARK  = #34d399
AGENT_YELLOW_DARK = #f59e0b
AGENT_PURPLE_DARK = #a78bfa
AGENT_ORANGE_DARK = #fb923c
AGENT_PINK_DARK   = #f472b6
AGENT_CYAN_DARK   = #22d3ee

# Agent role hues (light) — darkened for contrast
AGENT_RED_LIGHT    = #ef4444
AGENT_BLUE_LIGHT   = #0891b2
AGENT_GREEN_LIGHT  = #059669
AGENT_YELLOW_LIGHT = #d97706
AGENT_PURPLE_LIGHT = #7c3aed
AGENT_ORANGE_LIGHT = #c2410c
AGENT_PINK_LIGHT   = #be185d
AGENT_CYAN_LIGHT   = #0e7490
```

## Tier 2 — Semantic tokens (mode-aware, public API)

| Token | Dark | Light | Role / usage |
|---|---|---|---|
| `bg.deep` | `#010409` | `#0f172a` | terminal void / behind panels |
| `bg.base` | `#0d1117` | `#f8fafc` | app background |
| `bg.elevated` | `#161b22` | `#ffffff` | cards, modals |
| `bg.surface` | `#1c2128` | `#f1f5f9` | panels, inputs |
| `border.base` | `#30363d` | `#cbd5e1` | default borders |
| `border.focus` | `#0d9488` | `#0f766e` | focused element border |
| `border.dim` | `#21262d` | `#e2e8f0` | subtle dividers |
| `text.primary` | `#e6edf3` | `#1e293b` | body text |
| `text.strong` | `#f0f6fc` | `#0f172a` | bold/emphasis |
| `text.dim` | `#8b949e` | `#64748b` | secondary text |
| `text.muted` | `#6e7681` | `#94a3b8` | counters, URLs |
| `accent.primary` | `#0d9488` | `#0f766e` | brand, focus, interactive |
| `accent.secondary` | `#f59e0b` | `#d97706` | highlights, secondary action |
| `accent.cyan` | `#22d3ee` | `#0891b2` | links, info |
| `accent.purple` | `#a78bfa` | `#7c3aed` | logo/sparkle (currently used for spinner) |
| `status.success` | `#34d399` | `#059669` | checkmarks |
| `status.error` | `#f87171` | `#dc2626` | errors |
| `status.warning` | `#fbbf24` | `#d97706` | warnings |
| `prompt.cursor` | `#0d9488` (reversed) | `#0f766e` (reversed) | input cursor |
| `prompt.border` | `#0d9488` | `#0f766e` | input box border |
| `diff.add` (fg) | `#34d399` | `#059669` | added lines |
| `diff.del` (fg) | `#f87171` | `#dc2626` | removed lines |
| `diff.add.bg` | `rgba(52,211,153,.15)` | `rgba(5,150,105,.12)` | added line bg |
| `diff.del.bg` | `rgba(248,113,113,.15)` | `rgba(220,38,38,.12)` | removed line bg |
| `role.user` | `#f59e0b` | `#d97706` | user messages |
| `role.assistant` | `#0d9488` | `#0f766e` | assistant messages |
| `role.system` | `#8b949e` | `#64748b` | system messages |

### Agent role colors (semantic -> primitive above)

Planner, Coder, Tester, Reviewer + any user-defined stages map to the 8 agent hues
(`role_color()` in `theme.rs`). Keep order stable so a stage is the same color every run.

## Tier 3 — Component tokens (widget-level, resolved from Tier 2)

| Token | Resolves to | Used by |
|---|---|---|
| `chat.input.bg` | `bg.surface` | chat input box |
| `chat.input.border` | `prompt.border` | chat input box |
| `chat.cursor` | `prompt.cursor` | input cursor |
| `status_bar.bg` | `bg.elevated` | bottom status line |
| `tab.active.fg` | `accent.primary` | active tab |
| `modal.border` | `border.focus` | permission/confirm modals |
| `spinner` | `accent.purple` (conflict, see below) | loading animation |
| `autocomplete.bg` | `bg.elevated` | (future) completion dropdown |
| `scrollbar.thumb` | `border.focus` | (future) scrollbar |
| `selection.bg` | `accent.primary` | copy selection highlight |

## Conflicts to resolve before launch

1. **Logo vs TUI brand mismatch.** `assets/logo.svg` uses blue `#58a6ff` for the
   "NIKI" wordmark, but the shipped TUI identity is teal `#0d9488`. Pick one.
   Recommendation: make the logo teal to match the product, OR accept blue as a
   deliberate "ink" brand color and keep teal as the *interactive* accent. Document it.
2. **Spinner color.** `theme.rs` `claude()` returns purple `#a78bfa` and is used for the
   logo/spinner, contradicting the teal brand the doc recommends. Either rename to
   `brand_ink()` or switch the spinner to `accent.primary`.
3. **`text.muted` mis-map.** Code's `text_muted()` returns `#8b949e` (fg_dim) while the
   doc's `text.muted` = `#6e7681` (fg_subtle). Align the value.
4. **Missing tokens.** `text_strong`, `autocomplete_bg`, `scrollbar_thumb`, `shimmer`
   are referenced by the doc but absent in code. Add them or drop the references.
5. **Dead compound styles** (`clay_accent`, `status_ok/err/warn`, `footer_style`,
   `block_border`, `dim_style`, `accent_style`) are defined but unused. Wire or remove.
6. **`ThemeMode::Auto`** only falls back to Dark; no OSC 4 / `COLORFGBG` detection.

## Accessibility

- Target WCAG 2.0 AA (4.5:1 normal text, 3:1 large/UI). The dark palette passes for all
  text/neutral pairs; verify amber-on-light (`#d97706` on `#f8fafc`) for small text.
- Respect `NO_COLOR` and `TERM=dumb`. Provide a "bare mode" for screen readers.

## Spinner / glyph set (keep consistent everywhere)

- Stages: ◈ Planner · ⟠ Coder · ◉ Tester · ◆ Reviewer
- Spin: moon `◐ ◓ ◑ ◒`; sparkle `✦`; dot `●`; done `✓`; fail `✗`
- Borders: `╭─╮│╰─╯` (use rounded box-drawing, not ASCII `+-|`)
