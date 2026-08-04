# Plan: Replicate Reference Home Screen Design for Niki TUI

## Goal
Replicate the exact design from the reference image for the home screen, then apply the same color theme and font to every other page.

## Reference Image Analysis

### Visual Elements (Top to Bottom)
1. **Logo**: Thick, blocky, pixel-art style "NIKI" text centered — NOT thin ASCII art
2. **Task card**: Blue vertical pipe `│` on left, "Build" label, description text, `sandbox`/`podman` badge tags in gray pills
3. **Command line**: `$ niki run "..." --project ./my-app` in dim text
4. **Pipeline status**: Colored dots per stage (◆ Planner, ⊕ Coder, ● Tester, ◆ Reviewer) with status text
5. **Results**: `branch niki/xxxxx · report.md · changes.patch` bold text + `working tree: untouched` dim text
6. **Footer**: 3 compact keybindings only: `tab switch agent`, `ctrl-p commands`, `esc cancel run`

### Color Theme (from reference)
- **Background**: Very dark navy/charcoal (`#1A1A2E` or `#0F0F1A`)
- **Primary accent**: Blue/cyan (the pipe `│` on the task card is blue)
- **Logo**: White/light gray, bold
- **Pipeline status**: Blue/cyan for agent names and status text
- **Text**: White primary, gray dim
- **Checkmark**: Green for success

### Key Design Principles
- **Minimalism**: Only 3 keybindings visible — all others under `Ctrl-P`
- **Compact**: Everything fits in minimal vertical space
- **Clean**: No borders, no status bar, no heavy chrome

---

## Implementation Tasks

### Task 1: Replace Logo with Thick Blocky Font
**File**: `src/display/logo.rs`

- Replace the thin FIGlet "big" `LOGO_LINES` with a thick, blocky, pixel-art style "NIKI" rendered in block characters (█, ▓, ░, etc.)
- The logo should be ~6-8 lines tall, wide and bold
- Render in white (`theme::FG`) instead of clay orange
- Keep the `render_logo()` and `render_logo_with_subtitle()` API unchanged
- Update `LOGO_HEIGHT` constant

**New logo approach**: Hand-craft a blocky NIKI using Unicode block characters:
```
 ███╗   ██╗██╗  ██╗██╗██╗  ██╗
 ████╗  ██║██║  ██║██║██║  ██║
 ██╔██╗ ██║██║  ██║██║██║  ██║
 ██║╚██╗██║██║  ██║██║██║  ██║
 ██║ ╚████║╚██████╔╝██║╚██████╔╝
 ╚═╝  ╚═══╝ ╚═════╝ ╚═╝ ╚═════╝
```

### Task 2: Update Color Theme to Match Reference
**File**: `src/display/theme.rs`

The reference uses blue as the primary accent, not clay orange. Changes:
- Add `ACCENT: Color = Color::Rgb(88, 166, 255)` — bright blue for primary accent
- Add `BG_DEEP: Color = Color::Rgb(15, 15, 26)` — darker navy background matching reference
- Keep existing colors for backward compatibility
- Update `BORDER_ACTIVE` to use the new blue accent
- Keep `CLAY_ORANGE` for the Niki brand identity but use blue for the UI accent

### Task 3: Redesign Run (Home) Page for Exact Match
**File**: `src/display/pages/run.rs`

Complete rewrite to match reference layout:
- **Task card**: Blue pipe `│` + "Build" (white, bold) + description + badge tags in gray pills
- **Command line**: `$ niki run "..." --project ./my-app` (dim text)
- **Pipeline status**: Single-line per stage with colored glyphs and status text
- **Separator**: Thin dim line
- **Results**: Branch name (bold) + artifacts + working tree status
- **Footer**: Only 3 keybindings: `tab`, `Ctrl-P`, `Esc`
- **Remove scroll** for now — keep it flat and compact like reference

