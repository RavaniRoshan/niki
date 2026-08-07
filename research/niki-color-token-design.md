# NIKI Color Token Design System — Research & Recommendations

**Date:** 2026-08-06
**Depth:** Wide (landscape + codebase audit)
**Target:** Unique token design system that differentiates NIKI from Claude Code, Kimi Code, Copilot

---

## Executive Summary

NIKI's current color system (97 accessors, 32 palette fields) is comprehensive but **too close to Claude Code's blue/purple identity** — `primary()` maps to `#007aff` (blue), `claude()` to `#af87ff` (purple). Three sources we could not access (403) would strengthen the best-practices claims; the accessible sources still converge on semantic naming, three-tier architecture, and WCAG AA compliance.

Our audit found **one hardcoded color bug** in `state.rs` and **one light-mode incompatibility** in code blocks. The unused compound styles (`clay_accent`, `status_ok`, etc.) are dead code.

We recommend **Option B: "Midnight Teal"** — a cool, deep teal primary (`#0d9488`) with warm amber accents (`#f59e0b`) that occupies the unoccupied cool-toned space between Claude Code's blue/purple and Copilot's green. This gives NIKI a distinctive "precision engineering" brand identity while keeping all the semantic expectations (red=error, green=success) intact.

---

## Part 1: Competitor Color Token Systems

### 1.1 Claude Code — The Reference Standard

Claude Code has the most thoroughly documented system (~30+ tokens):

| Token Group | Tokens | Purpose |
|---|---|---|
| **Brand/Accent** | `claude`, `inverseText`, `inactive`, `subtle`, `suggestion`, `permission`, `remember` | Core UI chrome |
| **Status** | `success`, `error`, `warning`, `merged` | Operation outcomes |
| **Mode Indicators** | `promptBorder`, `planMode`, `autoAccept`, `bashBorder`, `ide`, `fastMode` | Current tool mode |
| **Diff** | `diffAdded`, `diffRemoved`, `diffAddedDimmed`, `diffRemovedDimmed`, `diffAddedWord`, `diffRemovedWord` | Word-level diff granularity |
| **Subagents** | 8 named colors with `_FOR_SUBAGENTS_ONLY` suffix | Multi-agent differentiation |
| **Ultrathink** | `rainbow_<color>` + `rainbow_<color>_shimmer` for 7 rainbow colors | Animated thinking display |
| **Base Presets** | `dark`, `light`, `dark-daltonized`, `light-daltonized`, `dark-ansi`, `light-ansi` | Accessibility variants |

