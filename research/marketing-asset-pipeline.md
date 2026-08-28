# NIKI Marketing Asset Pipeline — Deep Research Report

**Date:** 2026-08-27
**Product:** NIKI — hermetic multi-agent coding CLI (Rust, ratatui TUI)
**Repo:** https://github.com/RavaniRoshan/niki

---

## Executive Summary

NIKI has **4 media files** for a product with **14 distinct views**, **6 modal overlays**, and **12+ transient states**. The existing `demo.gif` is a Python-simulated recording that predates Phases 1–5 of TUI development and shows only the Chat view. This report maps every screen that needs marketing assets, identifies the responsive flaws that will break screenshots, specifies exact dimensions/formats per platform, and prescribes a complete VHS-driven workflow to generate them deterministically from the real binary.

---

## 1. Complete Screen Inventory

### 1.1 Standard Pages (12, via `Page` trait)

| # | Page | File | Hero Marketing State | Current Asset |
|---|------|------|---------------------|---------------|
| 1 | **Run** | `src/display/pages/run.rs` | Live pipeline streaming: task card + animated Planner/Coder/Tester/Reviewer spinners + "Recent tool cards" footer + "N new" chip | ❌ None |
| 2 | **Pipeline** | `src/display/pages/pipeline.rs` | 4-up card grid (≥104 cols) with role-colored stage cards + MODELS panel | ❌ None |
| 3 | **Agents** | `src/display/pages/agents.rs` | Tabbed per-stage view with metadata bar (status · elapsed · tokens · cost) + transcript pane | ❌ None |
| 4 | **Diff** | `src/display/pages/diff.rs` | Unified diff with line numbers, word-level highlighting, hunk clustering, file sidebar (wide) | ❌ None |
| 5 | **Verdict** | `src/display/pages/verdict.rs` | Large verdict tile (A P P R O V E D / F A I L E D) + scrollable REPORT pane | ❌ None |
| 6 | **Cost** | `src/display/pages/cost.rs` | COST BREAKDOWN table + PER-AGENT horizontal bar chart | ❌ None |
| 7 | **Artifacts** | `src/display/pages/artifacts.rs` | Split file-tree + preview pane with diff coloring | ❌ None |
| 8 | **History** | `src/display/pages/history.rs` | PAST RUNS table (ID/TASK/VERDICT/WHEN/BRANCH) with verdict-colored rows | ❌ None |
| 9 | **Config** | `src/display/pages/config.rs` | niki.toml form editor (GENERAL/AGENTS/SANDBOX/PIPELINE/SECURITY/THEME sections) | ❌ None |
| 10 | **Help** | `src/display/pages/help.rs` | Collapsible keybinding sections (GLOBAL/PAGES/RUN/DIFF) | ❌ None |
| 11 | **TestLog** | `src/display/pages/test_log.rs` | TEST OUTPUT pane with test-result coloring | ❌ None |
| 12 | **Chat** | `src/display/pages/chat.rs` | Conversational stream with progressive disclosure, tool cards, markdown streaming, input box | ✅ demo.gif (stale) |

### 1.2 Special Views (2, custom render)

| # | View | File | Hero State | Asset |
|---|------|------|-----------|-------|
| 13 | **Fleet** | `src/display/pages/fleet.rs` | Mission-control grid with status icons + attention flags + progress + cost | ❌ None |
| 14 | **Session** | `src/display/pages/session.rs` | 7-tab mission investigation (Conversation/Agents/Tools/Diff/Tests/Approvals/Evidence) | ❌ None |

### 1.3 Modal Overlays (6)

| # | Overlay | File | Hero State | Asset |
|---|---------|------|-----------|-------|
| 15 | **Permission modal** | `src/display/components/permission.rs` | Centered popup: "$ command" → separator → description → scope → 4 options | ❌ None |
| 16 | **Confirm/Error modal** | `src/display/modal.rs` | Dimmed scrim + "Quit NIKI?" + [Enter]/[Esc] | ❌ None |
| 17 | **Command palette** | `src/display/command_palette.rs` | Centered "Commands" popup with ▸ prefix + [shortcut] badges | ❌ None |
| 18 | **Tool detail modal** | `src/display/components/tool_detail.rs` | Full-screen 80%×70% modal with scrollable output + copy | ❌ None |
| 19 | **Onboarding wizard** | `src/display/onboarding.rs` | 5-step modal (Welcome → Auth → Terminal → Telemetry → Help) | ❌ None |
| 20 | **Help overlay** | `src/display/help_overlay.rs` | Centered 56-wide "Keybindings" popup | ❌ None |

