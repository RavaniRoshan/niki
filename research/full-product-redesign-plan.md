# Niki TUI — Full Product Redesign Plan (Product-Site Token Alignment)

**Status:** Plan (research complete, not yet implemented)
**Date:** 2026-08-04
**Method:** Deep research — 4 design subagents (color, typography/layout, component map, theme toggle) + 1 adversarial verification subagent. Product-site tokens are primary source (extracted from accessible CSS bundle). All file:line claims were spot-checked by the verifier and confirmed real.
**Scope decision (user):** TUI adopts **both light + dark themes with a toggle** (mirroring the product's `/theme`), full token alignment.

---

## Executive Summary

The Niki product-site ships a complete design-token system (light + dark) that the TUI currently ignores — the TUI hardcodes a single dark Claude-Code-derived palette in `src/display/theme.rs`, has no theme switch, no status line, no comparison view, and several half-finished components (dead TipsBanner, never-populated branch name, Config page without focus ring). This plan aligns the TUI to the product tokens across **colors, spacing, typography semantics, layout responsiveness, borders, and elevation**, adds a **light/dark/auto theme toggle** (config + persistence + Ctrl+T + palette command + OSC 11 auto-detect), and closes the product-parity gaps that a terminal-first, auditability-first design requires.

The redesign is deliberately **low-churn**: the core change is converting ~11 mode-dependent color constants to mode-aware accessors backed by an `AtomicU8`, sweeping all 361 `theme::*` call sites mechanically, and adding a `UiConfig.theme` field. Everything else is additive (new widgets/components) or surgical (per-page layout tweaks).

---

## Part 1 — Design Tokens → TUI Theme System

### 1.1 Color mapping (VERIFIED: composites checked by verifier)

Semantic name → Light / Dark. Source column: product token = confirmed; `*` = synthesized (no token exists; verifier-flagged for sign-off).

| Semantic | LIGHT | DARK | Source |
|---|---|---|---|
| `BG` / background / surface | `#fdfcfc` | `#121111` | product |
| `BG_DEEP` (deepest) | `#201d1d` (surface-dark) | `#000000` (ink-deep) | product |
| `BG_ELEVATED` / surface-card | `#f1eeee` | `#1f1c1c` | product |
| `BG_HIGHLIGHT` / surface-soft | `#f8f7f7` | `#1c1a1a` | product |
| `BORDER` (composited solid) | `#E0DEDE` | `#313030` | composite (VERIFIED math) |
| `BORDER_ACTIVE` / accent | `#007aff` | `#007aff` | product |
| `BORDER_DIM` | `#EFEDED` | `#20201F` | composite |
| `FG` / text | `#201d1d` | `#fdfcfc` | product |
| `FG_DIM` / text-muted | `#646262` | `#a7a4a4` | product |
| `FG_BRIGHT` (headings) | `#0f0000` (ink-deep) | `#ffffff` | product |
| `FG_SUBTLE` / ash | `#9a9898` | `#8b8888` | product |
| `SUCCESS` | `#1e8e3e` *(darker for light bg — see 1.2)* | `#30d158` | product + synthesized |
| `ERROR` | `#c62828` *(darker for light bg)* | `#ff3b30` | product + synthesized |
| `WARNING` | `#a16207` *(darker for light bg)* | `#ff9f0a` | product + synthesized |
| `ACCENT` (blue, interactive) | `#007aff` | `#007aff` | product |
| `CLAY_ORANGE` (brand) | `#d77757` | `#d77757` | `*` (brand keep) |
| `CYAN` | `#0e7490` | `#0891b2` | `*` |
| `PURPLE` | `#6d28d9` | `#af87ff` | `*` |
| `SELECTION_BG` | `#dfecfc` (accent 12% tint) | `#264f78` | `*` |
| `DIFF_ADD_BG` | `#edf9ef` (success 8% tint) | `#225c2b` | `*` (VERIFIED math) |
| `DIFF_DEL_BG` | `#fdedec` (error 8% tint) | `#7a2936` | `*` (VERIFIED math) |
| `DIFF_ADD_FG` / `DEL_FG` | `#1e8e3e` / `#c62828` | `#38a660` / `#b3596b` | `*` |
| `DIFF_HUNK` | `#a16207` | `#ffc107` | `*` |
| Agent role colors | darken for light: red `#b3261e`, blue `#2563eb`, green `#15803d`, yellow `#b45309`, purple `#6d28d9`, orange `#c2410c`, pink `#be185d`, cyan `#0e7490` | keep `#dc2626 #6a9bcc #16a34a #ca8a04 #827dbd #d97757 #c46686 #0891b2` | `*` |

### 1.2 VERIFIER-DRIVEN FIX — accents must be theme-aware (contradiction resolved)

The naive "keep GREEN/RED/AMBER and role colors as theme-agnostic consts" is **wrong for a light theme**: `#30d158` on `#fdfcfc` ≈ 1.96:1 contrast (fails even large-text 3:1); `#ff3b30` ≈ 3.46:1. The product keeps these shared **only because its terminal panel is dark (`#201d1d`)**. A light TUI cannot share them.

**Resolution:** the entire accent set (success/error/warning + diff fg + role colors) becomes per-theme pairs. Light uses darker variants (above); dark keeps the product values. Product's tinted-bg + dark-fg pattern applies for light-mode diff backgrounds.

### 1.3 Architecture (recommended by T1, hardened by verifier)

- `ThemeMode` enum (`Auto|Dark|Light`) + `Palette` struct with `LIGHT`/`DARK` consts + `static MODE: AtomicU8` + `set_mode()`/`current()`. **No thread_local** — CLI threads must see the same mode; `Color` is Copy.
- **ALL 11 mode-dependent constants** (verifier correction: the list is 11, not 10 — includes `BG_DEEP` and `BG_HIGHLIGHT`) convert `pub const X: Color` → `pub fn x() -> Color`, reading `current()`. Keep `no_color()`/`fg()` guard; extend it so the **background path is guarded too** (verifier-confirmed bug: `tui.rs:330` paints dark `BG` under `NO_COLOR`).
- **Sweep all 361 `theme::*` call sites** (verifier's independent count; not the researchers' ~328) mechanically: `theme::BG`→`theme::bg()`, `theme::FG`→`theme::fg()`, etc. Every surrounding `Style::default().fg(...)` stays byte-identical.
- Legacy `console::Style` `Theme` (CLI streaming: agent_stream, pipeline_status, completion, banner): **leave as-is** (16-color ANSI, acceptable in both shells). Optional follow-up: swap only `diff_add`/`diff_remove` by reading the global mode.
- Add product semantic aliases: `text()`, `text_body()`, `text_muted()`, `border()`, `border_strong()`, `accent()`, `success()`, `warning()`, `error()`, `surface()`, `surface_soft()`, `surface_card()`, `surface_dark()`, `surface_dark_elevated()`, `ink_deep()`, `ash()`, `charcoal()`, `stone()`, `on_dark()`.

---

## Part 2 — Theme Toggle (config + persistence + UX)

All claims verified: config load global→project merge (`types.rs:616-640`), no write path exists, Ctrl+T unused, `tui.rs:330` is the single global-bg hook, crossterm 0.28.1 (transitive via ratatui) has no OSC-11 API.

### 2.1 Config
- `src/config/types.rs`: add `ThemePreference` enum (`Auto|Dark|Light`, `#[serde(rename_all="lowercase")]`, default `Auto`) — name it `ThemePreference` NOT `Theme` (collision with existing `console::Style Theme` struct at `theme.rs:222`).
- Add `#[serde(default)] pub theme: ThemePreference` to `UiConfig` (types.rs:392) + granular `merge()` entry (mirror the `ui.tips` block at `types.rs:720-727`). Backwards-compatible (old TOML files default to Auto).

### 2.2 Persistence (verifier-confirmed: no write path exists)
- New `NikiConfig::save_theme(&self, ThemePreference)` that patches `~/.config/niki/niki.toml` via **`toml::Value` mutation** (load → `v["ui"]["theme"] = …` → write whole Value back, atomic temp+rename). **Never** `toml::to_string(&NikiConfig)` — that serializes all provider/agent defaults and clobbers user config.
- Persist to **global** config (theme is a user preference, not per-project). Fall back to raw-field write if global file absent.

### 2.3 Auto-detection (robust sequence, cached at startup)
1. `NO_COLOR` or `TERM=dumb` → mode none (all colors `Reset`; **also fixes the bg leak**).
2. `$COLORFGBG` → parse bg field; `<=7` dark, `>=8` light.
3. OSC 11 query (`\x1b]11;?\x1b\\`, ~120ms read timeout after `enable_raw_mode`/alternate screen, before the poll loop) → parse `rgb:RRRR/GGGG/BBBB`, luminance threshold.
4. Silent terminal → fall back to default (recommend dark to match current look; explicit user pref always wins).

### 2.4 UX
- **Ctrl+T** (verified globally unused) toggles `dark → light → auto → dark`, saves, sets dirty.
- **Palette item** `theme: cycle` (shortcut `t` — verified unused) → `PaletteAction::CycleTheme`, handled in `execute_selected` (`command_palette.rs:83-104`).
- No `/theme` input mode needed; palette entry is the discoverable command.

---

## Part 3 — Typography, Spacing, Layout, Border Translation

### 3.1 Typography (terminal constraints)
- Font: terminal is already monospace (product's own stack). No action.
- Weight: 500 and 700 both → `Modifier::BOLD`; differentiate 700-level (headings/wordmark) via `FG_BRIGHT` + uppercase section labels. **Policy change: current theme over-bolds; 500=BOLD/body=plain is the new ladder.**
- Sizes (fixed at one cell): display→ASCII logo; body→default; caption/small/key→`Modifier::DIM` (+ `[k]` bracket pattern already in use).
- Line-height → blank-line rhythm only: terminal-log 1.7 → tightest packing (blank line between stage transcripts, never within).

### 3.2 Spacing (4px base → cells; ASSUMPTION, verifier-flagged: terminal cells ≈ 7–9px wide, sub-cell values round up)
| token | px | TUI |
|---|---|---|
| space-1 | 4 | 1-col indent |
| space-2 | 8 | 2 cols / 0–1 row |
| space-3 | 12 | 3 cols / 1 row (card T/B pad) |
| space-4 | 16 | `Padding::horizontal(4).vertical(1)` |
| space-5 (gutter) | 24 | 2–3 cols page margin (terminals are width-poor) |
| space-6 | 32 | 2 rows between regions |
| space-7 | 48 | 3 rows |
| space-section | 96/64/48 | 2–3 rows between page regions |

### 3.3 Responsive (per verifier: 850/8=106, 768/8=96, 640/8=80 — use ≈104/80, document assumption)
| product rule | TUI rule | width threshold |
|---|---|---|
| pipeline 4→2→1 | stage grid 4-up → 2-up → stacked | ≥104 / ≥80 |
| split 2→1 | artifacts 40/60 → stacked | <96–100 |
| table 3col→block | history/cost table → key-value rows | <80 |
| pricing 2→1 | cost bars stack | <96 |

Branch on `area.width` before building `Layout::constraints`; use `Constraint::Percentage`/`Ratio`.

### 3.4 Shape/border/elevation
- **One consistent choice: `Borders::ALL` plain rectangles everywhere** (matches product radius-0 TUI frame/header/tables/pipeline; 4px radius is not representable; `Rounded` reads as "soft pill" — avoid except one optional overlay exception).
- Hairline 1px → thin border; border-strong → `theme::fg_dim()`; focus ring → `BORDER_ACTIVE` (palette + focused input).
- Border inventory: containers/cards/tables get borders; list rows borderless (marker + selected-BG); region divides use `─` separators (existing pattern).
- Elevation = BG contrast only (no shadow/lift representable): palette → `BG_ELEVATED`, modal → `BG_ELEVATED` + border, hover/selected → `BG_HIGHLIGHT`.

---

## Part 4 — Component-to-Page Redesign (verifier spot-checked all file:line evidence)

### 4.1 Confirmed current-state facts (verifier-verified)
- **No status line** (`tui.rs:333` comment; `branch_name` init `mod.rs:199`, never populated → `run.rs:215-219` always shows placeholder `niki/xxxxx`).
- **TipsBanner is dead code** (`mod.rs:173` stores it; `render()` never called; `tips.rs:103` defines it).
- **Planner role uses `BLUE #6A9BCC`** (`theme.rs:113`) which visually collides with the product accent `#007aff`. **Resolution (verifier-recommended): keep role colors; move focus/interactive styling to `#007aff` instead of recoloring Planner.**
- **No comparison-table page** exists (closest: Cost 5-col, History 4-col).
- **Config page has no focus ring** (selected_field cycles but nothing highlights it).
- **Pipeline page renders an ASCII flowchart**, not the product's repeat(4,1fr) card grid.

### 4.2 Per-page redesign spec (both themes)
- **Run (terminal panel):** `[task 3 · cmd+stream · transcript · results · status-line]`. Borderless pipe output on `surface-dark` bg; elevated prompt row (`#302c2c`-equivalent); task tags → bordered chips; results line = audit-trail strip; footer evolves into status line. Add jump-to-bottom + "N new" floating control.
- **Pipeline:** replace flowchart+MODELS with repeat(4,1fr) cards (muted role line / bold name / desc / status glyph); bottom band = MODELS list with `▸` accent selection.
- **Agents:** tabs as segmented chips (selected = surface-soft + accent inset); meta line with role glyph + color; collapsible tool/agent rows (`▶/▼`).
- **Diff:** keep line-numbered hunks; **make diff bg/fg theme-aware per §1.1**; `+` green, `-` red, `@@` warning, file header accent.
- **Verdict:** verdict tile border = state color (green/red/amber/dim) + glyph.
- **Artifacts:** 40/60 split (stack <96 cols); right preview switches diff/report/patch/JSON per selected file (`h/l`).
- **Config:** bordered section groups; **real focus**: accent `▸` gutter marker + full-width `BG_HIGHLIGHT` row + `#007aff` ring on selected field; checkboxes styled success when on.
- **History:** bordered table; real `.niki/` dir scan instead of hardcoded sample rows; verdict in state color + glyph.
- **Cost:** bordered table + per-agent bars; `$` values success; running-total chip (green/amber).
- **Help:** convert flat list to collapsible sections (`▶/▼`).
- **TestLog:** bordered `TEST OUTPUT` box; keep existing success/muted/warning color semantics; add jump-to-bottom.
- **Logo:** keep FIGlet `big` as home hero (already uses `theme::FG` ✓); add compact variant for sub-pages.
- **Command palette:** `▸` cursor accent; selected row `BG_HIGHLIGHT`; add substring typeahead filter (currently shortcut-match only, `command_palette.rs:71-78`).
- **Modal:** Confirm border = accent; Error border = error red; add dim scrim overlay per product layering.

### 4.3 New components (product parity gaps)
1. **Status line** (product "footer meta") — `tui.rs` under content, `Constraint::Length(1)`: model(s), task dir, git branch, context-usage, cost, duration. **Also fixes the never-populated `branch_name`** — requires a new `DisplayEvent::Branch`/`Model`/`Context` or reading `config`/`project_path` at render.
2. **Jump-to-bottom + "N new"** floating chip on Run and TestLog (renders when `scroll_offset < max_scroll`).
3. **Comparison table page** — new `PageId::Compare` (capability / single-agent / Niki, `✓`/`—` cells) — placement decision (new binding vs replace a low-value page).
4. **Collapsible agent/tool rows** on Agents.
5. **Per-file preview switching** in Artifacts.
6. **Home hero tagline** (optional, muted caption under logo).

---

## Part 5 — Implementation Order & Verification

### Phase 1 — Theme foundation (everything depends on it)
1. `theme.rs`: `ThemeMode` + `Palette` + `AtomicU8` + accessors (11 surfaces) + no_color-guarded bg. → verify: unit tests assert `current()` dark/light values.
2. Mechanical sweep of all 361 `theme::*` sites. → verify: `cargo check` clean, `cargo test` (174 unit + 123 integration) green.
3. `UiConfig.theme` + `merge()` + `ThemePreference`. → verify: old TOML files load.
4. `save_theme()` toml::Value patch + `Ctrl+T` + palette `t` action + auto-detect sequence. → verify: toggle flips colors live, persists, survives restart.
5. Fix NO_COLOR bg leak. → verify: NO_COLOR → no bg paint.

### Phase 2 — Per-page alignment (Part 4.2)
- Diff theme-aware colors, Pipeline card grid, Config focus ring, History real scan, Help accordion, TestLog/Run jump-to-bottom, modal scrim + accent borders, palette typeahead. → verify: `cargo test`; manual visual per page in both themes.

### Phase 3 — New components (Part 4.3)
- Status line + branch_name event wiring, comparison page, collapsible agent rows, artifacts preview switching, hero tagline. → verify: `cargo test`; screenshot diff per page in both themes.

### Phase 4 — Polish
- Responsive collapse thresholds (§3.3), spacing pass (§3.2), border inventory pass (§3.4), logo compact variant. → verify: visual regression at 3 widths.

---

## Part 6 — Open Questions / Explicit Limitations (from verification)

Carried as stated limitations (not silently dropped):
1. **Synthesized colors** (purple/cyan/clay/selection/diff-light, agent-role light variants) have **no product-token basis** — they are labeled `*` and need design sign-off.
2. **Dark diff `#225c2b`/`#7a2936` are carry-overs from Claude Code**, not product tokens. Alternative: derive dark diffs from the product's own 8%-color-mix recipe for consistency.
3. **Cell-width assumption 7–9px** (not 8px exact) — responsive thresholds (≈104/80 cols) are approximate; verifier flagged 850/8=106, not 104. Document as assumption range.
4. **`NO_COLOR` scope conflict** (cross-subagent contradiction A): T1/T4 recommend honoring NO_COLOR TUI-wide (fix bg leak); the ClaudeCode research notes Claude Code scopes NO_COLOR to subprocesses only. **Product decision needed:** honor TUI-wide (accessible default) vs CLI-output-only (mirror Claude Code). Recommended: TUI-wide honor.
5. **Dark-mode default vs Auto fallback:** product site is light-first, current TUI is dark. Recommended: keep dark as the fallback for unresponsive terminals, but `Auto` should follow the terminal when it answers OSC 11.
6. **Crossterm has no OSC-11 API** (0.28.1 transitive) — auto-detect needs raw byte handling; consider the `osc11`/`crossterm_query` crate or a tiny helper.
7. **TipsBanner** is dead code: wire into the status line (rotating context tip) or delete — product-look decision.
8. **Comparison-page placement** (new binding vs replace a page) and **Planner-vs-accent** color interplay (resolution above: keep role colors, accent is interaction-only) are open product calls.

---

## Sources
- Product-site design tokens (primary source): `research/tmp/product-site-design-tokens.md` (extracted from accessible CSS bundle + computed styles)
- Existing research: `research/claude-code-tui-visual-quality.md`, `research/coding-agent-structured-output-architecture.md`
- Niki code cited with file:line throughout (verified by adversarial verifier): `src/display/theme.rs`, `src/display/tui.rs`, `src/display/pages/*.rs`, `src/config/types.rs`, `src/display/command_palette.rs`, `src/display/modal.rs`, `src/display/logo.rs`
