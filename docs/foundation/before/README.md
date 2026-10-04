# Before — reference frames of the current TUI

These files are what the **existing** ratatui TUI actually renders. They were
produced by `tests/foundation_before.rs`, which builds a real `AppState`, feeds
it real `DisplayEvent`s through the real reducer, and draws it through the real
components into a `ratatui::backend::TestBackend`. Nothing here is
hand-written, mocked, or transposed by hand.

Regenerate with:

```bash
env -u NO_COLOR COLORTERM=truecolor TERM=xterm-256color \
  cargo test -j 2 --test foundation_before -- --test-threads=1 --nocapture
```

Two files per state per size:

| File | What it is |
| --- | --- |
| `<state>_<cols>x<rows>.txt` | the primary artefact — the plain-text grid, one line per row, trailing spaces trimmed |
| `<state>_<cols>x<rows>.ansi` | the same grid with SGR escapes, so the colour decisions are visible and not just the layout |

Sizes: 80x24 and 120x38.

---

## What each state is, and how it was reached

Every state starts from `AppState::new` (`src/display/state.rs:1283`) with
`NikiConfig::default()` and `/tmp/test` as the project path, exactly as
`tests/tui_navigation.rs:9` and `tests/visual_layout_check.rs:17` do.

| Frame | Page | Built by |
| --- | --- | --- |
| `idle` | `PageId::Run` | `AppState::new` and nothing else. `current_page` defaults to `PageId::Run` (`src/display/state.rs:1291`), so this is the untouched first frame. |
| `streaming` | `PageId::Chat` | `ChatMessage { role: "user" }` → `ChatPending` → three `ChatDelta` chunks, no `ChatFinished`, so the text is still the live `chat_stream` and renders as streaming (`src/display/state.rs:1772-1787`). |
| `parallel_tools` | `PageId::Chat` | `ChatMessage` → three `ToolCall`s → one `ToolResult { success: true }` for the first `shell` card. Two cards stay in `Running`, so a mixed running/done transcript is what gets captured (`src/display/state.rs:1949-1957` and `1958-1979`). |
| `failed_tool` | `PageId::Chat` | `ChatMessage` → `ToolCall` → `ToolResult { success: false, error: Some(..) }`. |
| `permission` | `PageId::Chat` | `ChatMessage` → `ToolCall` → `DisplayEvent::PermissionRequest { command, response_tx }`, which is the path the sandbox approval tool drives (`src/display/state.rs:1928-1945`). The test asserts `show_permission_modal` actually came up before writing the frame. |
| `help` | `PageId::Chat` | `state.show_help = true`. See the qualification below. |

The chat page is used for the conversation-shaped states because that is what
`niki chat` shows: `run_chat` sets `current_page = PageId::Chat` at
`src/display/tui.rs:1548` and nothing else. Tool cards are only pushed by
`build_chat_lines` on the chat transcript (`src/display/pages/chat.rs:1932`), so
`parallel_tools`, `failed_tool` and `permission` are not visible anywhere else.

---

## What could **not** be reached exactly as asked, and why

Nothing here was faked. Four things are qualified rather than captured:

### 1. The full frame composition is reassembled, not called

The real draw function is private: `fn render(...)` at
`src/display/tui.rs:2111` is not `pub`. The test therefore lays the frame down
from the same public pieces in the same order — background fill, adaptive
header, page, status line, then the overlay ladder bottom-up as
`src/display/tui.rs:2129-2268` orders it:

- `niki::display::tui::MIN_TERMINAL` / `render_too_small` (`tui.rs:2088`, `2091`)
- `niki::display::tui::bands` (`tui.rs:328`)
- `niki::display::logo::render_adaptive_header` (`logo.rs:77`)
- `niki::display::layout::render_chat` (`layout/mod.rs:42`) or
  `PageRouter::render_current`, with the Fleet and Session pages taking the
  special-cased branches from `tui.rs:2143-2173`
- `niki::display::components::status_bar::render_status_bar` (`status_bar.rs:13`,
  which `render_status_line` delegates to at `tui.rs:1987`)
- `modal::render_modal`, `help_overlay::render_help_overlay`,
  `components::render_permission_modal`, `components::ask_user::render_ask_user_modal`,
  `components::render_command_menu`, `components::render_autocomplete`,
  `command_palette::render_command_palette`, `tool_detail::*`, `sheets::render_top_sheet`

**One layer is missing from every frame: the activity spinner.**
`fn render_activity_spinner` (`src/display/tui.rs:1993`) is private and is drawn
at `src/display/tui.rs:2244-2245` when `state.has_running_stage()`. It cannot be
called from a test. It is *absent* from all six states by construction, because
none of them starts a pipeline stage — they are chat turns, tool cards and
overlays — so no captured frame is silently missing a strip it should have had.
The `resolved_run` line that replaces it (`tui.rs:2246-2263`) is likewise absent,
because `resolved_run` is only populated by the live loop.