### Task 4: Create Command Palette (Ctrl-P Modal)
**New file**: `src/display/command_palette.rs`

- New modal/overlay triggered by `Ctrl-P`
- Shows all available page shortcuts in a clean list format
- Categories: PAGES (pipeline, agents, diff, verdict, cost, artifacts, history, config), RUN (pause, scroll, quit), PIPELINE (next/prev stage, tab agent)
- Styled with the same blue accent theme
- `Esc` closes the palette
- Selecting an item navigates to that page

### Task 5: Update TUI Main Layout
**File**: `src/display/tui.rs`

- Remove the status bar at the bottom (reference has none)
- Add `Ctrl-P` key handler to open command palette
- Handle command palette events in the main event loop
- Render command palette overlay when active
- Adjust layout to use full height for content (no status bar)

### Task 6: Update Global Key Handling
**Files**: `src/display/tui.rs`, `src/display/pages/run.rs`

- Remove individual page navigation hotkeys from Run page (no more `p`, `a`, `d`, etc.)
- Only `Ctrl-P` opens the command palette for navigation
- Keep `q`/`Esc` for quit, `Space` for pause, `j/k` for scroll on Run page
- Other pages keep `Esc`/`q` to return to Run

### Task 7: Update All Page Headers to Use Blue Accent
**Files**: All `src/display/pages/*.rs`

- Replace `CLAY_ORANGE` page headers with the new blue accent color
- Consistent styling across all pages
- Same font weight and style as reference

### Task 8: Update Modal to Match Reference Style
**File**: `src/display/modal.rs`

- Update modal styling to match the reference design
- Blue accent borders instead of clay orange
- Cleaner layout

### Task 9: Write Tests for New Components
**File**: `tests/tui_navigation.rs`

- Add tests for command palette rendering and navigation
- Update existing navigation tests to use `Ctrl-P` flow
- Test that old hotkeys no longer work on Run page
- Test palette opens/closes correctly

### Task 10: Full Verification
- `cargo check` — no errors
- `cargo test` — all tests pass
- Visual verification with `cargo run -- run "test" --tui --dry-run`

---

## File Change Summary

| File | Change |
|---|---|
| `src/display/logo.rs` | Replace thin ASCII art with thick blocky logo |
| `src/display/theme.rs` | Add blue accent color, darker background |
| `src/display/pages/run.rs` | Complete redesign to match reference |
| `src/display/tui.rs` | Remove status bar, add Ctrl-P handler, layout changes |
| `src/display/command_palette.rs` | **NEW** — command palette overlay |
| `src/display/mod.rs` | Add command_palette module |
| `src/display/modal.rs` | Update styling |
| `src/display/pages/pipeline.rs` | Update header to blue accent |
| `src/display/pages/agents.rs` | Update header to blue accent |
| `src/display/pages/diff.rs` | Update header to blue accent |
| `src/display/pages/verdict.rs` | Update header to blue accent |
| `src/display/pages/cost.rs` | Update header to blue accent |
| `src/display/pages/artifacts.rs` | Update header to blue accent |
| `src/display/pages/history.rs` | Update header to blue accent |
| `src/display/pages/config.rs` | Update header to blue accent |
| `src/display/pages/help.rs` | Update header to blue accent |
| `src/display/pages/test_log.rs` | Update header to blue accent |
| `tests/tui_navigation.rs` | Add command palette tests, update nav tests |

---

## Verification Criteria
- [ ] Logo renders as thick blocky text matching reference
- [ ] Background is dark navy matching reference
- [ ] Task card has blue pipe + "Build" label matching reference
- [ ] Pipeline status shows colored dots per stage matching reference
- [ ] Footer shows only 3 keybindings matching reference
- [ ] Ctrl-P opens command palette with all shortcuts
- [ ] All page headers use blue accent consistently
- [ ] `cargo check` clean (no new errors)
- [ ] `cargo test` all pass (updated tests)
