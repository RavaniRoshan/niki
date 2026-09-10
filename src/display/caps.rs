//! Terminal capability detection (TUI-021).
//!
//! Two layers, in increasing cost:
//!
//! 1. **Environment matrix** ([`Capabilities`]): synchronous env sniffing for
//!    hyperlink (OSC 8) support. Unknown terminals default to *no* hyperlinks
//!    — an unsupported OSC 8 sequence renders as garbage, while the fallback
//!    (plain underlined URL) always works. Override with
//!    `NIKI_HYPERLINKS=0|1`.
//! 2. **Live query parsers** ([`parse_osc11_response`], [`parse_scheme_response`]):
//!    pure parsers for OSC 11 background-color and CSI ? 997 color-scheme
//!    replies. The actual query round-trip is a follow-up: it must integrate
//!    with the crossterm event loop (raw mode + concurrent stdin reads risk
//!    swallowing user input), so it is deliberately not wired here yet.

/// Detected terminal capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// Whether the terminal renders OSC 8 hyperlinks.
    pub hyperlinks: bool,
}

/// Detect capabilities from the environment. See module docs for the
/// conservative-unknown policy.
pub fn capabilities() -> Capabilities {
    if let Ok(v) = std::env::var("NIKI_HYPERLINKS") {
        let v = v.to_ascii_lowercase();
        if v == "1" || v == "true" || v == "yes" {
            return Capabilities { hyperlinks: true };
        }
        if v == "0" || v == "false" || v == "no" {
            return Capabilities { hyperlinks: false };
        }
    }
    Capabilities {
        hyperlinks: detect_hyperlinks(),
    }
}

fn detect_hyperlinks() -> bool {
    let term = std::env::var("TERM")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let program = std::env::var("TERM_PROGRAM")
        .unwrap_or_default()
        .to_ascii_lowercase();
    // Known-bad terminals first.
    if term == "dumb" || term == "linux" || term.starts_with("screen") {
        return false;
    }
    if std::env::var("TERMINAL_EMULATOR")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .contains("jetbrains")
    {
        return false;
    }
    // Explicit capability signals.
    if std::env::var("KITTY_WINDOW_ID").is_ok()
        || std::env::var("ITERM_SESSION_ID").is_ok()
        || std::env::var("WT_SESSION").is_ok()
        || std::env::var("WEZTERM_PANE").is_ok()
        || std::env::var("GHOSTTY_RESOURCES_DIR").is_ok()
        || std::env::var("VSCODE_INJECTION").is_ok()
        || std::env::var("VTE_VERSION").is_ok()
    {
        return true;
    }
    if term.contains("kitty")
        || term.contains("ghostty")
        || term.contains("wezterm")
        || term.contains("alacritty")
        || term.contains("foot")
    {
        return true;
    }
    if program.contains("kitty")
        || program.contains("ghostty")
        || program.contains("wezterm")
        || program.contains("vscode")
        || program.contains("warpterminal")
        || program.contains("iterm")
        || program.contains("zed")
    {
        return true;
    }
    false
}

/// Scale a 1–4 digit hex channel to 8 bits (X11 convention: full-range
/// scaling, so `f` → `ff` and `1` → `11`).
fn scale_hex(s: &str) -> Option<u8> {
    if s.is_empty() || s.len() > 4 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    let max = (1u32 << (s.len() * 4)) - 1;
    Some((v * 255 / max) as u8)
}

/// Parse an OSC 11 background-color reply body into 8-bit RGB.
/// Accepts `#rrggbb`, `#rrrrggggbbbb`, and `rgb:r/g/b` with 1–4 hex
/// digits per channel.
pub fn parse_osc11_response(body: &str) -> Option<(u8, u8, u8)> {
    let body = body.trim();
    if let Some(hex) = body.strip_prefix('#') {
        let (r, g, b) = match hex.len() {
            6 => (&hex[0..2], &hex[2..4], &hex[4..6]),
            12 => (&hex[0..4], &hex[4..8], &hex[8..12]),
            _ => return None,
        };
        return Some((scale_hex(r)?, scale_hex(g)?, scale_hex(b)?));
    }
    if let Some(rgb) = body.strip_prefix("rgb:") {
        let mut it = rgb.split('/');
        let (r, g, b) = (it.next()?, it.next()?, it.next()?);
        if it.next().is_some() {
            return None;
        }
        return Some((scale_hex(r)?, scale_hex(g)?, scale_hex(b)?));
    }
    None
}