### 1.4 Persistent Chrome & Transient States (9)

| # | State | Where | Marketing Value |
|---|-------|-------|-----------------|
| 21 | **Status bar** | `status_bar.rs` | Model badge + current tool indicator + shortcuts + branch + cost + ctx gauge + permission badge |
| 22 | **Input box** | `input_box.rs` | Rounded capsule + mode indicator + pill badges + cursor + streaming-disabled state |
| 23 | **Spinner animation** | `spinner.rs` | Moon/Dots/Bars/Arrow — drives Run-page stage animation |
| 24 | **Tool card** | `tool_card.rs` | Collapsible card: status glyph + tool name + summary + output preview + timing |
| 25 | **Slash command menu** | `command_menu.rs` | Fuzzy-filtered overlay in Command mode |
| 26 | **@ Autocomplete** | `autocomplete.rs` | File-path completion in Insert mode |
| 27 | **Tips banner** | `tips.rs` | Rotating tips (39 curated strings) |
| 28 | **Context gauge** | `status_bar.rs` | Claude Code-style ▓/░ bar showing context-window utilization |
| 29 | **"N new" chip** | `run.rs` | Floating pill when scrolled up during live stream |

**Total: 29 distinct visual states. Currently captured: 1 (Chat, stale).**

---

## 2. Responsive Flaws That Will Break Screenshots

Verified by reading the source. These must be fixed before capturing marketing assets at any terminal size other than 120×30.

### 2.1 Hardcoded Widths (status_bar.rs)

The status bar has **13+ magic-width branches** (`width < 10`, `>= 40`, `>= 80`, `>= 50`, `>= 65`, `>= 45`, `>= 55`) that gate model badge, shortcuts, branch, cost, context gauge, and queued counters. Narrow terminals silently drop these without the user knowing what's hidden.

**Fix:** Use a priority-based greedy layout (already partially implemented) with a clear "more" indicator when items are hidden.

### 2.2 Hardcoded mode_len (input_box.rs)

`handle_click` hardcodes `mode_len` per mode (`Shell=7`, `Command=5`, `Insert=7`) separately from `render_input_box`. The two can drift and mis-place the cursor on click.

**Fix:** Extract a shared `mode_indicator_len(mode)` function.

### 2.3 Fixed Vertical Constraints (run.rs)

Uses all fixed vertical constraints (`Length(3), Length(1), Min(4), Length(1), Length(2), Length(1)`). On a short terminal the pipeline output `Min(4)` collapses and the page shows only chrome.

**Fix:** Replace fixed `Length` with `Min` + `Percentage` constraints.

### 2.4 Sidebar Breakpoint (diff.rs)

Toggles file sidebar on `area.width > 80` with a 25/75 percentage split. At exactly 80 cols the sidebar vanishes with no transition.

**Fix:** Use `Percentage(25)` with a `Min` constraint so the sidebar degrades gracefully.

### 2.5 Hardcoded 80 in tool_card hit-test (chat.rs) — FIXED

Was: `tool_card_height(card, 80)` — a stale constant that wouldn't match the actual rendered width.
Now: `tool_card_height(card, chat_width)` using the live `state.chat_width`.

### 2.6 Working-Tree Line Offset (run.rs)

Renders the working-tree line at hardcoded `y: chunks[4].y + 1` — relies on `chunks[4]` being exactly 2 rows tall.

**Fix:** Use a vertical layout with explicit constraints instead of absolute positioning.

### 2.7 Missing Responsive Tests

`tests/visual_layout_check.rs` only tests `(80,24)`, `(120,40)`, `(200,50)`. No test covers:
- Very narrow terminals (40-col)
- Very short terminals (12-row)
- Resize reflow of `chat_width`
- Fleet/Pipeline/Config pages at multiple sizes

---

## 3. Tool Stack for Asset Generation

### 3.1 Recommended Stack

