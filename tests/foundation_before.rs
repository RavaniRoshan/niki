//! "Before" reference frames for the TUI rebuild.
//!
//! This binary renders the **existing** ratatui TUI — the real `AppState`, fed
//! through the real reducer, drawn by the real components — into a
//! [`ratatui::backend::TestBackend`], and writes what came out as text files
//! under `docs/foundation/before/`. A rebuild can then be diffed against them.
//!
//! Two rules govern everything here:
//!
//! * **Nothing is invented.** Every state is built the way the product builds
//!   it: `AppState::new` (or `run_chat`'s own one-line setup at
//!   `src/display/tui.rs:1548`) plus a sequence of real `DisplayEvent`s through
//!   `AppState::apply_event`. If a state cannot be reached, the closest
//!   reachable one is captured and the gap is written down in
//!   `docs/foundation/before/README.md` — see [`STATE_NOTES`].
//! * **Nothing is hidden.** Two files per frame: a plain-text grid
//!   (`<state>_<cols>x<rows>.txt`, the primary artefact) and an ANSI-coloured
//!   variant (`<state>_<cols>x<rows>.ansi`) so the colour decisions are visible
//!   and not just the layout.
//!
//! Run with:
//!
//! ```text
//! cargo test -j 2 --test foundation_before -- --test-threads=1 --nocapture
//! ```

use std::path::{Path, PathBuf};

use niki::artifacts::types::AgentRole;
use niki::config::types::NikiConfig;
use niki::display::pages::{AppState, PageId, PageRouter};
use niki::display::tui::DisplayEvent;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Block;

/// The two terminal sizes every state is captured at.
const SIZES: [(u16, u16); 2] = [(80, 24), (120, 38)];

/// Where the frames go, relative to the crate root.
const OUT_REL: &str = "docs/foundation/before";

// ============================================================================
// Frame composition
// ============================================================================

/// Draw one full frame, the way `src/display/tui.rs::render` does.
///
/// `render` itself is private (`src/display/tui.rs:2111`), so the layering is
/// reassembled here out of the same public pieces in the same order:
/// background → adaptive header → page → status line → overlays, top to
/// bottom, exactly as `tui.rs:2129-2268` orders them. The one layer that cannot
/// be reproduced from outside the crate is the activity spinner
/// (`tui.rs:1993`, drawn at `tui.rs:2244` when a stage is running); it is
/// documented in the README rather than faked.
fn draw_frame(state: &AppState, cols: u16, rows: u16) -> Buffer {
    let backend = TestBackend::new(cols, rows);
    let mut terminal = Terminal::new(backend).expect("TestBackend terminal");
    let router = PageRouter::new();
    let palette = niki::display::command_palette::CommandPalette::new();

    terminal
        .draw(|f| {
            let size = f.area();
            if size.height < niki::display::tui::MIN_TERMINAL.1
                || size.width < niki::display::tui::MIN_TERMINAL.0
            {
                niki::display::tui::render_too_small(f, size);
                return;
            }

            f.render_widget(
                Block::default().style(Style::default().bg(niki::display::theme::bg_color())),
                size,
            );

            let bands = niki::display::tui::bands(size);

            if bands.header.height > 0 {
                niki::display::logo::render_adaptive_header(f, bands.header, state);
            }

            match state.current_page {
                PageId::Fleet => niki::display::pages::fleet::render_fleet(
                    &state.fleet,
                    bands.content,
                    f.buffer_mut(),
                ),
                PageId::Session => match state.session_view {
                    Some(ref sv) => niki::display::pages::session::render_session(
                        sv,
                        &state.chat_log,
                        bands.content,
                        f.buffer_mut(),
                    ),
                    None => f.render_widget(
                        ratatui::widgets::Paragraph::new(vec![
                            ratatui::text::Line::from(" session"),
                            ratatui::text::Line::from(""),
                            ratatui::text::Line::from(ratatui::text::Span::styled(
                                "  No session open — run a task or pick a mission from Fleet (g).",
                                Style::default().fg(niki::display::theme::fg_dim()),
                            )),
                        ]),
                        bands.content,
                    ),
                },
                PageId::Chat => niki::display::layout::render_chat(f, bands.content, state),
                _ => router.render_current(f, bands.content, state),
            }

            niki::display::components::status_bar::render_status_bar(f, state, bands.status);

            if let Some(ref modal) = state.modal {
                niki::display::modal::render_modal(f, modal, size);
            }
            if let Some(ref onboard) = state.onboarding {
                onboard.render(f, size);
            }
            if state.show_command_palette {
                niki::display::command_palette::render_command_palette(f, &palette, size);
            }
            if state.show_help {
                niki::display::help_overlay::render_help_overlay(
                    f,
                    size,
                    &state.keybindings,
                    &state.keybinding_overrides,
                    state.keybinding_conflicts.len(),
                );
            }
            if state.show_command_menu {
                niki::display::components::render_command_menu(f, size, state);
            }
            if state.input_state.autocomplete.is_some() {
                niki::display::components::render_autocomplete(f, size, state);
            }
            if state.show_permission_modal
                && let Some(ref req) = state.permission_request
            {
                niki::display::components::render_permission_modal(f, req, size, state);
            }
            if state.show_ask_modal
                && let Some(ref req) = state.ask_request
            {
                niki::display::components::ask_user::render_ask_user_modal(f, req, size, state);
            }
            if let Some(idx) = state.tool_detail_index
                && let Some(card) = state.tool_cards.get(idx)
            {
                let content = niki::display::components::tool_detail::detail_content_lines(card);
                let viewport = niki::display::components::tool_detail::detail_viewport(size);
                let offset = state.tool_detail_scroll.view_offset(content, viewport);
                niki::display::components::tool_detail::render_tool_detail(f, card, size, offset);
            }
            niki::display::sheets::render_top_sheet(f, size, &state.sheets, state);
        })
        .expect("draw must not fail");

    terminal.backend().buffer().clone()
}