/// Whether an sRGB background is perceptually dark (luminance threshold).
pub fn is_dark_background(r: u8, g: u8, b: u8) -> bool {
    let lum = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lum(r) + 0.7152 * lum(g) + 0.0722 * lum(b) < 0.179
}

/// Parse a CSI ? 997 color-scheme reply parameter: `1` = dark, `2` = light.
/// Returns `Some(dark)`.
pub fn parse_scheme_response(param: &str) -> Option<bool> {
    match param.trim() {
        "1" => Some(true),
        "2" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes env-mutating tests (same pattern as theme `MODE_TEST_LOCK`).
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct EnvGuard {
        vars: Vec<(&'static str, Option<String>)>,
    }

    impl EnvGuard {
        fn take(names: &[&'static str]) -> EnvGuard {
            let vars = names.iter().map(|n| (*n, std::env::var(n).ok())).collect();
            EnvGuard { vars }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (name, prev) in self.vars.drain(..) {
                unsafe {
                    match prev {
                        Some(v) => std::env::set_var(name, v),
                        None => std::env::remove_var(name),
                    }
                }
            }
        }
    }

    const VARS: &[&str] = &[
        "NIKI_HYPERLINKS",
        "TERM",
        "TERM_PROGRAM",
        "TERMINAL_EMULATOR",
        "KITTY_WINDOW_ID",
        "ITERM_SESSION_ID",
        "WT_SESSION",
        "WEZTERM_PANE",
        "GHOSTTY_RESOURCES_DIR",
        "VSCODE_INJECTION",
        "VTE_VERSION",
    ];

    fn set(var: &str, val: &str) {
        unsafe { std::env::set_var(var, val) };
    }

    fn unset(var: &str) {
        unsafe { std::env::remove_var(var) };
    }

    #[test]
    fn hyperlinks_matrix() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = EnvGuard::take(VARS);
        for v in VARS {
            unset(v);
        }
        // Unknown terminal: conservative false.
        set("TERM", "xterm-256color");
        assert!(!capabilities().hyperlinks);
        // Known-good signals.
        set("KITTY_WINDOW_ID", "1");
        assert!(capabilities().hyperlinks);
        unset("KITTY_WINDOW_ID");
        set("TERM", "xterm-kitty");
        assert!(capabilities().hyperlinks);
        set("TERM_PROGRAM", "vscode");
        set("TERM", "dumb");
        // Known-bad wins over known-good program? dumb TERM short-circuits.
        assert!(!capabilities().hyperlinks);
        // Explicit override wins over everything.
        set("NIKI_HYPERLINKS", "1");
        assert!(capabilities().hyperlinks);
        set("NIKI_HYPERLINKS", "0");
        assert!(!capabilities().hyperlinks);
    }

    #[test]
    fn osc11_parsing() {
        assert_eq!(parse_osc11_response("#1a2b3c"), Some((0x1a, 0x2b, 0x3c)));
        assert_eq!(
            parse_osc11_response("#1a1a2b2b3c3c"),
            Some((0x1a, 0x2b, 0x3c))
        );
        assert_eq!(
            parse_osc11_response("rgb:1a/2b/3c"),
            Some((0x1a, 0x2b, 0x3c))
        );
        assert_eq!(parse_osc11_response("rgb:1/2/3"), Some((0x11, 0x22, 0x33)));
        assert_eq!(
            parse_osc11_response("rgb:1a1a/2b2b/3c3c"),
            Some((0x1a, 0x2b, 0x3c))
        );
        assert_eq!(parse_osc11_response("nope"), None);
        assert_eq!(parse_osc11_response("#12345"), None);
        assert_eq!(parse_osc11_response("rgb:1/2"), None);
    }

    #[test]
    fn darkness_threshold() {
        assert!(is_dark_background(0x1a, 0x1b, 0x2e));
        assert!(!is_dark_background(0xff, 0xff, 0xff));
        assert!(!is_dark_background(0xf5, 0xf0, 0xe6));
    }

    #[test]
    fn scheme_parsing() {
        assert_eq!(parse_scheme_response("1"), Some(true));
        assert_eq!(parse_scheme_response("2"), Some(false));
        assert_eq!(parse_scheme_response("0"), None);
        assert_eq!(parse_scheme_response(""), None);
    }
}
