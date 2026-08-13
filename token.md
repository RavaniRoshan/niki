# NIKI Design Tokens — `token.md` (single source of truth)

> **Provenance:** Colors extracted from the NIKI brand image (TUI screenshot). The palette
> uses three signature accents: teal (#4ecdc4) for hero/success, purple (#9682c8) for
> agent status, and blue (#5d8fd6) for structural elements. This file is the canonical
> source for every design token in NIKI (TUI, README, landing page, marketing kit).
> Today the TUI reads `src/display/theme.rs` directly; promote this file to a build-time
> source (e.g. a `tokens/token.md` consumed by a small generator that emits `tokens.rs`)
> so one edit flows everywhere. See research report §"Design tokens" for the migration plan.

## Tier 1 — Primitives (raw values, private to the theme module)

These are the only hardcoded hex values. Everything below resolves to these.

```
# Midnight Teal primitives (extracted from brand image)
TEAL_500   = #4ecdc4   # brand teal (hero accent)
TEAL_600   = #0d9488   # brand teal (darkened for light bg)
AMBER_500  = #f59e0b   # warm accent (dark)
AMBER_600  = #d97706   # warm accent (light)
CYAN_500   = #5d8fd6   # brand blue (header, badges)
PURPLE_500 = #9682c8   # brand purple (Coder/Tester)
CLAY_500   = #f59e0b

# Status
SUCCESS_500 = #4ecdc4   SUCCESS_600 = #0d9488
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
| `border.focus` | `#4ecdc4` | `#0d9488` | focused element border (brand teal) |
| `border.dim` | `#21262d` | `#e2e8f0` | subtle dividers |
| `text.primary` | `#e6edf3` | `#1e293b` | body text |
| `text.strong` | `#f0f6fc` | `#0f172a` | bold/emphasis |
| `text.dim` | `#8b949e` | `#64748b` | secondary text |
| `text.muted` | `#6e7681` | `#94a3b8` | counters, URLs |
| `accent.primary` | `#4ecdc4` | `#0d9488` | brand teal, focus, interactive |
| `accent.secondary` | `#f59e0b` | `#d97706` | highlights, secondary action |
| `accent.cyan` | `#5d8fd6` | `#4a6fa5` | links, info (brand blue) |
| `accent.purple` | `#9682c8` | `#7c5fc0` | logo/sparkle (brand purple) |
| `status.success` | `#4ecdc4` | `#0d9488` | checkmarks (brand teal) |
| `status.error` | `#f87171` | `#dc2626` | errors |
| `status.warning` | `#fbbf24` | `#d97706` | warnings |
| `prompt.cursor` | `#4ecdc4` (reversed) | `#0d9488` (reversed) | input cursor |
| `prompt.border` | `#4ecdc4` | `#0d9488` | input box border |
| `diff.add` (fg) | `#4ecdc4` | `#0d9488` | added lines |
| `diff.del` (fg) | `#f87171` | `#dc2626` | removed lines |
| `diff.add.bg` | `rgba(78,205,196,.15)` | `rgba(13,148,136,.12)` | added line bg |
| `diff.del.bg` | `rgba(248,113,113,.15)` | `rgba(220,38,38,.12)` | removed line bg |
| `role.user` | `#f59e0b` | `#d97706` | user messages |
| `role.assistant` | `#4ecdc4` | `#0d9488` | assistant messages |
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

## Conflicts — resolution status (verified 2026-08-13)

1. **Logo vs TUI brand mismatch — RESOLVED.** `assets/logo.svg` is teal `#0d9488`, matching the TUI brand.
2. **Spinner color — RESOLVED.** `theme::claude()` (spinners, status dot, logo) now returns `accent.primary` (teal). `accent.purple` remains reserved for the shell prompt marker (`theme::shell()`).
3. **`text.muted` mis-map — RESOLVED.** `text_muted()` now returns `fg_subtle` (`#6e7681` dark / `#94a3b8` light) per the Tier 2 table.
4. **Missing tokens — RESOLVED.** `text_strong()`, `autocomplete_bg()`, `scrollbar_thumb()`, `shimmer()` added to `theme.rs` (aliases per Tier 3 table).
5. **Dead compound styles — RESOLVED (removed).** `clay_accent`, `status_ok/err/warn`, `footer_style`, `block_border`, `dim_style`, `accent_style` removed from `theme.rs`; surviving styles: `header_style`, `status_running`, `block_border_active`.
6. **`ThemeMode::Auto` light detection — RESOLVED.** `resolved_mode()` interprets `COLORFGBG` (background index ≥ 8 → light), falling back to Dark when absent.

## Accessibility

- Target WCAG 2.0 AA (4.5:1 normal text, 3:1 large/UI). The dark palette passes for all
  text/neutral pairs; verify amber-on-light (`#d97706` on `#f8fafc`) for small text.
- Respect `NO_COLOR` and `TERM=dumb`. Provide a "bare mode" for screen readers.

## Spinner / glyph set (keep consistent everywhere)

- Stages: ◈ Planner · ⟠ Coder · ◉ Tester · ◆ Reviewer
- Spin: moon `◐ ◓ ◑ ◒`; sparkle `✦`; dot `●`; done `✓`; fail `✗`
- Borders: `╭─╮│╰─╯` (use rounded box-drawing, not ASCII `+-|`)
