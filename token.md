# NIKI Design Tokens — `token.md` (Single Source of Truth)

> **Design Direction:** Warm Terracotta & Cream Studio Aesthetic (Claude Code / Anthropic Inspired).
> **Dominant Palette:** Warm Light-Brown / Terracotta Clay (`#CC785C`, `#D4A373`) and Off-White / Cream (`#FAF8F5`, `#E6DFD5`) over Warm Charcoal Espresso (`#1A1716`, `#201D1D`).
> **Accent Rule:** Green (`#4EBE82`) is strictly reserved for minimal interactive indicators — such as active thinking spinners, model generation activity, and final completion checkmarks `✓`. It is NEVER used as a primary structural or background color.

---

## Tier 1 — Primitives

These are the canonical raw color values. All semantic and component tokens resolve to these primitives.

```ini
# ── Warm Earth & Clay Primitives (Dominant Accents) ───────────
CLAY_500     = #cc785c   # signature warm terracotta / clay (hero brand accent)
CLAY_600     = #b85c38   # deep terracotta (light-mode hero / high contrast)
CLAY_400     = #d48b70   # light clay / hover highlight
SAND_500     = #d4a373   # warm sand / secondary accent / planner
SAND_600     = #b58352   # dark sand for light backgrounds
AMBER_500    = #e09f3e   # warm golden amber / user prompt / warnings

# ── Off-White & Cream Primitives (Dominant Foregrounds) ─────────
CREAM_100    = #faf8f5   # pure cream off-white (bright text / headers)
CREAM_200    = #f3efea   # soft cream (body text)
CREAM_300    = #e6dfd5   # muted sand-cream (dim labels / subtext)
CREAM_400    = #c4bbb0   # stone cream (secondary meta / borders)
CREAM_500    = #8a8480   # warm ash (subtle line numbers / paths)

# ── Warm Charcoal & Espresso Primitives (Dark Mode Base) ───────
ESPRESSO_900 = #141211   # deep void / terminal backdrop behind panels
ESPRESSO_800 = #1a1716   # base app canvas (dark mode background)
ESPRESSO_700 = #201d1d   # card & input surface background
ESPRESSO_600 = #282423   # elevated panels / modals / dropdowns
ESPRESSO_500 = #383330   # base borders & dividers
ESPRESSO_400 = #48423e   # hover borders / active focus

# ── Light Mode Warm Paper Primitives ───────────────────────────
PAPER_50     = #fdfcfc   # clean warm paper (light mode background)
PAPER_100    = #f8f5f2   # elevated card surface (light mode)
PAPER_200    = #f1ece6   # input & panel background (light mode)
PAPER_300    = #e5ded5   # base borders (light mode)
INK_900      = #1f1b1a   # deep espresso ink (primary text light)
INK_700      = #423d3b   # body ink (light mode)
INK_500      = #756d69   # muted ink (light mode)

# ── Minimal Interactive Accents (STRICTLY CONSTRAINED) ─────────
# Reserved only for active thinking/loading spinners and pass checkmarks
THINKING_GREEN = #4ebe82 # model thinking spinner / live token pulse
SUCCESS_GREEN  = #34d399 # test passed / checkmark ✓
ERROR_CORAL    = #e76f51 # failed test / syntax error / blocker
INFO_BLUE      = #6a9bcc # external link / git branch badge
```

---

## Tier 2 — Semantic Tokens (Mode-Aware)

| Token | Dark Mode (`#1a1716`) | Light Mode (`#fdfcfc`) | Usage & Role |
|---|---|---|---|
| `bg.canvas` | `#1a1716` (ESPRESSO_800) | `#fdfcfc` (PAPER_50) | Main TUI screen background |
| `bg.surface` | `#201d1d` (ESPRESSO_700) | `#f8f5f2` (PAPER_100) | Message cards, tool call boxes |
| `bg.input` | `#282423` (ESPRESSO_600) | `#f1ece6` (PAPER_200) | User input bar background |
| `bg.modal` | `#201d1d` (ESPRESSO_700) | `#ffffff` | Overlays, command palette, popup menus |
| `border.subtle` | `#383330` (ESPRESSO_500) | `#e5ded5` (PAPER_300) | Subtle container frames, box-drawing |
| `border.focus` | `#cc785c` (CLAY_500) | `#b85c38` (CLAY_600) | Active input border, focused message |
| `text.hero` | `#faf8f5` (CREAM_100) | `#1f1b1a` (INK_900) | NIKI 3D Logo, headers, strong emphasis |
| `text.body` | `#f3efea` (CREAM_200) | `#423d3b` (INK_700) | Primary chat and transcript text |
| `text.dim` | `#e6dfd5` (CREAM_300) | `#756d69` (INK_500) | Secondary metadata, agent descriptions |
| `text.muted` | `#8a8480` (CREAM_500) | `#9e948f` | Timestamps, durations, token counts |
| `brand.primary` | `#cc785c` (CLAY_500) | `#b85c38` (CLAY_600) | Terracotta brand accent, prompt symbol `>` |
| `brand.secondary` | `#d4a373` (SAND_500) | `#b58352` (SAND_600) | Pill badges, secondary highlights |
| `state.thinking` | `#4ebe82` (THINKING_GREEN) | `#2d9f67` | **Blinking/spinning "Thinking..." tag ONLY** |
| `state.success` | `#34d399` (SUCCESS_GREEN) | `#059669` | **Checkmark `✓`, test pass count ONLY** |
| `state.error` | `#e76f51` (ERROR_CORAL) | `#c94a29` | Error alerts, failed assertions |

---

## Tier 3 — Component & Agent Mapping

### Agent Hierarchy & Visual Signatures

| Agent Role | Glyph | Visual Color | Purpose |
|---|---|---|---|
| **Planner** | `◈` | `#d4a373` (Warm Sand) | Architecture & TaskSpec planning |
| **Coder** | `⟠` | `#cc785c` (Terracotta Clay) | Unified diff & file implementation |
| **Tester** | `◉` | `#e6dfd5` (Cream Stone) | Test execution & verification |
| **Reviewer** | `◆` | `#e09f3e` (Warm Amber) | Code review & audit verdict |

### Interactive UI Elements

- **Input Prompt:** Prompt cursor `|` in `#cc785c` (Clay), input box surface `#282423` with rounded pill badges `[sandbox]` and `[podman]` in `#383330`.
- **Status Bar & Shortcuts:** Monospace labels in `#faf8f5` with action descriptions in `#8a8480` (`tab toggle view   ctrl-p commands   esc quit`).
- **Diff Display:**
  - Added lines: Background `rgba(78, 190, 130, 0.12)`, text `#e6dfd5` with subtle `+` prefix.
  - Deleted lines: Background `rgba(231, 111, 81, 0.12)`, text `#8a8480` with `-` prefix.
- **Thinking / Generation Spinner:** Minimal spinning dot `⠋` or sparkle `✦` in `#4ebe82` accompanied by muted text `"Thinking..."`.

---

## Typography & Geometry Specs

- **Monospace Family:** `Berkeley Mono`, `JetBrains Mono`, `IBM Plex Mono`
- **Box Drawing:** Single rounded box lines: `╭ ─ ╮ │ ╰ ─ ╯`
- **Corner Radius:** 8px–12px on web / clean rounded UTF-8 glyphs in TUI
- **Spacing Scale:** 4px (xs), 8px (sm), 12px (md), 16px (lg), 24px (xl)
