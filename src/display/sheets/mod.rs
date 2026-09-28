//! Sheets: transient surfaces drawn over whatever page is behind them.
//!
//! This is the structure that lets the TUI become the whole product surface
//! rather than a viewer for a CLI. Settings, a theme picker, a provider
//! chooser, an MCP editor — none of those are pages. They are things you open,
//! use, and are done with, and they must not cost you your place in the
//! conversation you were having.
//!
//! So they are not `PageId`s. They are sheets, held on a **stack**, with a
//! two-state completion:
//!
//! ```text
//! Accepted  — the user committed (Enter on a form, chose a list item)
//! Cancelled — the user backed out (Esc)
//! ```
//!
//! The stack rather than a single optional sheet is what makes nesting
//! possible — a settings form that opens a provider picker must be able to come
//! back to the form, not to the page. A single `Option<Sheet>` gives you a
//! dialog; a `Vec<Sheet>` gives you a system.
//!
//! Keys are routed here before anything else. That ordering is the whole
//! contract: a sheet owns every key until it is done, because a settings form
//! that leaks a keypress to the page behind it is worse than no form at all.

use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::display::state::AppState;

/// How a sheet finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetOutcome {
    /// The user committed. Any pending changes have been written.
    Accepted,
    /// The user backed out. Pending changes are discarded.
    Cancelled,
}

/// A transient surface.
#[derive(Debug)]
pub enum Sheet {
    /// The editable settings form.
    Settings(Box<settings::SettingsSheet>),
    /// Choose a colour theme, with live preview.
    Theme(Box<theme::ThemeSheet>),
    /// See and change which provider and model each agent uses.
    Providers(Box<providers::ProviderSheet>),
    /// See and change the MCP servers this run may talk to.
    Mcp(Box<mcp::McpSheet>),
}

impl Sheet {
    /// The sheet's title. Takes `state` because a title may be derived from
    /// what is configured — "MCP — 3 servers" — and a title that had to guess
    /// would be a title that lies on first run.
    pub fn title(&self, state: &AppState) -> String {
        match self {
            Sheet::Settings(s) => s.title(),
            Sheet::Theme(s) => s.title(),
            Sheet::Providers(s) => s.title(),
            Sheet::Mcp(s) => s.title(state),
        }
    }

    /// The single line of help at the bottom of the sheet.
    pub fn hint(&self) -> String {
        match self {
            Sheet::Settings(s) => s.hint(),
            Sheet::Theme(s) => s.hint(),
            Sheet::Providers(s) => s.hint(),
            Sheet::Mcp(s) => s.hint(),
        }
    }

    /// Draw the sheet.
    ///
    /// Public so a test can render one into a `TestBackend` and read the frame
    /// a user would actually see. A source-text check cannot: it cannot tell a
    /// visible field from a comment about it, and it cannot tell a rendered
    /// line from one scrolled off the top or clipped by the viewport.
    pub fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        match self {
            Sheet::Settings(s) => s.render(frame, area, state),
            Sheet::Theme(s) => s.render(frame, area, state),
            Sheet::Providers(s) => s.render(frame, area, state),
            Sheet::Mcp(s) => s.render(frame, area, state),
        }
    }

    /// Handle a key.
    ///
    /// `Ok(None)` means the sheet is still open and has consumed the key.
    /// `Ok(Some(outcome))` means it is done and the caller pops it.
    /// Handle a key.
    ///
    /// Public so a test can drive the sheet the way the key loop does and read
    /// what it renders — a source grep cannot tell a field a user can edit
    /// from one they can only read.
    pub fn on_key(&mut self, key: KeyEvent, state: &mut AppState) -> Result<Option<SheetOutcome>> {
        match self {
            Sheet::Settings(s) => s.on_key(key, state),
            Sheet::Theme(s) => s.on_key(key, state),
            Sheet::Providers(s) => s.on_key(key, state),
            Sheet::Mcp(s) => s.on_key(key, state),
        }
    }

    /// Called when this sheet is popped after a nested sheet was accepted, so a
    /// parent that opened a child can collapse itself too.
    fn on_child_accepted(&mut self) {
        match self {
            Sheet::Settings(s) => s.on_child_accepted(),
            Sheet::Theme(_) => {}
            Sheet::Providers(_) => {}
            Sheet::Mcp(_) => {}
        }
    }
}

