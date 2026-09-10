//! Mouse tracking modes (TUI-030).
//!
//! crossterm's `EnableMouseCapture` is DEC 1000 (clicks only). The TUI also
//! handles drags (scrollbar drag-to-scroll, text selection) and hover
//! highlights, which need button-motion (1002) + SGR extended coordinates
//! (1006). Hover-motion (1003, all mouse movement) is enabled only outside
//! multiplexers — under tmux/screen it floods the event loop (same caution
//! Pi applies).
//!
//! Call [`enable_tracking`] right after crossterm's `EnableMouseCapture` and
//! [`disable_tracking`] wherever capture is torn down (exit guard, Ctrl+E).

use std::io::{self, Write};

/// Button-motion + SGR coordinates. Always safe (works under tmux).
pub const MOTION_BUTTON: &str = "\x1b[?1002h\x1b[?1006h";
/// All-motion (hover without buttons). Direct terminals only.
pub const MOTION_ALL: &str = "\x1b[?1003h";
/// Full direct-terminal tracking: buttons + SGR + hover-motion.
pub const MOTION_FULL: &str = concat!("\x1b[?1002h\x1b[?1006h", "\x1b[?1003h");
const DISABLE_ALL: &str = "\x1b[?1002l\x1b[?1003l\x1b[?1006l";

/// Whether stdio sits inside tmux/screen (conservative motion policy).
pub fn multiplexed() -> bool {
    std::env::var("TMUX").is_ok()
        || std::env::var("STY").is_ok()
        || std::env::var("SCREEN").is_ok()
        || std::env::var("TERM")
            .unwrap_or_default()
            .starts_with("screen")
}

/// Escape sequence enabling motion tracking for the current environment.
pub fn enable_seq() -> &'static str {
    if multiplexed() {
        MOTION_BUTTON
    } else {
        MOTION_FULL
    }
}

/// Enable motion tracking on stdout (best-effort, never fails the TUI).
pub fn enable_tracking() -> io::Result<()> {
    let mut out = io::stdout();
    out.write_all(enable_seq().as_bytes())?;
    out.flush()
}

/// Disable all extended mouse tracking (1000 itself is crossterm's job).
pub fn disable_tracking() -> io::Result<()> {
    let mut out = io::stdout();
    out.write_all(DISABLE_ALL.as_bytes())?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequences_are_wellformed_dec_private_modes() {
        assert!(MOTION_BUTTON.starts_with("\x1b[?"));
        assert!(MOTION_BUTTON.contains("1002h"));
        assert!(MOTION_BUTTON.contains("1006h"));
        assert!(MOTION_ALL.contains("1003h"));
        assert!(DISABLE_ALL.contains("1002l"));
        assert!(DISABLE_ALL.contains("1003l"));
        assert!(DISABLE_ALL.contains("1006l"));
    }

    #[test]
    fn enable_seq_respects_multiplexer() {
        // enable_seq follows multiplexed(); both read the same env, so they
        // must agree by construction.
        let expected = if multiplexed() {
            MOTION_BUTTON
        } else {
            MOTION_FULL
        };
        assert_eq!(enable_seq(), expected);
    }
}