| Tool | Role | License | Why |
|------|------|---------|-----|
| **vhs** (Charmbracelet) | Animated GIF/MP4/WebM recorder | MIT | Already in repo (`demo.tape`). Declarative `.tape` DSL. CI-friendly via `vhs-action`. |
| **freeze** (Charmbracelet) | Static PNG/SVG/WebP screenshots | MIT | Pipes TUI output via `tmux capture-pane -pet N | freeze`. Window chrome, shadows, backgrounds. |
| **asciinema** + **agg** | Highest-quality terminal GIFs | GPL-3.0 | Truecolor, Nerd Font, emoji rendering. Best when recording must look exactly like the real terminal. |
| **svg-term-cli** | Animated SVG for docs | MIT | Razor-sharp, zoomable. Ideal for README/docs where zoom quality matters. |
| **ffmpeg** | Format conversion, optimization | LGPL/GPL | GIF→MP4, VP9/WebM encoding, scaling. |
| **oxipng** / **pngquant** | PNG optimization | MIT | Lossless/lossy PNG minification. |
| **gifsicle** | GIF optimization | GPL | Lossy GIF compression (`-O3 --lossy=30`). |

### 3.2 Tools Evaluated and Rejected

| Tool | Reason for Rejection |
|------|---------------------|
| **peek** | Deprecated 2024. X11/GNOME Wayland only. |
| **terminalizer** | Node.js dependency. Less CI-friendly than vhs. |
| **ttygif** | Screenshot-based, lower quality than modern alternatives. |
| **gifcast** | Browser-only, not scriptable for CI. |
| **t-rec-rs** | Requires real desktop session, non-deterministic. |

---

## 4. Platform-Specific Dimensions & Formats

### 4.1 Exact Dimensions

| Platform | Dimensions | Aspect | Max Size | Format |
|----------|-----------|--------|----------|--------|
| **GitHub README** | 1200×600 to 1311×605 | ~2:1 | 10 MB | PNG, GIF, MP4 (via `<video>`) |
| **GitHub Social Preview** | 1280×640 | 2:1 | 1 MB | PNG, JPG |
| **Twitter/X `summary_large_image`** | 1200×600 | 2:1 | 5 MB | JPG, PNG, WEBP (GIF uses first frame only) |
| **Open Graph (FB/LinkedIn)** | 1200×630 | 1.91:1 | 5 MB | JPG, PNG |
| **YouTube thumbnail** | 3840×2160 (min 640 wide) | 16:9 | 2–50 MB | JPG, PNG |
| **Dev.to cover** | 1000×420 | 2.38:1 | — | PNG, JPG |
| **Landing page hero** | 2560×1440 (2× retina) | 16:9 | — | MP4/WebM (autoplay loop) |
| **Docs site full-width** | 1440×810 | 16:9 | — | PNG |

### 4.2 Format Tradeoffs for Terminal Demos

| Format | File Size | Quality | Autoplay | Truecolor | Recommendation |
|--------|-----------|---------|----------|-----------|----------------|
| **GIF** | Very large | 256 colors only | Universal, silent | ❌ | Avoid for truecolor TUI |
| **MP4 (H.264)** | ~5–10× smaller than GIF | Full | Needs `<video>` tag | ✅ | Landing page hero |
| **WebM (VP9)** | Smaller than MP4 | Full | Needs `<video>` tag | ✅ | Landing page (Chrome/Firefox) |
| **APNG** | ~1.5–3× GIF | Full | Loops, no autoplay on Twitter | ✅ | README fallback |
| **PNG** | Smallest single frame | Full | Static only | ✅ | Static hero, social preview |

**Key insight:** GIF's 256-color palette mangles NIKI's truecolor (24-bit) TUI output. Use MP4/WebM for animated demos and PNG/APNG for static social previews.

### 4.3 Platform Gotchas

- **GitHub** displays only the **first frame** of animated GIFs — make the first frame the most representative.
- **Twitter** ignores animated GIFs for `summary_large_image` (uses first frame). Video cards require a separate Player Card meta tag.
- **LinkedIn/Facebook** fall back to `og:image` when `og:video` isn't provided.
- **Twitter Cards Validator:** https://cards-dev.twitter.com/validator
- **Facebook Sharing Debugger:** https://developers.facebook.com/tools/debug/

---

