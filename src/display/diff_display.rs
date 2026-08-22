use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use similar::{ChangeTag, TextDiff};

use crate::display::theme;

/// Render a unified diff into styled lines using Codex-style conventions:
/// `gutter (line number) │ sign │ content`, a full-width background tint on
/// added/removed lines, `DIM` on deletions so syntax color does not overpower
/// the removal cue, and word-level intra-line emphasis. On ANSI-16 / no-color
/// terminals backgrounds are dropped (foreground-only) so syntax tokens stay
/// readable instead of being swallowed by saturated blocks.
pub fn render_diff(diff: &str, term_width: u16) -> Vec<Line<'static>> {
    let depth = theme::ColorDepth::detect();
    let use_bg = !matches!(
        depth,
        theme::ColorDepth::Ansi16 | theme::ColorDepth::NoColor
    );
    let width = term_width as usize;

    let add_bg = theme::DIFF_ADD_BG();
    let del_bg = theme::DIFF_DEL_BG();
    let add_fg = theme::DIFF_ADD_FG();
    let del_fg = theme::DIFF_DEL_FG();

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut old_line = 0u32;
    let mut new_line = 0u32;
    let mut prev_deletion: Option<String> = None;

    for raw in diff.lines() {
        if raw.starts_with("diff --git") || raw.starts_with("index ") {
            lines.push(Line::from(Span::styled(
                raw.to_string(),
                Style::default()
                    .fg(theme::BLUE())
                    .add_modifier(Modifier::BOLD),
            )));
        } else if raw.starts_with("--- ") || raw.starts_with("+++ ") {
            lines.push(Line::from(Span::styled(
                raw.to_string(),
                Style::default()
                    .fg(theme::fg_dim())
                    .add_modifier(Modifier::BOLD),
            )));
        } else if raw.starts_with("@@") {
            lines.push(Line::from(Span::styled(
                raw.to_string(),
                Style::default().fg(theme::DIFF_HUNK()),
            )));
            if let Some((o, n)) = parse_hunk_header(raw) {
                old_line = o;
                new_line = n;
            }
        } else if let Some(content) = raw.strip_prefix('+') {
            let mut spans = Vec::new();
            if use_bg {
                spans.push(Span::styled(
                    format!("{:>4} + ", new_line),
                    Style::default().fg(theme::fg_dim()).bg(add_bg),
                ));
            } else {
                spans.push(Span::styled(
                    format!("{:>4} + ", new_line),
                    Style::default().fg(theme::fg_dim()),
                ));
            }
            if let Some(prev) = prev_deletion.take() {
                for change in TextDiff::from_words(prev.as_str(), content).iter_all_changes() {
                    match change.tag() {
                        ChangeTag::Equal => spans.push(Span::styled(
                            format!(" {}", change.value()),
                            style_for(add_fg, use_bg.then_some(add_bg)),
                        )),
                        ChangeTag::Delete => spans.push(Span::styled(
                            format!("-{}", change.value()),
                            Style::default()
                                .fg(theme::error())
                                .add_modifier(Modifier::BOLD),
                        )),
                        ChangeTag::Insert => spans.push(Span::styled(
                            format!("+{}", change.value()),
                            style_for(add_fg, use_bg.then_some(add_bg))
                                .add_modifier(Modifier::BOLD),
                        )),
                    }
                }
            } else {
                spans.push(Span::styled(
                    content.to_string(),
                    style_for(add_fg, use_bg.then_some(add_bg)),
                ));
            }
            if use_bg {
                pad(&mut spans, add_bg, width);
            }
            lines.push(Line::from(spans));
            new_line += 1;
        } else if let Some(content) = raw.strip_prefix('-') {
            let mut spans = Vec::new();
            if use_bg {
                spans.push(Span::styled(
                    format!("{:>4} - ", old_line),
                    Style::default().fg(theme::fg_dim()).bg(del_bg),
                ));
            } else {
                spans.push(Span::styled(
                    format!("{:>4} - ", old_line),
                    Style::default().fg(theme::fg_dim()),
                ));
            }
            spans.push(Span::styled(
                content.to_string(),
                style_for(del_fg, use_bg.then_some(del_bg)).add_modifier(Modifier::DIM),
            ));
            if use_bg {
                pad(&mut spans, del_bg, width);
            }
            lines.push(Line::from(spans));
            prev_deletion = Some(content.to_string());
            old_line += 1;
        } else {
            prev_deletion = None;
            let mut spans = Vec::new();
            spans.push(Span::styled(
                format!("{:>4} {:>4} ", old_line, new_line),
                Style::default().fg(theme::fg_dim()),
            ));
            spans.push(Span::styled(
                format!(" {}", raw),
                Style::default().fg(theme::fg_color()),
            ));
            lines.push(Line::from(spans));
            old_line += 1;
            new_line += 1;
        }
    }
    lines
}

fn style_for(fg: Color, bg: Option<Color>) -> Style {
    match bg {
        Some(b) => Style::default().fg(fg).bg(b),
        None => Style::default().fg(fg),
    }
}

fn pad(spans: &mut Vec<Span<'static>>, bg: Color, width: usize) {
    if width == 0 {
        return;
    }
    let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    if width > used {
        spans.push(Span::styled(
            " ".repeat(width - used),
            Style::default().bg(bg),
        ));
    }
}

/// Parse a `@@ -old,count +new,count @@` hunk header, returning (old_start, new_start).
fn parse_hunk_header(header: &str) -> Option<(u32, u32)> {
    let rest = header.strip_prefix("@@ -")?;
    let old_start = rest
        .split(' ')
        .next()?
        .split(',')
        .next()?
        .parse::<u32>()
        .ok()?;
    let after_old = rest.split('+').nth(1)?;
    let new_start = after_old
        .split(' ')
        .next()?
        .split(',')
        .next()?
        .parse::<u32>()
        .ok()?;
    Some((old_start, new_start))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
diff --git a/src/main.rs b/src/main.rs
index abc123..def456 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,5 +1,6 @@
 use std::io;
+use std::env;

 fn main() {
+    let args: Vec<String> = env::args().collect();
     println!(\"hello\");
 }";

    #[test]
    fn renders_nonempty_with_structure() {
        let lines = render_diff(SAMPLE, 80);
        assert!(!lines.is_empty());
        let joined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(joined.contains("use std::env;"));
        assert!(joined.contains("let args:"));
    }

    #[test]
    fn zero_width_renders_without_panic() {
        let lines = render_diff(SAMPLE, 0);
        let joined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(joined.contains("use std::env;"));
    }

    #[test]
    fn parse_hunk_header_works() {
        assert_eq!(parse_hunk_header("@@ -10,7 +15,9 @@").unwrap(), (10, 15));
        assert_eq!(parse_hunk_header("@@ -1 +1 @@").unwrap(), (1, 1));
        assert!(parse_hunk_header("not a hunk").is_none());
    }
}