/// Where a sheet is drawn: a centred box, so the page behind stays legible and
/// the user never loses their place.
pub fn sheet_area(area: Rect, width_pct: u16, height_pct: u16) -> Rect {
    let w = area.width.saturating_mul(width_pct) / 100;
    let h = area.height.saturating_mul(height_pct) / 100;
    let w = w.clamp(24.min(area.width), area.width);
    let h = h.clamp(6.min(area.height), area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

/// Draw a sheet's frame, title and hint. Every sheet shares this chrome so they
/// look like one system rather than four dialogs.
pub fn chrome(frame: &mut Frame, area: Rect, title: &str, hint: &str, focused: bool) -> Rect {
    let border = if focused {
        crate::display::theme::border_active()
    } else {
        crate::display::theme::border_color()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(ratatui::style::Style::default().fg(border))
        .title(Line::from(vec![
            Span::styled(" ", ratatui::style::Style::default()),
            Span::styled(
                title.to_string(),
                ratatui::style::Style::default()
                    .fg(crate::display::theme::fg_color())
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
            Span::styled(" ", ratatui::style::Style::default()),
        ]));
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    if !hint.is_empty() && inner.height > 1 {
        let hint_area = Rect {
            x: inner.x,
            y: inner.y + inner.height - 1,
            width: inner.width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                hint.to_string(),
                ratatui::style::Style::default().fg(crate::display::theme::fg_dim()),
            ))),
            hint_area,
        );
        Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: inner.height.saturating_sub(1),
        }
    } else {
        inner
    }
}

pub mod mcp;
pub mod providers;
pub mod settings;
pub mod theme;

/// The stack, held on `AppState` as a `Vec` so a sheet can open another.
pub type SheetStack = Vec<Sheet>;

/// Route a key to the top sheet, from the caller's one borrow of `state`.
///
/// The stack lives *inside* `AppState` and a sheet's key handler also needs
/// `&mut AppState`, so passing both is two mutable borrows of one value. Moving
/// the stack out for the duration is the fix; it is three lines and it keeps
/// the sheets' own signatures honest.
///
/// Returns `Ok(true)` when a sheet consumed the key and the caller must stop
/// routing for this frame — which is every frame a sheet is open, because the
/// top sheet owns its keys exclusively.
pub fn route(state: &mut AppState, key: KeyEvent) -> Result<bool> {
    let mut stack = std::mem::take(&mut state.sheets);
    let result = route_sheet_key(&mut stack, key, state);
    // Even on the error path the stack has to go back, or the sheet would
    // vanish and the user would lose their unsaved edits with no explanation.
    state.sheets = stack;
    result
}

/// Route a key to the top sheet of an explicit stack. See [`route`].
pub fn route_sheet_key(
    stack: &mut SheetStack,
    key: KeyEvent,
    state: &mut AppState,
) -> Result<bool> {
    let Some(sheet) = stack.last_mut() else {
        return Ok(false);
    };
    match sheet.on_key(key, state)? {
        None => {}
        Some(SheetOutcome::Cancelled) => {
            stack.pop();
        }
        Some(SheetOutcome::Accepted) => {
            stack.pop();
            // A sheet that opened a child collapses too, so `/config` → pick a
            // provider → accept returns to the page rather than to a stale
            // form.
            if let Some(parent) = stack.last_mut() {
                parent.on_child_accepted();
            }
        }
    }
    Ok(true)
}

/// Render the top sheet, if any.
pub fn render_top_sheet(frame: &mut Frame, area: Rect, stack: &SheetStack, state: &AppState) {
    let Some(sheet) = stack.last() else { return };
    let inner = sheet_area(area, 72, 80);
    sheet.render(frame, inner, state);
}

/// Push a sheet, replacing any that is already open.
///
/// Replacing rather than stacking is right for `/config`: pressing it twice
/// should not leave two identical forms, and Esc should return to the page
/// rather than to a duplicate.
pub fn open_sheet(state: &mut AppState, sheet: Sheet) {
    state.sheets.clear();
    state.sheets.push(sheet);
}

pub fn sheets_open(state: &AppState) -> bool {
    !state.sheets.is_empty()
}

/// Helper shared by the list-style sheets: move a cursor within `len`, wrapping.
pub fn move_cursor(cursor: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let len = len as isize;
    let next = cursor as isize + delta;
    (((next % len) + len) % len) as usize
}

/// Whether a key is the "cancel" key, in both the emacs and vi conventions.
pub fn is_cancel(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Esc)
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}

/// Whether a key is the "commit" key.
pub fn is_commit(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Enter)
}

/// The glyph a sheet uses for its cursor.
///
/// The rest of the chrome already draws box-drawing characters unconditionally,
/// so branching here would make a sheet the one dialog in the product that
/// looks different in a limited terminal.
pub fn cursor_glyph() -> &'static str {
    "▸"
}