## 5. Landing Page Section → Asset Mapping

Based on analysis of Cursor, Claude Code, OpenAI Codex, GitHub Copilot, and Devin landing pages.

| # | Section | Required Asset | NIKI Source |
|---|---------|---------------|-------------|
| 1 | **Hero** | Full-screen animated demo (MP4/WebM) showing prompt → agents running → diff/branch produced | Chat page + Run page + Diff page |
| 2 | **How it works** | 3–4 step animated workflow diagram | Pipeline page (Planner→Coder→Tester→Reviewer) |
| 3 | **Features grid** | 4–6 callout screenshots, one per feature | Run, Diff, Verdict, Cost, Fleet, Session |
| 4 | **Capabilities showcase** | Vertical stack of 4–6 cards with before/after diffs | Diff page + Verdict page |
| 5 | **Comparison table** | Clean table with checkmarks or benchmark chart | Cost page (per-agent breakdown) |
| 6 | **Social proof** | Headshots or logos with quote blocks | (needs external content) |
| 7 | **Benchmarks** | Bar chart or percentage callout | (needs external data) |
| 8 | **Pricing** | Pricing cards with feature lists | (needs external content) |
| 9 | **FAQ** | Text-only accordion | Help page |
| 10 | **Security/Trust** | Shield icons, compliance badges, architecture diagram | Permission modal + sandbox config |

**Hero image recommendation:** Show the agent *mid-action* — a diff streaming into a file, terminal output visible, or the agent autonomously running commands. For NIKI specifically, show the **handoff moment**: one agent finishing and another picking up, or the git branch appearing. Static screenshots underperform animated GIFs that show the full loop: prompt → agents working → diff/branch produced.

---

## 6. Asset Generation Workflow

### 6.1 Current State (Broken)

```
demo.tape → python3 scripts/render_demo_terminal.py → vhs → assets/demo.gif + assets/demo.mp4
```

**Problem:** The Python simulator hand-renders ANSI frames that diverge from the real `ratatui` TUI. The current GIF predates Phases 1–5 of TUI development.

### 6.2 Proposed Workflow (Real Binary)

```
┌─────────────────────────────────────────────────────────────────┐
│  1. Build: cargo build --release                                │
│  2. Launch: tmux new-session -x 120 -y 30 ./target/release/niki │
│  3. Drive: VHS .tape script sends keystrokes, waits for markers │
│  4. Capture: vhs renders GIF/MP4/WebM from the live tmux pane   │
│  5. Optimize: gifsicle / ffmpeg / oxipng                        │
│  6. Export: copy to assets/, docs/public/, social/              │
└─────────────────────────────────────────────────────────────────┘
```

### 6.3 VHS Tape Structure (per screen)

```tape
# demo-chat.tape — Chat view demo
Set FontFamily "JetBrains Mono"
Set FontSize 14
Set Width 1200
Set Height 600
Set Theme "niki-dark"
Set LoopOffset 40

Hide
Type "./target/release/niki run 'Add health endpoint' --backend worktree --tui"
Enter
Wait+Screen "Welcome to NIKI"
Show

# Type a prompt
Type "Add a /health endpoint that returns 200 OK"
Enter
Wait+Screen "Planner"

# Let agents run
Wait+Screen "Coder"
Wait+Screen "Tester"
Wait+Screen "Reviewer"

# Show /cost output
Type "/cost"
Enter
Wait+Screen "Session Economics"

# Show context gauge
Wait+Screen "Context Window"

Screenshot assets/screenshots/chat-cost.png
Output assets/demo-chat.gif
Output assets/demo-chat.mp4
```

### 6.4 Screen-by-Screen Tape Plan

| Screen | Tape File | Key Actions | Output |
|--------|-----------|-------------|--------|
| Chat | `demo-chat.tape` | Type prompt → agents run → /cost → context gauge | `demo-chat.gif`, `demo-chat.mp4` |
| Run | `demo-run.tape` | Show live pipeline streaming + tool cards | `demo-run.gif` |
| Pipeline | `demo-pipeline.tape` | Navigate to Pipeline tab, show 4-up grid | `demo-pipeline.png` |
| Diff | `demo-diff.tape` | Navigate to Diff, show file sidebar + hunks | `demo-diff.gif` |
| Verdict | `demo-verdict.tape` | Show verdict tile + report | `demo-verdict.png` |
| Cost | `demo-cost.tape` | Show cost breakdown + bar chart | `demo-cost.png` |
| Fleet | `demo-fleet.tape` | Show mission grid | `demo-fleet.png` |
| Session | `demo-session.tape` | Show 7-tab investigation view | `demo-session.png` |
| Permission | `demo-permission.tape` | Trigger permission modal | `demo-permission.png` |
| Onboarding | `demo-onboarding.tape` | Show 5-step wizard | `demo-onboarding.gif` |

