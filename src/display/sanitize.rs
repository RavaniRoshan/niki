//! Terminal-output sanitisation.
//!
//! NIKI streams model tokens straight to the user's terminal, and those tokens
//! are influenced by repository content (`AGENTS.md`, source files, issue text)
//! and by artifact fields the model wrote. Without a filter, any of those can
//! carry a terminal control sequence that acts on the user's machine rather
//! than on NIKI:
//!
//! | Sequence | Effect |
//! |---|---|
//! | `ESC ] 52 ; c ; <base64>` | overwrite the system **clipboard** |
//! | `ESC ] 0 ; <text>` / `ESC ] 2 ;` | rewrite the **terminal title** |
//! | `ESC [ 2 J` | **clear scrollback** |
//! | `ESC [ ? 1049 h` | enter the **alternate screen**, leaving the user stranded |
//! | `ESC ( B` | switch the charset, scrambling everything already on screen |
//! | `\r` | rewrite the current line, hiding what was printed |
//!
//! `redact_secrets` already exists but was only applied to provider error
//! strings; this is the complementary control for terminal control.
//!
//! The rule is deliberately conservative: NIKI's own legitimate output is
//! plain text (the TUI renders through ratatui, which owns its own escape
//! sequences). Any control sequence arriving from a model or a repository is
//! by definition not ours, so it is removed rather than reasoned about.

/// What to do with a single non-ESC character.
enum Disposition {
    /// Emit the character unchanged — printable text, newline, tab, Unicode.
    Pass,
    /// Emit a visible marker so the user can tell something was stripped.
    Marker(&'static str),
    /// Drop it entirely.
    Drop,
}

fn disposition_of(c: char) -> Disposition {
    if c == '\r' {
        Disposition::Marker("<CR>")
    } else if c == '\u{7f}' || c == '\u{0}' {
        Disposition::Drop
    } else if (c as u32) < 0x20 && c != '\n' && c != '\t' {
        // Remaining C0 controls (BEL, BS, VT, FF, ...). BEL in particular is
        // the OSC terminator and carries no text of its own.
        Disposition::Drop
    } else {
        Disposition::Pass
    }
}

/// Strip every terminal control sequence from `input`.
///
/// Preserves `\n` and `\t`. ANSI SGR colour codes are also stripped: NIKI's
/// plain-text paths do not emit colour, and a model-supplied SGR can repaint
/// the user's scrollback.
pub fn sanitize_for_terminal(input: impl AsRef<str>) -> String {
    let input = input.as_ref();
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            match disposition_of(c) {
                Disposition::Pass => out.push(c),
                Disposition::Marker(m) => out.push_str(m),
                Disposition::Drop => {}
            }
            continue;
        }
        // ESC — consume the whole sequence, whatever form it takes.
        match chars.peek() {
            // CSI: parameter and intermediate bytes, then a final byte @..~
            Some('[') => {
                chars.next();
                while let Some(c) = chars.peek().copied() {
                    // A parameter byte can never be ESC, so seeing one means a
                    // nested sequence. Leave it in the stream for the outer
                    // loop — consuming it here let the nested OSC's payload
                    // leak through as visible text.
                    if c == '\u{1b}' {
                        break;
                    }
                    chars.next();
                    if ('\u{40}'..='\u{7e}').contains(&c) {
                        // Final byte consumed; the sequence is complete.
                        break;
                    }
                }
            }
            // OSC / DCS / SOS / PM / APC: terminated by BEL or ST (ESC \).
            Some(']') | Some('P') | Some('X') | Some('^') | Some('_') => {
                chars.next();
                loop {
                    // Peek rather than consume: a nested ESC is not a
                    // terminator, and treating it as one leaked the rest of the
                    // payload into the terminal as visible text.
                    match chars.peek() {
                        Some('\u{7}') => {
                            chars.next();
                            break;
                        }
                        Some('\u{1b}') => {
                            // ST is ESC \. Anything else after ESC inside a
                            // string is a nested sequence — stop here and let
                            // the outer loop handle that ESC.
                            if chars.clone().nth(1) == Some('\\') {
                                chars.next();
                                chars.next();
                            }
                            break;
                        }
                        Some(_) => {
                            chars.next();
                        }
                        None => break,
                    }
                }
            }
            // ESC with intermediate bytes (0x20..0x2F) then a final byte, e.g.
            // `ESC ( B` (charset designator) and `ESC # 8` (DEC line size).
            Some(c) if ('\u{20}'..='\u{2f}').contains(c) => {
                for c in chars.by_ref() {
                    if ('\u{30}'..='\u{7e}').contains(&c) {
                        break;
                    }
                }
            }
            // ESC plus a single final byte, or nothing at all.
            Some(_) => {
                chars.next();
            }
            None => {}
        }
    }
    out
}

/// Sanitise a whole line, and collapse the NUL byte that would truncate the
/// string in several C consumers.
pub fn sanitize_line(input: impl AsRef<str>) -> String {
    sanitize_for_terminal(input).replace('\u{0}', "")
}

