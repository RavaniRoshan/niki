//! Interactive UI components matching Claude Code / Kimi Code quality.
//!
//! Components:
//! - [`status_bar`] — bottom status bar with model, context, cost
//! - [`input_box`] — text input with cursor rendering
//! - [`spinner`] — animated spinner (moon loader)
//! - [`permission`] — permission request modal
//! - [`command_menu`] — slash command menu overlay
//! - [`autocomplete`] — @ file autocomplete
//! - [`progress`] — progress indicators
//! - [`list_cursor`] — universal list cursor + focus model shared by overlays

pub mod autocomplete;
pub mod command_menu;
pub mod input_box;
pub mod list_cursor;
pub mod permission;
pub mod progress;
pub mod spinner;
pub mod status_bar;

// Re-exports
pub use autocomplete::render_autocomplete;
pub use command_menu::render_command_menu;
pub use input_box::render_input_box;
pub use input_box::render_input_box_multiline;
pub use list_cursor::{FocusState, ListCursor};
pub use permission::render_permission_modal;
pub use progress::render_progress_bar;
pub use spinner::{Spinner, SpinnerState, SpinnerStyle};
pub use status_bar::render_status_bar;