### 6.5 CI Integration

Use `charmbracelet/vhs-action` in a GitHub workflow:

```yaml
# .github/workflows/assets.yml
on:
  push:
    paths:
      - 'demo-*.tape'
      - 'src/display/**'
jobs:
  render:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: charmbracelet/vhs-action@v2
        with:
          path: "demo-chat.tape"
          install-fonts: true
      - uses: stefanzweifel/git-auto-commit-action@v5
        with:
          file_pattern: "assets/*.gif assets/*.mp4 assets/*.png"
```

### 6.6 Mock LLM for Deterministic Demos

The tmux smoke suite already uses `tests/integration/mock_llm.py` on `:8080`. VHS can drive the real binary against this mock:

```bash
# Terminal 1: Start mock LLM
python3 tests/integration/mock_llm.py &

# Terminal 2: Run VHS
vhs demo-chat.tape
```

This produces **pixel-accurate, deterministic** recordings of the actual TUI — no Python simulator divergence.

---

## 7. Optimization Pipeline

### 7.1 GIF Optimization

```bash
gifsicle -O3 --lossy=30 -o assets/demo-chat-optimized.gif assets/demo-chat.gif
```

### 7.2 MP4 Conversion (for landing page)

```bash
ffmpeg -i assets/demo-chat.gif -movflags faststart -pix_fmt yuv420p \
  -vf "fps=30,scale=1280:-2" assets/demo-chat.mp4
```

### 7.3 WebM Conversion (smaller, Chrome/Firefox)

```bash
ffmpeg -i assets/demo-chat.gif -c:v libvpx-vp9 -b:v 0 -crf 30 \
  -pix_fmt yuv420p assets/demo-chat.webm
```

### 7.4 PNG Optimization

```bash
oxipng -o 4 --strip safe assets/screenshots/*.png
# or for lossy:
pngquant --quality=65-80 assets/screenshots/*.png
```

### 7.5 Social Preview (1280×640)

```bash
# Take first frame of GIF, resize to 1280×640, optimize
ffmpeg -i assets/demo-chat.gif -vf "select=eq(n\,0)" -vframes 1 - \
  | convert - -resize 1280x640 - assets/social/preview.png
```

---

## 8. File Organization

```
assets/
├── demo-chat.gif              # Chat view (animated)
├── demo-chat.mp4             # Chat view (video)
├── demo-run.gif              # Run page (animated)
├── demo-diff.gif             # Diff page (animated)
├── demo-onboarding.gif       # Onboarding wizard (animated)
├── screenshots/
│   ├── chat-cost.png         # Chat with /cost output
│   ├── chat-context.png      # Chat with context gauge
│   ├── run-pipeline.png      # Run page with live pipeline
│   ├── pipeline-grid.png     # Pipeline 4-up grid
│   ├── diff-sidebar.png      # Diff with file sidebar
│   ├── verdict-approved.png  # Verdict: APPROVED
│   ├── cost-breakdown.png    # Cost table + bar chart
│   ├── fleet-grid.png        # Fleet mission grid
│   ├── session-tabs.png      # Session 7-tab view
│   ├── permission-modal.png  # Permission request
│   └── command-palette.png   # Command palette
├── social/
│   ├── preview.png           # 1280×640 GitHub social preview
│   ├── twitter.png           # 1200×600 Twitter card
│   ├── og.png                # 1200×630 Open Graph
│   └── youtube-thumb.png     # 3840×2160 YouTube thumbnail
├── tapes/
│   ├── demo-chat.tape
│   ├── demo-run.tape
│   ├── demo-diff.tape
│   ├── demo-onboarding.tape
│   └── ...
└── logo.svg                  # Existing
```

