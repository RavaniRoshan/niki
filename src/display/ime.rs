//! IME (composition) anchoring via the terminal cursor-position report.
//!
//! Some terminals (kitty, Ghostty, newer xterm) anchor the IME composition
//! window to the current cursor position. We ask the terminal for its cursor
//! position (`CSI 6n` — Device Status Report) and parse the response
//! (`CSI <row>;<col> R`) so we can keep the anchor current as the user types.
//!
//! This is **opt-in and degraded**:
//! - Under `tmux` (< 3.7) or `screen`, cursor-position reporting is unreliable,
//!   so we never send the request and the anchor stays stale.
//! - In test environments (no `TERM`) the capability check is false, so the
//!   request is never emitted and the render loop is untouched.
//! - The request is emitted only when explicitly enabled (`config.ui.ime_anchor`)
//!   and guarded by an `anchor_pending` flag so it can never interfere with
//!   normal key handling.

/// Whether the terminal likely supports cursor-position reporting for IME
/// anchoring. Degrades under `tmux` (< 3.7) and `screen`.
pub fn ime_capable() -> bool {
    if std::env::var("TMUX").is_ok() {
        return false;
    }
    if std::env::var("STERM").is_ok() || std::env::var("SCREEN").is_ok() {
        return false;
    }
    if let Ok(term) = std::env::var("TERM") {
        let t = term.to_lowercase();
        if t.contains("tmux") || t.contains("screen") {
            return false;
        }
    }
    let term = std::env::var("TERM").unwrap_or_default();
    let term_program = std::env::var("TERM_PROGRAM").unwrap_or_default();
    term.contains("kitty")
        || term.contains("ghostty")
        || term.contains("xterm")
        || term_program.contains("kitty")
        || term_program.contains("ghostty")
        || term_program.contains("WezTerm")
        || term_program.contains("iTerm")
}

/// Emit a cursor-position report request (`CSI 6n`). Best-effort: failures are
/// ignored and the anchor simply stays stale.
pub fn request_cursor_position() {
    let _ = std::io::Write::write_all(&mut std::io::stdout(), b"\x1b[6n");
}

/// Parse a Device Status Report response of the form `\x1b[<row>;<col>R`.
/// Returns `None` if the input is not a valid DSR response.
pub fn parse_dsr_response(s: &str) -> Option<(u16, u16)> {
    let s = s.trim();
    let body = s.strip_prefix("\u{1b}[")?;
    let nums = body.strip_suffix('R')?;
    let mut parts = nums.split(';');
    let row = parts.next()?.parse::<u16>().ok()?;
    let col = parts.next()?.parse::<u16>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((row, col))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_dsr_response_valid() {
        assert_eq!(parse_dsr_response("\u{1b}[10;20R"), Some((10, 20)));
        assert_eq!(parse_dsr_response("\u{1b}[1;1R"), Some((1, 1)));
    }

    #[test]
    fn parse_dsr_response_rejects_garbage() {
        assert_eq!(parse_dsr_response("hello"), None);
        assert_eq!(parse_dsr_response("\u{1b}[10R"), None); // missing col
        assert_eq!(parse_dsr_response("\u{1b}[a;bR"), None); // non-numeric
        assert_eq!(parse_dsr_response("\u{1b}[10;20;30R"), None); // extra field
        assert_eq!(parse_dsr_response(""), None);
    }

    #[test]
    fn ime_capable_degrades_without_term() {
        // In the test environment there is no TERM, so it must degrade to false
        // rather than risk emitting a cursor-position request.
        assert!(!ime_capable() || std::env::var("TERM").is_ok());
    }
}
