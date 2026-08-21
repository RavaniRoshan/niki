//! Kitty keyboard protocol support (progressive adoption, I4).
//!
//! Some terminals (Kitty, Ghostty, WezTerm) support the enhanced keyboard
//! protocol, which disambiguates keys that share the same escape sequence —
//! notably Shift+Enter vs Enter. We enable it when the terminal advertises
//! support and degrade gracefully otherwise (tmux < 3.7 / screen never
//! support it). The enable/disable sequences are emitted around the TUI
//! session; decoding of CSI-u (`ESC [ code ; mods u`) is provided for
//! terminals that emit it, so modifiers map to a real [`KeyEvent`].

use std::io;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Whether the current terminal likely supports the Kitty keyboard protocol.
pub fn kitty_capable() -> bool {
    if std::env::var("TMUX").is_ok() {
        return false;
    }
    if std::env::var("STERM").is_ok() || std::env::var("SCREEN").is_ok() {
        return false;
    }
    let term = std::env::var("TERM").unwrap_or_default();
    let term_program = std::env::var("TERM_PROGRAM").unwrap_or_default();
    term.contains("kitty")
        || term.contains("ghostty")
        || term.contains("wezterm")
        || term_program.contains("kitty")
        || term_program.contains("ghostty")
        || term_program.contains("WezTerm")
}

/// Enable the Kitty keyboard protocol (`ESC [ > 1 u`).
pub fn enable_kitty_keyboard() -> io::Result<()> {
    use std::io::Write;
    let mut out = io::stdout();
    out.write_all(b"\x1b[>1u")?;
    out.flush()
}

/// Disable the Kitty keyboard protocol (`ESC [ < 1 u`).
pub fn disable_kitty_keyboard() -> io::Result<()> {
    use std::io::Write;
    let mut out = io::stdout();
    out.write_all(b"\x1b[<1u")?;
    out.flush()
}

/// Decode a CSI-u sequence of the form `ESC [ <code> ; <mods> u` into a
/// [`KeyEvent`]. `code` is the Unicode codepoint; `mods` is the Kitty
/// modifier bitmask (1=Shift, 2=Alt, 4=Ctrl, 8=Meta, 16=Super, ...).
///
/// Returns `None` if the input is not a valid CSI-u sequence.
pub fn decode_csi_u(input: &str) -> Option<KeyEvent> {
    let body = input.strip_prefix("\u{1b}[")?;
    let inner = body.strip_suffix('u')?;
    let mut parts = inner.split(';');
    let code = parts.next()?.parse::<u32>().ok()?;
    let mods_raw = parts
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);

    let mut modifiers = KeyModifiers::NONE;
    if mods_raw & 1 != 0 {
        modifiers |= KeyModifiers::SHIFT;
    }
    if mods_raw & 2 != 0 {
        modifiers |= KeyModifiers::ALT;
    }
    if mods_raw & 4 != 0 {
        modifiers |= KeyModifiers::CONTROL;
    }
    if mods_raw & 8 != 0 {
        modifiers |= KeyModifiers::META;
    }

    let ch = char::from_u32(code)?;
    if ch.is_ascii_control() || ch.is_whitespace() {
        // CSI-u for control/whitespace keys is handled by the terminal's own
        // escape sequence; do not synthesise a Char event for them.
        None
    } else {
        Some(KeyEvent::new(KeyCode::Char(ch), modifiers))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_csi_u_basic() {
        let ev = decode_csi_u("\u{1b}[120;1u").unwrap();
        assert_eq!(ev.code, KeyCode::Char('x'));
        assert!(ev.modifiers.contains(KeyModifiers::SHIFT));
    }

    #[test]
    fn decode_csi_u_ctrl_alt() {
        let ev = decode_csi_u("\u{1b}[120;6u").unwrap();
        assert_eq!(ev.code, KeyCode::Char('x'));
        assert!(ev.modifiers.contains(KeyModifiers::CONTROL));
        assert!(ev.modifiers.contains(KeyModifiers::ALT));
    }

    #[test]
    fn decode_csi_u_invalid_returns_none() {
        assert!(decode_csi_u("hello").is_none());
        assert!(decode_csi_u("\u{1b}[120").is_none());
    }
}