/// Split `input` at a byte offset, respecting UTF-8 boundaries.
///
/// Used by tests that assert a payload split across two streamed tokens is
/// still defused once reassembled. Exposed here rather than in the test so the
/// split point is chosen the same way the streaming layer would.
pub fn split_at(input: &str, byte: usize) -> (String, String) {
    let b = input.as_bytes();
    let mut cut = byte.min(b.len());
    while cut > 0 && !input.is_char_boundary(cut) {
        cut -= 1;
    }
    (
        String::from_utf8_lossy(&b[..cut]).to_string(),
        String::from_utf8_lossy(&b[cut..]).to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc52_clipboard_write_is_removed() {
        let attack = "before \u{1b}]52;c;cHlwZWNiZXNlcnJldGVk\u{7} after";
        let out = sanitize_for_terminal(attack);
        assert!(!out.contains('\u{1b}'), "ESC survived: {out:?}");
        assert!(!out.contains("52;c"), "OSC 52 payload survived: {out:?}");
        assert!(
            out.contains("before") && out.contains("after"),
            "text was lost: {out:?}"
        );
    }

    #[test]
    fn osc_terminated_by_st_is_removed() {
        // Some terminals emit ESC \ (ST) instead of BEL.
        let attack = "x\u{1b}]0;pwned\u{1b}\\y";
        let out = sanitize_for_terminal(attack);
        assert!(!out.contains("pwned"), "{out:?}");
        assert!(!out.contains('\u{1b}'));
    }

    #[test]
    fn title_rewrite_is_removed() {
        assert!(!sanitize_for_terminal("a\u{1b}]0;title\u{7}b").contains("title"));
        assert!(!sanitize_for_terminal("a\u{1b}]2;title\u{7}b").contains("title"));
    }

    #[test]
    fn csi_screen_clear_is_removed() {
        assert_eq!(sanitize_for_terminal("a\u{1b}[2Jb"), "ab");
    }

    #[test]
    fn csi_alternate_screen_is_removed() {
        // Would leave the user stranded in a full-screen app they cannot exit.
        let out = sanitize_for_terminal("x\u{1b}[?1049hy");
        assert_eq!(out, "xy");
    }

    #[test]
    fn sgr_colour_codes_are_removed() {
        // A model-supplied colour can repaint the user's scrollback.
        assert_eq!(sanitize_for_terminal("\u{1b}[31mred\u{1b}[0m"), "red");
    }

    #[test]
    fn carriage_return_line_rewrite_is_neutralised() {
        // Backspace-overwrite: a naive terminal renders `\b\b\bpwned` as
        // "pwned" sitting on top of "safe". Dropping the backspaces defeats
        // the attack — "pwned" then appears as literal text, which is honest.
        let out = sanitize_line("safe\u{8}\u{8}\u{8}\u{8}pwned");
        assert!(!out.contains('\u{8}'), "backspace survived: {out:?}");
        assert_eq!(out, "safepwned");
    }

    #[test]
    fn charset_switch_is_removed() {
        assert_eq!(sanitize_for_terminal("a\u{1b}(Bb"), "ab");
    }

    #[test]
    fn bell_alone_is_dropped() {
        assert_eq!(sanitize_for_terminal("ding\u{7}"), "ding");
    }

    #[test]
    fn nul_is_stripped() {
        assert_eq!(sanitize_line("a\u{0}b"), "ab");
    }

    #[test]
    fn newlines_and_tabs_survive() {
        // These are legitimate in model output and in diffs.
        let text = "line one\n\tindented\nline two";
        assert_eq!(sanitize_for_terminal(text), text);
    }

    #[test]
    fn ordinary_diff_text_is_untouched() {
        let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,4 @@\n pub fn f() {}\n";
        assert_eq!(sanitize_for_terminal(diff), diff);
    }

    #[test]
    fn unicode_content_survives() {
        let text = "héllo ✅ 世界 🌍 é́ combining";
        assert_eq!(sanitize_for_terminal(text), text);
    }

    #[test]
    fn a_truncated_escape_at_end_of_input_does_not_hang() {
        // A stream cut mid-escape must not loop forever.
        assert_eq!(sanitize_for_terminal("abc\u{1b}"), "abc");
        assert_eq!(sanitize_for_terminal("abc\u{1b}["), "abc");
        assert_eq!(sanitize_for_terminal("abc\u{1b}]"), "abc");
        assert_eq!(sanitize_for_terminal("abc\u{1b}]52;c;abc"), "abc");
    }

    #[test]
    fn nested_and_repeated_escapes_are_all_removed() {
        let attack = "\u{1b}]\u{1b}[\u{1b}]52;c;x\u{1b}\u{1b}\u{1b}[0m\u{1b}]2;t\u{7}tail";
        let out = sanitize_for_terminal(attack);
        assert_eq!(out, "tail");
    }
}