**Custom theming:** `~/.claude/themes/*.json` with `name`, `base`, and `overrides` fields. Supports `#rrggbb`, `#rgb`, `rgb(r,g,b)`, `ansi256(n)`, `ansi:<name>` color values. (source: https://code.claude.com/docs/en/terminal-config)

**Brand identity:** Warm orange/sand tones — "claude-sand orange" lives in the `ansi:redBright` slot. Emotional positioning: "less hype more humanity." (source: https://dev.to/palo_alto_ai/four-themes-for-a-terminal-you-read-more-than-you-syntax-highlight-58kd)

### 1.2 GitHub Copilot CLI — Preset Modes Only

- `/theme` command supports: `auto`, `default`, `dim`, `high-contrast`, `colorblind` (source: https://github.blog/changelog/2026-06-23-copilot-cli-new-terminal-interface-is-generally-available/)
- Does NOT expose individual token customization
- Auto theme probes terminal ANSI palette via OSC 4
- Known issue: thinking text uses hardcoded dim color (source: https://github.com/github/copilot-cli/issues/3866)
- Brand identity: green

### 1.3 Kimi Code CLI — Minimal

- Only `theme = "dark" | "light"` option (source: https://github.com/MoonshotAI/kimi-cli/issues/1981)
- No custom theming; feature request open for catppuccin/gruvbox/nord presets
- Brand identity: warm tones

### 1.4 Aider — Role-Based Flags

- `--dark-mode` / `--light-mode` for theme selection
- Individual color flags: `--user-input-color (#00cc00)`, `--tool-error-color (#FF2222)`, `--tool-warning-color (#FFA500)`, `--assistant-output-color (#0088ff)` (source: https://aider.chat/docs/config/options.html)
- `--code-theme` for syntax highlighting (monokai, solarized, Pygments)

### 1.5 Others Observed

- **CodeWhale**: ~20 semantic tokens — `bg`, `bg_alt`, `surface`, `surface_raised`, `border`, `border_focus`, `text`, `text_muted`, `text_dim`, `selection_bg`, `selection_fg`, `primary`, `secondary`, `accent`, `accent_2`, `success`, `warning`, `error`, `error_bg`, `link`
- **Terminal.UI**: Semantic markup tokens `[primary]`, `[success]`, `[warning]`, `[error]`, `[accent]`, `[muted]`, `[disabled]`
- **Opaline (Rust)**: 26 core semantic tokens across `text.*`, `bg.*`, `accent.*`, `border.*`, `code.*`

---

## Part 2: Terminal UI Color Best Practices

### 2.1 Semantic Naming Over Color-Value Naming

The consensus is to name tokens by **purpose** (`text`, `surface`, `border`, `success`, `error`) rather than by color (`red-500`, `gray-200`). This supports dark/light theming at the semantic layer without touching primitive values.

### 2.2 Three-Tier Token Architecture

**Primitive → Semantic → Component**:
1. **Primitive tokens** — raw hex/RGB values (e.g., `#0d9488`)
2. **Semantic tokens** — map primitives to functional roles (e.g., `accent.primary`, `text.muted`, `border.focus`)
3. **Component tokens** — reference semantic tokens for specific widgets (e.g., `input.cursor`, `status_bar.bg`)

This is a common pattern across design systems (NYS Design System documents it as Primitive/Semantic/Theme). The accessible sources support this, though it's *a* common pattern rather than absolute consensus.

### 2.3 WCAG Contrast Requirements

- **WCAG 2.0 AA**: 4.5:1 for normal text, 3:1 for large text and UI components
- **Section 508** extends these requirements to all ICT including software
- **WCAG 1.4.1**: Color must not be the sole means of conveying information (pair with glyphs/labels)

> ⚠️ Note: Stripe's foundational article on accessible color systems is from 2019. While still valid, WCAG 2.2 (2024) has additional guidance not reflected there.

### 2.4 Token Count Guidance

Design systems commonly define 10-20 core semantic color tokens for general UIs. For multi-agent terminal UIs with role differentiation, 20-30 tokens is typical. The specific "15-30" figure was unsupported by the cited NY Design System source — treat it as an industry observation, not a sourced claim.

### 2.5 Accessibility Musts

- Respect `NO_COLOR` environment variable (no-color.org)
- Respect `TERM=dumb` for minimal terminals
- Offer "bare mode" output for screen-reader users (per ACM CHI CLI accessibility research)

---

## Part 3: NIKI Codebase Audit — Current State

### 3.1 Token Inventory (97 accessors)

| Category | Count | Examples |
|---|---|---|
| **Palette fields** | 32 | `bg`, `bg_elevated`, `border`, `fg`, `success`, `error`, `warning`, `accent`, `clay_orange`, 8 agent colors |
| **Mode-aware accessors** | 19 | `bg_color()`, `border_color()`, `fg_color()`, `success()`, etc. |
| **Chat tokens** | 8 | `primary()`, `claude()`, `shell()`, `role_user()`, `role_assistant()`, `role_system()`, `prompt_cursor()`, `text_dim()` |
| **Semantic aliases** | 14 | `text()`, `surface()`, `border()`, `header_style()`, etc. |
| **Compound styles** | 11 | `status_ok()`, `status_err()`, `block_border()`, `clay_accent()`, etc. |
| **Legacy uppercase** | 25+ | `BG()`, `FG()`, `GREEN()`, `AGENT_*()` |

### 3.2 Bugs Found

| File | Line | Issue | Fix |
|---|---|---|---|
| `state.rs` | 550, 554 | **Hardcoded `Color::Yellow` / `Color::DarkGray`** in revision notes | Use `theme::warning()` / `theme::text_dim()` |
| `code_block.rs` | 43 | **Hardcoded `"base16-ocean.dark"`** syntect theme | Switch syntect theme based on `ThemeMode` |

### 3.3 Gaps Found

| Gap | Impact | Recommendation |
|---|---|---|
| `markdown.rs` only applies 4/13 `MessageRenderConfig` fields | `success_color`, `error_color`, `warning_color` unused in markdown body | Add colored spans for `> [!NOTE]`/`> [!WARNING]` admonitions |
| Unused compound styles: `clay_accent`, `status_ok/err/warn`, `footer_style`, `block_border/active`, `dim_style`, `accent_style` | Dead code | Either wire into components or remove |
| `ThemeMode::Auto` → Dark fallback | No true terminal color detection | Implement OSC 4 probing or detect from `COLORFGBG` |

### 3.4 What's Done Well

- ✅ **Clean migration**: All new chat files use semantic accessors, no legacy uppercase aliases
- ✅ **Consistent `MessageRenderConfig` pattern**: Centralized config struct passed to all renderers
- ✅ **`NO_COLOR` respected**: Existing `no_color()` guard in palette accessors
- ✅ **Role color system**: 8 distinct agent colors mapped from theme palette
- ✅ **`Span::styled` with theme tokens**: No hardcoded hex in component renderers

---

## Part 4: Differentiation Strategy

### 4.1 The Competitive Color Map

| Agent | Primary Hue | Accent | Emotional Register |
|---|---|---|---|
| **Claude Code** | Blue/Purple | Warm orange/sand (`ansi:redBright`) | Warmth, humanity, calm |
| **Kimi Code** | Warm tones | (undocumented) | (undocumented) |
| **GitHub Copilot** | Green | Green | Familiar, Microsoft-aligned |
| **Aider** | Blue (#0088ff) | Green (#00cc00) inputs | Technical, functional |

### 4.2 Unoccupied Terminal Color Space

Based on competitor analysis, these hue ranges are NOT used by major AI coding assistants as primary identity:

| Hue | Hex Example | Vibe | Risks |
|---|---|---|---|
| **Teal/Cyan** | `#0d9488` | Modern, precise, fresh | Close to "info" blue |
| **Amber/Gold** | `#f59e0b` | Confident, distinctive | Close to "warning" |
| **Coral/Salmon** | `#f43f5e` | Energetic, unique | Close to "error" red |
| **Magenta/Hot Pink** | `#d946ef` | Bold, memorable | Can look unprofessional |
| **Deep Emerald** | `#059669` | Natural, calm | Close to Copilot green |

### 4.3 Perceptually Uniform Color Spaces

Stripe proved that **Oklch / CIELAB** color spaces enable both uniqueness and accessibility — pick vibrant, distinct hues while guaranteeing consistent perceived lightness across all colors. This solves the "yellow looks brighter than blue" problem in HSL.

---

## Part 5: Recommended Color Token Designs for NIKI

### Option A: "Warm Clay" — Confident Earth Tones

**Concept:** Clay/terracotta primary with cool teal secondary. Warm but distinct from Claude's orange.

| Token | Dark | Light | Usage |
|---|---|---|---|
| `accent.primary` | `#E07A5F` | `#C0604F` | Brand, focus, interactive |
| `accent.secondary` | `#81B29A` | `#5F8C7A` | Secondary actions |
| `bg.base` | `#1A1513` | `#FAF7F4` | Main background |
| `bg.elevated` | `#2A221F` | `#FFFFFF` | Cards, modals |
| `text.primary` | `#F4F1DE` | `#2A221F` | Body text |
| `text.muted` | `#A89F94` | `#7A7068` | Descriptions |
| `border.base` | `#4A3F3A` | `#D4CFC8` | Borders |
| `border.focus` | `#E07A5F` | `#C0604F` | Focused element |
| `role.user` | `#F2CC8F` | `#B8860B` | User messages |
| `role.assistant` | `#81B29A` | `#5F8C7A` | Assistant messages |
| `status.success` | `#81B29A` | `#5F8C7A` | Checkmarks |
| `status.error` | `#E07A5F` | `#C0604F` | Error messages |
| `status.warning` | `#F2CC8F` | `#B8860B` | Warnings |

**Pros:** Distinctive, warm but not "Claude-like", reads as confident
**Cons:** `error` and `primary` share hue — may confuse at a glance

---

### Option B: "Midnight Teal" — Cool Precision *(RECOMMENDED)*

**Concept:** Deep teal primary with warm amber accents. Occupies the cool-toned space Claude avoids. Reads as "precision engineering."

| Token | Dark | Light | Usage |
|---|---|---|---|
| `accent.primary` | `#0d9488` | `#0f766e` | Brand, focus, interactive |
| `accent.secondary` | `#f59e0b` | `#d97706` | Secondary, highlights |
| `bg.base` | `#0d1117` | `#f8fafc` | Main background |
| `bg.elevated` | `#161b22` | `#ffffff` | Cards, modals |
| `bg.surface` | `#1c2128` | `#f1f5f9` | Panels |
| `text.primary` | `#e6edf3` | `#1e293b` | Body text |
| `text.strong` | `#f0f6fc` | `#0f172a` | Bold/emphasized |
| `text.dim` | `#8b949e` | `#64748b` | Descriptions |
| `text.muted` | `#6e7681` | `#94a3b8` | Counters, URLs |
| `border.base` | `#30363d` | `#cbd5e1` | Borders |
| `border.focus` | `#0d9488` | `#0f766e` | Focused element |
| `role.user` | `#f59e0b` | `#d97706` | User messages |
| `role.assistant` | `#0d9488` | `#0f766e` | Assistant messages |
| `status.success` | `#34d399` | `#059669` | Checkmarks |
| `status.error` | `#f87171` | `#dc2626` | Error messages |
| `status.warning` | `#fbbf24` | `#d97706` | Warnings |
| `spinner` | `#0d9488` | `#0f766e` | Animation |
| `prompt.cursor` | `#0d9488` (reversed) | `#0f766e` (reversed) | Input cursor |

**Pros:**
- ✅ Teal primary = unoccupied space (Claude=blue/purple, Copilot=green, Kimi=warm)
- ✅ Amber accent = strong contrast to teal, reads as "premium"
- ✅ Cool bg + warm text = comfortable long sessions
- ✅ All status colors distinct from brand colors (no hue confusion)
- ✅ Teal works beautifully in both dark and light modes

**Cons:**
- ⚠️ Teal can read as "info" blue in some contexts — mitigated by saturation

---

### Option C: "Arctic Slate" — Monochromatic Minimal

**Concept:** Near-monochromatic gray with a single bold accent. Maximum differentiation through restraint.

| Token | Dark | Light | Usage |
|---|---|---|---|
| `accent.primary` | `#a78bfa` | `#7c3aed` | Brand, focus, interactive |
| `bg.base` | `#0f0f10` | `#fafafa` | Main background |
| `bg.elevated` | `#1a1a1c` | `#ffffff` | Cards, modals |
| `text.primary` | `#e4e4e7` | `#18181b` | Body text |
| `text.dim` | `#a1a1aa` | `#71717a` | Descriptions |
| `border.base` | `#3f3f46` | `#d4d4d8` | Borders |
| `border.focus` | `#a78bfa` | `#7c3aed` | Focused element |

**Pros:** Maximum differentiation, focuses attention on accent-colored elements
**Cons:** Can feel cold/impersonal, accent-purple edges close to Claude

---

## Part 6: Implementation Recommendations

### 6.1 Fix Immediate Bugs

```rust
// state.rs:550,554 — Replace hardcoded colors:
// BEFORE:
Color::Yellow  // revision round
Color::DarkGray  // issues

// AFTER:
crate::display::theme::warning()   // revision round
crate::display::theme::text_dim()  // issues
```

```rust
// code_block.rs:43 — Theme-aware syntect:
let syntect_theme_name = if crate::display::theme::is_light() {
    "base16-ocean.light"
} else {
    "base16-ocean.dark"
};
```

### 6.2 Adopt Three-Tier Architecture

Refactor theme.rs to follow Primitive → Semantic → Component:

```rust
// Tier 1: Primitives (raw hex — private to theme module)
mod primitives {
    pub const TEAL_500: u32 = 0x0d9488;
    pub const TEAL_600: u32 = 0x0f766e;
    pub const AMBER_500: u32 = 0xf59e0b;
    // ...
}

// Tier 2: Semantic (mode-aware, public)
pub fn accent_primary() -> Color {
    let rgb = match current_mode() {
        ThemeMode::Light => primitives::TEAL_600,
        _ => primitives::TEAL_500,
    };
    Color::Rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

// Tier 3: Component (used by specific widgets)
pub fn input_cursor() -> Style {
    Style::default().bg(accent_primary()).fg(bg_base())
}
```

### 6.3 Kill or Wire Dead Code

| Dead Code | Action |
|---|---|
| `clay_accent()` | Wire into status bar or remove |
| `status_ok()` / `status_err()` / `status_warn()` | Wire into page content renderers |
| `footer_style()` | Wire into layout footer |
| `block_border()` / `block_border_active()` | Wire into modal borders |
| `dim_style()` | Wire into dimmed overlay backgrounds |
| `accent_style(color)` | Generic helper — keep |

### 6.4 Add Missing Tokens for Chat Interface

```rust
// New tokens needed for full chat interface:
pub fn text_strong() -> Color { ... }        // Bold/emphasized text
pub fn text_muted() -> Color { ... }        // Counters, URLs
pub fn prompt_border() -> Color { ... }     // Input box border
pub fn prompt_cursor() -> Style { ... }     // Reversed cursor
pub fn autocomplete_bg() -> Color { ... }   // Autocomplete dropdown bg
pub fn scrollbar_thumb() -> Color { ... }   // Scrollbar (future)
pub fn shimmer(color: Color) -> Color { ... } // Lighter variant for animation
```

---

## Part 7: Disagreements & Open Questions

### Disagreements

1. **Best-practice sources**: Three sources returned 403 (designsystemscollective.com, stackexchange.com, medium.com/Design-Bootcamp). The accessible sources still support semantic naming and three-tier architecture, but these specific claims would be stronger with direct access.

2. **Token count**: The "15-30 tokens" figure was unsupported by the NY Design System source. Treat as industry observation rather than sourced fact.

3. **Copilot theme modes**: The research agent listed `dark` and `light` as Copilot CLI modes, but the source only documents `auto`, `default`, `dim`, `high-contrast`, `colorblind`. The dark/light modes may exist but weren't confirmed.

### Open Questions

1. **Terminal color detection**: How should `ThemeMode::Auto` actually detect the terminal's color scheme? OSC 4? `COLORFGBG` env var? Kitty's `background_opacity`?
2. **Perceptual uniformity**: Should NIKI adopt Oklch/CIELAB for palette generation, or is manual RGB selection sufficient?
3. **Contrast checking**: Is there a tool that validates terminal color pairs for WCAG compliance (not just web colors)?
4. **User testing**: Which palette (A, B, or C) do actual NIKI users prefer? A/B testing needed.

---

## Part 8: Source List

| # | Source | URL | Status |
|---|---|---|---|
| 1 | Claude Code Terminal Config Docs | https://code.claude.com/docs/en/terminal-config | ✅ 200 |
| 2 | GitHub Copilot CLI Changelog | https://github.blog/changelog/2026-06-23-copilot-cli-new-terminal-interface-is-generally-available/ | ✅ 200 |
| 3 | Copilot CLI Issue #2830 (custom themes) | https://github.com/github/copilot-cli/issues/2830 | ✅ 200 |
| 4 | Copilot CLI Issue #3866 (hardcoded dim) | https://github.com/github/copilot-cli/issues/3866 | ✅ 200 |
| 5 | Kimi CLI Issue #1981 (themes) | https://github.com/MoonshotAI/kimi-cli/issues/1981 | ✅ 200 |
| 6 | Aider Config Options | https://aider.chat/docs/config/options.html | ✅ 200 |
| 7 | CodeWhale Issue #2017 | https://github.com/hmbown/codewhale/issues/2017 | ✅ 200 |
| 8 | Terminal Color Standards | https://github.com/termstandard/colors | ✅ 200 |
| 9 | Catppuccin | https://github.com/catppuccin/catppuccin | ✅ 200 |
| 10 | Rosé Pine | https://github.com/rose-pine/rose-pine-theme | ✅ 200 |
| 11 | Dimidium | https://github.com/dofuuz/dimidium | ✅ 200 |
| 12 | Stripe Accessible Color Systems | https://stripe.com/blog/accessible-color-systems | ✅ 200 (2019) |
| 13 | Ham Vocke Blog | https://hamvocke.com/blog/lets-create-a-terminal-color-scheme/ | ✅ 200 |
| 14 | Section 508 Color Accessibility | https://www.section508.gov/create/making-color-usage-accessible/ | ✅ 200 |
| 15 | Design Bootcamp (Claude branding) | https://medium.com/design-bootcamp/the-quiet-genius-of-claudes-branding-less-hype-more-humanity-f4f5567051cc | ❌ 403 |
| 16 | Palo Alto (terminal themes) | https://dev.to/palo_alto_ai/four-themes-for-a-terminal-you-read-more-than-you-syntax-highlight-58kd | ✅ 200 |
| 17 | Design System Collective | https://www.designsystemscollective.com/color-token-naming-what-works-what-fails-the-best-approach-for-your-design-system-50f844d25f01 | ❌ 403 |
| 18 | StackExchange Design Tokens | https://ux.stackexchange.com/questions/153154/which-level-of-design-tokens-should-be-built-with-dark-mode-colours | ❌ 403 |
| 19 | NY Design System | https://designsystem.ny.gov/foundations/tokens/ | ✅ 200 |
| 20 | Afixt CLI Accessibility | https://afixt.com/accessible-by-design-improving-command-line-interfaces-for-all-users/ | ✅ 200 |
| 21 | FourZeroThree Semantic Tokens | https://www.fourzerothree.in/p/semantic-colour-tokens-in-action | ✅ 200 |
| 22 | Always Twisted | https://www.alwaystwisted.com/articles/a-design-tokens-workflow-part-7 | ✅ 200 |