/// The plain-text grid: one line per row, trailing spaces trimmed.
///
/// Trailing spaces are dropped so a diff between two frames is a diff about
/// content. The **row count is not** adjusted — every frame is exactly `rows`
/// lines, blank ones included, and the test asserts it.
fn buffer_to_text(buf: &Buffer) -> String {
    let area = buf.area;
    let mut out = String::new();
    for y in 0..area.height {
        let mut line = String::with_capacity(area.width as usize);
        for x in 0..area.width {
            line.push_str(buf[(x, y)].symbol());
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// Map one cell's style to the SGR sequence that would produce it.
///
/// `ratatui::style::Style` holds `Option<Color>` for fg/bg, so an unset
/// channel maps to the terminal's own default (`39` / `49`) — the same thing
/// the buffer means by "no colour set here".
fn sgr(style: Style) -> String {
    let mut codes: Vec<String> = Vec::new();
    let m = style.add_modifier;
    for (flag, code) in [
        (Modifier::BOLD, "1"),
        (Modifier::DIM, "2"),
        (Modifier::ITALIC, "3"),
        (Modifier::UNDERLINED, "4"),
        (Modifier::REVERSED, "7"),
        (Modifier::CROSSED_OUT, "9"),
    ] {
        if m.contains(flag) {
            codes.push(code.to_string());
        }
    }
    for (color, fg) in [(style.fg, true), (style.bg, false)] {
        if let Some(code) = color.and_then(|c| color_code(c, fg)) {
            codes.push(code);
        }
    }
    if codes.is_empty() {
        String::new()
    } else {
        format!("\x1b[{}m", codes.join(";"))
    }
}

/// The SGR fragment for one colour channel: the sixteen ANSI names map to their
/// fixed codes, 256-colour indices to `5;n`, truecolour to `2;r;g;b`.
fn color_code(color: Color, fg: bool) -> Option<String> {
    use Color::*;
    // (variant, fg code, bg code) for the fixed sixteen.
    let named = match color {
        Reset => ("39", "49"),
        Black => ("30", "40"),
        Red => ("31", "41"),
        Green => ("32", "42"),
        Yellow => ("33", "43"),
        Blue => ("34", "44"),
        Magenta => ("35", "45"),
        Cyan => ("36", "46"),
        Gray => ("37", "47"),
        DarkGray => ("90", "100"),
        LightRed => ("91", "101"),
        LightGreen => ("92", "102"),
        LightYellow => ("93", "103"),
        LightBlue => ("94", "104"),
        LightMagenta => ("95", "105"),
        LightCyan => ("96", "106"),
        White => ("97", "107"),
        Rgb(r, g, b) => {
            return Some(if fg {
                format!("38;2;{r};{g};{b}")
            } else {
                format!("48;2;{r};{g};{b}")
            });
        }
        Indexed(n) => {
            return Some(if fg {
                format!("38;5;{n}")
            } else {
                format!("48;5;{n}")
            });
        }
    };
    Some(if fg { named.0 } else { named.1 }.to_string())
}

/// The same grid with SGR escapes: a style change emits a reset plus the new
/// style, so `cat`ing the file in a real terminal shows the colours the buffer
/// actually carried. Trailing whitespace is dropped per row, as in the text
/// variant, but every row still emits its own reset.
fn buffer_to_ansi(buf: &Buffer) -> String {
    let area = buf.area;
    let mut out = String::new();
    for y in 0..area.height {
        let cells: Vec<(&str, Style)> = (0..area.width)
            .map(|x| {
                let c = &buf[(x, y)];
                (
                    c.symbol(),
                    Style::default().fg(c.fg).bg(c.bg).add_modifier(c.modifier),
                )
            })
            .collect();
        let last_ink = cells.iter().rposition(|(s, _)| !s.trim().is_empty());
        let mut current: Option<Style> = None;
        if let Some(last) = last_ink {
            for (symbol, style) in &cells[..=last] {
                if current != Some(*style) {
                    out.push_str("\x1b[0m");
                    let s = sgr(*style);
                    if !s.is_empty() {
                        out.push_str(&s);
                    }
                    current = Some(*style);
                }
                out.push_str(symbol);
            }
            out.push_str("\x1b[0m");
        }
        out.push('\n');
    }
    out
}

// ============================================================================
// The states
// ============================================================================

/// A fresh state, exactly as `AppState::new` leaves it (`state.rs:1283`).
/// `current_page` defaults to `PageId::Run` (`state.rs:1291`).
fn fresh_state() -> AppState {
    AppState::new(
        "add a health endpoint".into(),
        NikiConfig::default(),
        "/tmp/test".into(),
    )
}

/// `niki chat` puts the transcript on screen the moment it starts:
/// `run_chat` sets `current_page = PageId::Chat` (`tui.rs:1548`) and nothing
/// else. Anything that renders in the transcript — chat turns, tool cards — is
/// only visible from here, which is why the conversation-shaped states use it.
fn chat_state() -> AppState {
    let mut state = fresh_state();
    state.current_page = PageId::Chat;
    state
}

/// 1. Idle: a state that has been created and nothing has happened to it.
fn state_idle() -> AppState {
    fresh_state()
}

/// 2. Streaming: a user turn is in, and the assistant is mid-answer.
///
/// `ChatMessage` for the user turn, `ChatPending` for the in-flight request,
/// then `ChatDelta` chunks — the exact sequence `cli/chat.rs` emits. No
/// `ChatFinished`, so the text is still the live `chat_stream` and renders as
/// streaming rather than as a committed turn (`state.rs:1785-1787`).
fn state_streaming() -> AppState {
    let mut state = chat_state();
    state.apply_event(DisplayEvent::ChatMessage {
        role: "user".to_string(),
        text: "Why is the CI job flaky on the integration suite?".to_string(),
    });
    state.apply_event(DisplayEvent::ChatPending);
    state.apply_event(DisplayEvent::ChatDelta {
        text: "Let me look at the last failing run. The integration suite runs ".to_string(),
    });
    state.apply_event(DisplayEvent::ChatDelta {
        text: "against a shared Postgres fixture, and two suites truncate the \
               same table.\n\n"
            .to_string(),
    });
    state.apply_event(DisplayEvent::ChatDelta {
        text: "```rust\n// both suites share this\ndb.execute(\"TRUNCATE users CASCADE\");\n```"
            .to_string(),
    });
    state
}

/// 3. Parallel tools: three calls in flight at once, one already back.
fn state_parallel_tools() -> AppState {
    let mut state = chat_state();
    state.apply_event(DisplayEvent::ChatMessage {
        role: "user".to_string(),
        text: "Wire the new endpoint up and prove it works.".to_string(),
    });
    state.apply_event(DisplayEvent::ToolCall {
        role: AgentRole::Coder,
        tool_name: "file_edit".to_string(),
        summary: "src/routes/health.rs".to_string(),
    });
    state.apply_event(DisplayEvent::ToolCall {
        role: AgentRole::Coder,
        tool_name: "shell".to_string(),
        summary: "cargo build --release".to_string(),
    });
    state.apply_event(DisplayEvent::ToolCall {
        role: AgentRole::Coder,
        tool_name: "shell".to_string(),
        summary: "cargo test --test integration".to_string(),
    });
    // The first shell call comes back while the other two are still running.
    state.apply_event(DisplayEvent::ToolResult {
        role: AgentRole::Coder,
        tool_name: "shell".to_string(),
        success: true,
        error: None,
        output: Some("    Finished release [optimized] target(s) in 41.2s".to_string()),
        duration_ms: 41_240,
    });
    state
}

/// 4. A failed tool: the card that carries the inline error row.
fn state_failed_tool() -> AppState {
    let mut state = chat_state();
    state.apply_event(DisplayEvent::ChatMessage {
        role: "user".to_string(),
        text: "Add the health endpoint and check the build.".to_string(),
    });
    state.apply_event(DisplayEvent::ToolCall {
        role: AgentRole::Coder,
        tool_name: "shell".to_string(),
        summary: "cargo clippy --all-targets".to_string(),
    });
    state.apply_event(DisplayEvent::ToolResult {
        role: AgentRole::Coder,
        tool_name: "shell".to_string(),
        success: false,
        error: Some("error[E0432]: unresolved import `niki::display::route`".to_string()),
        output: None,
        duration_ms: 3_410,
    });
    state
}

/// 5. The permission modal, opened by a real `PermissionRequest`.
///
/// `apply_display_event` builds the `PermissionRequest` and raises
/// `show_permission_modal` (`state.rs:1928-1945`), which is the same path the
/// sandbox approval tool drives.
fn state_permission() -> AppState {
    let mut state = chat_state();
    state.apply_event(DisplayEvent::ChatMessage {
        role: "user".to_string(),
        text: "Run the full suite before you hand it back.".to_string(),
    });
    state.apply_event(DisplayEvent::ToolCall {
        role: AgentRole::Coder,
        tool_name: "sandbox_exec".to_string(),
        summary: "cargo test --verbose --all-features".to_string(),
    });
    let (tx, _rx) = std::sync::mpsc::channel::<niki::permissions::PermissionAction>();
    state.apply_event(DisplayEvent::PermissionRequest {
        command: "cargo test --verbose --all-features".to_string(),
        response_tx: tx,
    });
    assert!(
        state.show_permission_modal && state.permission_request.is_some(),
        "a PermissionRequest must raise the modal — otherwise this frame is \
         not the permission state and must not be written as one"
    );
    state
}

/// 6. The help overlay.
///
/// The overlay is raised by `?` in `route_overlay_key` (`tui.rs:404-406`),
/// which is a private function, so there is no public event or reducer entry
/// for it. `show_help` is a public `AppState` field, set the same way
/// `tests/visual_layout_check.rs:318` already sets it. Nothing else about the
/// state is touched — see `README.md`.
fn state_help() -> AppState {
    let mut state = chat_state();
    state.show_help = true;
    assert!(state.show_help);
    state
}

/// Which state each frame name refers to, and how it was reached.
/// One state factory and the note describing what the frame is meant to prove.
type StateNote = (&'static str, fn() -> AppState);

const STATE_NOTES: [StateNote; 6] = [
    ("idle", state_idle),
    ("streaming", state_streaming),
    ("parallel_tools", state_parallel_tools),
    ("failed_tool", state_failed_tool),
    ("permission", state_permission),
    ("help", state_help),
];

// ============================================================================
// Writing
// ============================================================================

fn out_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(OUT_REL)
}

fn write(path: &Path, contents: &str) -> usize {
    std::fs::write(path, contents).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    contents.len()
}

// ============================================================================
// The test
// ============================================================================

#[test]
fn foundation_before_frames() {
    let dir = out_dir();
    std::fs::create_dir_all(&dir)
        .unwrap_or_else(|e| panic!("cannot create {}: {e}", dir.display()));

    // The colours in the `.ansi` files are the product's, resolved against this
    // environment. Printed, because the same command under `NO_COLOR=1` produces
    // the same layout and a completely different `.ansi`.
    println!(
        "colour env: NO_COLOR={:?} COLORTERM={:?} TERM={:?} NIKI_REDUCED_MOTION={:?}",
        std::env::var_os("NO_COLOR"),
        std::env::var_os("COLORTERM"),
        std::env::var_os("TERM"),
        std::env::var_os("NIKI_REDUCED_MOTION"),
    );

    let mut expected: Vec<PathBuf> = Vec::new();

    for (name, build) in STATE_NOTES {
        for (cols, rows) in SIZES {
            let state = build();

            // Determinism: the same state, rendered twice, must be byte-identical.
            let first = buffer_to_text(&draw_frame(&state, cols, rows));
            let second = buffer_to_text(&draw_frame(&state, cols, rows));
            assert_eq!(
                first, second,
                "{name} at {cols}x{rows} is not deterministic — the two renders \
                 of one state differ"
            );

            // Row count: exactly `rows` lines, blank rows included.
            assert_eq!(
                first.lines().count(),
                rows as usize,
                "{name} at {cols}x{rows} wrote {} lines, not {rows}",
                first.lines().count()
            );

            let txt_path = dir.join(format!("{name}_{cols}x{rows}.txt"));
            let ansi_path = dir.join(format!("{name}_{cols}x{rows}.ansi"));

            let txt_bytes = write(&txt_path, &first);
            let ansi = buffer_to_ansi(&draw_frame(&state, cols, rows));
            assert_eq!(
                ansi.lines().count(),
                rows as usize,
                "{name} at {cols}x{rows} ANSI frame has {} lines, not {rows}",
                ansi.lines().count()
            );
            let ansi_bytes = write(&ansi_path, &ansi);

            // The file exists and holds what we just wrote — the assertion that
            // fails if a write silently did not land.
            for (path, expected_bytes, what) in [
                (&txt_path, txt_bytes, "text"),
                (&ansi_path, ansi_bytes, "ansi"),
            ] {
                assert!(
                    path.exists(),
                    "{what} frame was not written: {}",
                    path.display()
                );
                let on_disk = std::fs::read_to_string(path)
                    .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
                assert_eq!(
                    on_disk.len(),
                    expected_bytes,
                    "{} on disk is {} bytes, the rendered frame was {expected_bytes}",
                    path.display(),
                    on_disk.len()
                );
            }

            println!(
                "{:<48} {:>6} bytes",
                format!("{OUT_REL}/{name}_{cols}x{rows}.txt"),
                txt_bytes
            );
            println!(
                "{:<48} {:>6} bytes",
                format!("{OUT_REL}/{name}_{cols}x{rows}.ansi"),
                ansi_bytes
            );

            expected.push(txt_path);
            expected.push(ansi_path);
        }
    }

    assert_eq!(
        expected.len(),
        STATE_NOTES.len() * SIZES.len() * 2,
        "every state at every size must produce a text and an ansi frame"
    );
    for path in &expected {
        assert!(path.exists(), "missing frame file: {}", path.display());
    }
}