---

## 9. Immediate Action Items

### 9.1 Fix Responsive Flaws (before capturing)

1. **status_bar.rs:** Replace magic-width branches with priority-based greedy layout + "more" indicator.
2. **input_box.rs:** Extract shared `mode_indicator_len(mode)` function.
3. **run.rs:** Replace fixed `Length` constraints with `Min` + `Percentage`.
4. **diff.rs:** Use `Percentage(25)` with `Min` for sidebar graceful degradation.
5. **run.rs:** Replace absolute `y` positioning with vertical layout constraints.
6. **Add responsive tests** for Fleet, Pipeline, Config pages at 40-col, 80-col, 120-col, 200-col widths.

### 9.2 Create VHS Tapes

1. `demo-chat.tape` — Full chat flow with mock LLM.
2. `demo-run.tape` — Live pipeline streaming.
3. `demo-diff.tape` — Diff with file sidebar.
4. `demo-onboarding.tape` — 5-step wizard.
5. One tape per remaining page (Pipeline, Verdict, Cost, Fleet, Session).

### 9.3 Generate Assets

1. Run all tapes against the real binary + mock LLM.
2. Optimize GIFs with `gifsicle`.
3. Convert to MP4/WebM for landing page.
4. Generate PNG screenshots for static pages.
5. Create social previews at exact dimensions.

### 9.4 Wire to CI

1. Add `.github/workflows/assets.yml` with `vhs-action`.
2. Auto-commit regenerated assets on tape changes.
3. Add a `make assets` target for local regeneration.

### 9.5 Update Marketing Surfaces

1. **README.md:** Replace stale `demo.gif` with new `demo-chat.gif`. Add per-page screenshots below the fold.
2. **docs/content/:** Add screenshots to feature pages (currently zero images).
3. **GitHub Social Preview:** Upload 1280×640 `assets/social/preview.png`.
4. **Landing page (future):** Use MP4/WebM hero with autoplay loop.

---

## 10. Verification Notes

### Issues Found and Resolved

| Issue | Source | Resolution |
|-------|--------|------------|
| Hardcoded `80` in tool_card hit-test | chat.rs:986 | Fixed to use `state.chat_width.get()` |
| "14 pages" claim glosses over Fleet/Session being special-cased | agent-0 | Clarified: 12 standard + 2 special views |
| VHS output is from Python simulator, not real binary | agent-4 | Prescribed real-binary workflow with mock LLM |
| README has no social proof/benchmarks despite agent-2 listing them | agent-2 vs agent-7 | Clarified: these are best-practice recommendations, not current state |
| Repo doesn't follow its own stated best practices (1311px image vs 700–1000px claim) | agent-5 | Noted as external advice, not repo finding |

### Contradictions Resolved

- **Agent-1 (VHS stack) vs Agent-4 (Python simulator):** VHS is the right tool, but must drive the real binary, not the Python simulator.
- **Agent-2 (best practices) vs Agent-7 (actual assets):** Best practices are aspirational; current state is 4 media files for 29 visual states.

---

## Appendix A: Existing Media Files

| File | Size | Format | Content | Status |
|------|------|--------|---------|--------|
| `assets/demo.gif` | 892 KB | GIF | Chat view (Python-simulated, stale) | ⚠️ Needs refresh |
| `assets/demo.mp4` | 979 KB | MP4 | Same as GIF | ⚠️ Needs refresh |
| `assets/logo.svg` | 449 B | SVG | Brand logo on dark card | ✅ Current |
| `docs/public/logo.svg` | 449 B | SVG | Docs site logo | ✅ Current |

## Appendix B: Key Source Files

| File | Purpose |
|------|---------|
| `demo.tape` | VHS tape driving the current (stale) demo |
| `scripts/render_demo_terminal.py` | Python ANSI simulator (diverged from real TUI) |
| `tests/tui_smoke/lib.sh` | Real-PTY tmux automation harness |
| `tests/integration/mock_llm.py` | Mock LLM server for deterministic demos |
| `tests/visual_layout_check.rs` | Responsive layout tests (limited coverage) |
| `docs/prompt-demo-gif-refresh.md` | Tracked TODO for demo refresh |
| `docs/launch-audit.md` | Launch checklist (notes demo is outdated) |