### 2. `help` is raised by setting a public field, not by an event

The `?` key toggles `show_help` inside `route_overlay_key`
(`src/display/tui.rs:404-406`), and that function is private. There is no
`DisplayEvent` for it, and `DisplayEvent` (`src/display/tui.rs:48-238`) has no
help variant. So the test sets `state.show_help = true` directly — the same thing
`tests/visual_layout_check.rs:350` already does, and the only public route in.
Nothing else about the state is touched, so the frame is a real help overlay over
a real chat surface.

### 3. `permission` cannot name the tool that asked

`DisplayEvent::PermissionRequest` carries only `command` and `response_tx`
(`src/display/tui.rs:201-204`), and the reducer builds the `PermissionRequest`
with `tool_name: "sandbox_exec"` hardcoded (`src/display/state.rs:1934`) and
`description: String::new()` (`src/display/state.rs:1936`). So the captured modal
says `sandbox_exec` no matter which tool actually asked, and its description row
is empty. That is what the current UI shows; it is recorded rather than corrected.

### 4. `failed_tool` shows no inline error row — there isn't one

This is the honest finding, not a gap in the harness. The captured frames show
`✗ shell cargo clippy --all-targets` and nothing else: no error text, at any
width. Why:

- `set_failed` stores the message in `ToolStatus::Failed { error }` and sets
  `expanded = true` with the comment *"Auto-expand to show error"*
  (`src/display/components/tool_card.rs:67-72`).
- The expanded card body renders `card.output` and nothing else
  (`src/display/components/tool_card.rs:144-172`) — it never reads the error out
  of the status.
- The full-screen tool-detail overlay reads `card.output` too
  (`src/display/components/tool_detail.rs:29-33`), so pressing Enter on the card
  does not surface it either.

So the error string is captured in state and rendered nowhere on this surface.
The timing footer is likewise absent on a failed card, because `timing()` covers
only `Running` and `Success` (`src/display/components/tool_card.rs:90-96`).
Whatever the rebuild does here, it is not reproducing an existing behaviour.

---

## Determinism

The test renders every state twice and asserts the two grids are byte-identical;
it also asserts each frame is exactly `rows` lines (blank rows included — trailing
*spaces* are trimmed per row, the row count is not).

The frames are stable across runs on one machine, with two documented exceptions
that come from the product, not from the writer:

1. **Colour depends on the environment.** `theme::supports_truecolor()` reads
   `COLORTERM`/`TERM` (`src/display/theme.rs:168`) and `no_color()` reads
   `NO_COLOR` (`src/display/theme.rs:119`), which flattens every colour to
   `Reset`. With `NO_COLOR=1` set — as it is in some shells — the `.ansi` files
   come out as `39;49` throughout and carry no colour information. The committed
   frames were generated with `NO_COLOR` unset and truecolor advertised; use the
   command at the top of this file to reproduce them. `NIKI_REDUCED_MOTION` and
   `config.ui.reduced_motion` similarly change the permission-badge shimmer.
2. **The header carries the version.** `NIKI v0.10.0` comes from
   `CARGO_PKG_VERSION`, so a version bump rewrites row 1 of every `.txt`.

The `.txt` layout frames are unaffected by both.

---

## What the frames say about the current UI

Read as a set, these are the observations a rebuild should not inherit silently:

- `idle_80x24.txt` — the Run page's empty state is a suggested command line
  (`$ niki run 'add a health endpoint' --project /tmp/test`) inside a bordered
  box, with the rest of the frame empty. Nothing about a run that has not
  happened is claimed.
- At 120x38 the header becomes a 6-line block-drawing NIKI logo plus a hint line;
  at 80x24 it is a single compact line. Both are in the frames.
- `parallel_tools_80x24.txt` — the transcript's tool section is a fixed-width box
  (`┌─ Tool Execution ─…┐`) that is the same 50 characters wide at 120 columns as
  at 80, so on a wide frame it occupies the left third and leaves the rest blank.
- `failed_tool_*.txt` — a failure is a glyph and a name; the reason is absent
  (section 4 above).
- `permission_*.txt` — the modal is painted over the transcript without clearing
  it, so the transcript text and the modal borders interleave on the same rows.
- `help_*.txt` — likewise drawn over the surface beneath rather than over a
  cleared background: the input box's own text bleeds through the overlay's
  bottom border row, e.g. row 22 of `help_80x24.txt` reads
  `│▎ Build  Des────────…────────┤x   podman │`, where `▎ Build  Des` and
  `x   podman` are the input box showing through on either side of the box's
  corner.
- `streaming_*.txt` — the streamed assistant text is rendered with no
  role/author header, unlike the user turn, which carries `◈ user:`.