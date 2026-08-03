# Marketing Assets: Render Real TUI Pages, Not Fabricate

## Context
The existing `marketing-assets-export/` contains fabricated assets (fake terminal streams, HTML mockups, CSS-animated SVGs with wrong timing). The user wants **real product screenshots** of the actual Niki TUI, captured from the real code, using real task data from the completed e429 run.

## Strategy
Use `ratatui::backend::TestBackend` to render each TUI `Page` into a character buffer, convert buffer to high-DPI PNG via a Python PIL renderer. For terminal recording, use per-stage block events with real e429 timing (no fake typing).

## Key Files (e429 task data)
- `/home/shiva/projects/niki/.niki/tasks/edb05249-be9c-48ac-9fa9-06051d8f472e/`
  - `task.json`, `report.md`, `changes.patch`, `safety_proof.json`
  - `artifacts/planner.json`, `artifacts/coder.json`, `artifacts/tester.json`, `artifacts/red.json`, `artifacts/reviewer.json`
  - `dashboard.html`

## TUI Source (real theme + pages)
- `src/display/theme.rs:10-31` — exact RGB colors
- `src/display/pages/run.rs` — RunPage (home)
- `src/display/pages/pipeline.rs` — PipelinePage
- `src/display/pages/diff.rs` — DiffPage
- `src/display/pages/verdict.rs` — VerdictPage
- `src/display/pages/cost.rs` — CostPage
- `src/display/pages/mod.rs` — AppState, StageInfo, Page trait
- `src/display/logo.rs:16` — FIGlet NIKI logo
- `src/display/banner.rs` — CLI banner (for cast)

## Deliverables

### 1. Wipe `marketing-assets-export/`
Delete all 14 files, recreate empty directory.

### 2. Add `src/display/pages/test_log.rs`
New `TestLogPage` that renders `tester.json` stdout line-by-line:
- `test ... ok` → green
- `test result:` → green bold
- `running N tests` → dim
- Register in `src/display/pages/mod.rs` (`PageId::TestLog`)

### 3. Add `src/bin/render_tui.rs`
Buffer-dump binary:
- Args: `--page <id> --task-id <uuid> --output <path>`
- Constructs `AppState` from e429 artifacts
- Uses `TestBackend::new(width, height)` + `Terminal::new(backend)`
- Calls `page.render()` for the selected page
- Dumps cell buffer as JSON: `{cells: [[{ch, fg_r, fg_g, fg_b, bg_r, bg_g, bg_b}, ...], ...]}`
- Pages to render: `home`, `pipeline`, `diff`, `verdict`, `cost`, `test_log`

### 4. Add `marketing-assets-export/render_tui.py`
PIL renderer:
- Reads JSON dump from stdin or file
- Uses `/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf` (monospace)
- Renders each cell as filled bg rect + fg text
- Output: PNG at 2x scale (2200x1200)

### 5. Screenshots to generate

| File | Page | Buffer Size | PNG Size |
|---|---|---|---|
| `home-screenshot.png` | RunPage | 110x40 | 2200x800 |
| `pipeline-screenshot.png` | PipelinePage | 110x40 | 2200x800 |
| `diff-screenshot.png` | DiffPage | 110x40 | 2200x800 |
| `verdict-screenshot.png` | VerdictPage | 110x40 | 2200x800 |
| `cost-screenshot.png` | CostPage | 110x40 | 2200x800 |
| `test-log-screenshot.png` | TestLogPage | 110x40 | 2200x800 |

### 6. Dashboard analytics (web)
- Playwright capture of `dashboard.html` at 1600x1200
- File: `dashboard-web.png`

### 7. Terminal recording

**`demo.cast`** — per-stage block events:
- Planner: 0s → 15s (complete block from planner.json)
- Coder: 15s → 25s (complete diff from coder.json)
- Tester: 25s → 62s (complete test output from tester.json)
- Red: 62s → 135s (complete challenges from red.json)
- Reviewer: 135s → 212s (complete verdict from reviewer.json)
- Completion: 212s (banner from banner.rs format)

No fake typing — one `"o"` event per stage with complete content.

**`demo.svg`** — `termtosvg render demo.cast demo.svg`

**`demo.mp4`** — Chromium animation-delay seeking (30 keyframes) → ffmpeg at 21fps, duration > 10s.

### 8. Pipeline architecture

`pipeline-architecture.svg` — regenerate with:
- Exact theme colors: `#B1B9F9`, `#C6A0F6`, `#4EBA65`, `#FFC107`, `#FF6384`, `#0D0D0D`
- Real glyphs: `◆ ⚡ ● ✗ ◆`
- Layout from PipelinePage flowchart
- Sandbox callout from OnboardingPage::SandboxBackends

`pipeline-architecture.png` — cairosvg render

## Implementation Order
1. Delete `marketing-assets-export/`
2. Add `src/display/pages/test_log.rs` + register in mod.rs
3. Add `src/bin/render_tui.rs`
4. Add `marketing-assets-export/render_tui.py`
5. Build and run `render_tui` for each page → JSON dumps
6. Run `render_tui.py` on each dump → PNGs
7. Capture `dashboard-web.png` via Playwright
8. Rewrite `demo.cast` generator with real e429 timing
9. Render `demo.svg` via termtosvg
10. Generate `demo.mp4` via Chromium + ffmpeg
11. Regenerate `pipeline-architecture.svg` + render to PNG
12. Verify all assets

## Validation
- [ ] `home-screenshot.png` shows NIKI FIGlet logo + task card + 5 agent rows + footer
- [ ] All screenshots use actual e429 data (edb05249, square function, 11/11 tests, Approved)
- [ ] `cost-screenshot.png` has `COST` column header from CostPage
- [ ] `demo.mp4` duration > 10 seconds
- [ ] `demo.cast` parses with no errors
- [ ] `pipeline-architecture.svg` uses exact theme colors
- [ ] No HTML mockups — every image is TUI buffer render or real capture
