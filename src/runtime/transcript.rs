//! Compressing the loop's own transcript, and saying what was taken.
//!
//! ## Why this is not the memory compressor
//!
//! `memory::compression` compacts *knowledge* between stages and writes a
//! block to disk. Its call site in the pipeline is `let _ =
//! compress_context(…)` — the result is thrown away, which is a fair summary
//! of what it is for: a record, not a repair. **Neither one touches the
//! conversation the model is in.** A Coder that runs sixty tool steps grows
//! its transcript until the provider rejects the request, and nothing in the
//! repository notices until the request fails.
//!
//! ## The rule: never truncate silently
//!
//! §8's warning is the whole design: *"the failure mode to avoid is silent
//! truncation. Every strategy must be counted and reported, so a run that lost
//! context says which strategy took it."*
//!
//! A model whose earlier turns were quietly removed cannot tell that happened.
//! It carries on, confident, reasoning from a conversation that is not the one
//! it had. So [`compress`] returns a [`Report`], and [`Report::is_silent`]
//! exists to be asserted on: a transcript that shrank with an empty report is
//! a bug, and one test is the whole defence.
//!
//! ## Order, and why the cheap things come first
//!
//! Duplicate system messages, then tool results, then file reads, and only
//! then a summarisation call. The first three are exact and free; the fourth
//! costs a request and loses detail, so it is the last resort rather than
//! the first move. `the_strategies_run_in_that_order` is the test.

use crate::runtime::tools::LoopMessage;

/// One strategy, in the order they are tried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    /// Repeated system prompts: identical text, so keeping the first is lossless.
    SnipDuplicateSystem,
    /// Long tool results: keep the head and the tail, mark the middle gone.
    MicrocompactToolResults,
    /// Long file reads: keep the first and last lines, drop the middle.
    CollapseFileReads,
    /// A summarisation call. Not implemented here — see the note above.
    Summarise,
}

/// What one strategy did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    pub strategy: Strategy,
    /// How many turns it touched.
    pub units: usize,
    /// Characters it removed.
    pub chars_saved: usize,
}

impl Applied {
    /// One line, for the transcript. Every strategy names itself: a run that
    /// lost context has to be able to say which strategy took it.
    pub fn to_line(&self) -> String {
        format!(
            "{}: {} turn(s), {} characters removed",
            strategy_name(self.strategy),
            self.units,
            self.chars_saved
        )
    }
}

pub fn strategy_name(s: Strategy) -> &'static str {
    match s {
        Strategy::SnipDuplicateSystem => "snipped duplicate system messages",
        Strategy::MicrocompactToolResults => "microcompacted tool results",
        Strategy::CollapseFileReads => "collapsed long file reads",
        Strategy::Summarise => "summarised the conversation",
    }
}

/// What compression did, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub applied: Vec<Applied>,
    pub chars_before: usize,
    pub chars_after: usize,
}

impl Report {
    /// The line a run prints, or `None` when nothing was done.
    pub fn to_line(&self) -> Option<String> {
        if self.applied.is_empty() {
            return None;
        }
        Some(format!(
            "context compressed: {} → {} characters. {}",
            self.chars_before,
            self.chars_after,
            self.applied
                .iter()
                .map(Applied::to_line)
                .collect::<Vec<_>>()
                .join("; ")
        ))
    }

    /// A transcript that shrank with nothing to say about it.
    ///
    /// The property §8 calls out, as a thing a test can hold rather than a
    /// thing a code comment promises.
    pub fn is_silent(&self) -> bool {
        self.chars_after < self.chars_before && self.applied.is_empty()
    }
}

fn chars_of(messages: &[LoopMessage]) -> usize {
    messages
        .iter()
        .map(|m| match m {
            LoopMessage::System(s) | LoopMessage::User(s) => s.len(),
            LoopMessage::Assistant { content, .. } => content.len(),
            LoopMessage::ToolResult { content, .. } => content.len(),
        })
        .sum()
}

/// Keep the head and the tail of a long text, and say what went.
fn elide(text: &str, head: usize, tail: usize) -> String {
    if text.chars().count() <= head + tail {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let omitted = chars.len() - head - tail;
    let mut out: String = chars[..head].iter().collect();
    out.push_str(&format!(
        "\n… {omitted} characters elided by context compression …\n"
    ));
    out.extend(chars[chars.len() - tail..].iter());
    out
}

/// Compress `messages` in place, and report what was taken.
///
/// The first three strategies are exact and free. `Summarise` is **not
/// implemented** — it needs a model call, and §8 puts it last for that
/// reason. A caller that needs the fourth must say so, and this function
/// reports only what it actually did.
pub fn compress(messages: &mut Vec<LoopMessage>) -> Report {
    let chars_before = chars_of(messages);
    let mut applied = Vec::new();

    // 1. Duplicate system messages. Identical text, so dropping the repeats is
    //    lossless — and it is the one strategy with no downside at all.
    {
        let mut seen: Vec<String> = Vec::new();
        let mut units = 0;
        let mut saved = 0;
        messages.retain(|m| match m {
            LoopMessage::System(s) => {
                if seen.iter().any(|x| x == s) {
                    units += 1;
                    saved += s.len();
                    false
                } else {
                    seen.push(s.clone());
                    true
                }
            }
            _ => true,
        });
        if units > 0 {
            applied.push(Applied {
                strategy: Strategy::SnipDuplicateSystem,
                units,
                chars_saved: saved,
            });
        }
    }

    // 2. Long tool results. A read of a large file is mostly boilerplate; the
    //    head says what the file is and the tail says how it ends.
    {
        const HEAD: usize = 1200;
        const TAIL: usize = 800;
        let mut units = 0;
        let mut saved = 0;
        for m in messages.iter_mut() {
            if let LoopMessage::ToolResult { content, .. } = m
                && content.chars().count() > HEAD + TAIL
            {
                let before = content.len();
                *content = elide(content, HEAD, TAIL);
                saved += before.saturating_sub(content.len());
                units += 1;
            }
        }
        if units > 0 {
            applied.push(Applied {
                strategy: Strategy::MicrocompactToolResults,
                units,
                chars_saved: saved,
            });
        }
    }

    // 3. Long file reads specifically — they are the bulk of a Coder's
    //    transcript and the most redundant of them.
    {
        const HEAD_LINES: usize = 40;
        const TAIL_LINES: usize = 20;
        let mut units = 0;
        let mut saved = 0;
        for m in messages.iter_mut() {
            if let LoopMessage::ToolResult { content, .. } = m {
                let lines: Vec<&str> = content.lines().collect();
                if lines.len() > HEAD_LINES + TAIL_LINES {
                    let before = content.len();
                    let omitted = lines.len() - HEAD_LINES - TAIL_LINES;
                    let mut out: Vec<String> =
                        lines[..HEAD_LINES].iter().map(|s| s.to_string()).collect();
                    out.push(format!("… {omitted} lines elided by context compression …"));
                    out.extend(
                        lines[lines.len() - TAIL_LINES..]
                            .iter()
                            .map(|s| s.to_string()),
                    );
                    *content = out.join("\n");
                    saved += before.saturating_sub(content.len());
                    units += 1;
                }
            }
        }
        if units > 0 {
            applied.push(Applied {
                strategy: Strategy::CollapseFileReads,
                units,
                chars_saved: saved,
            });
        }
    }

    Report {
        applied,
        chars_before,
        chars_after: chars_of(messages),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::provider::ToolCall;

    fn big(n: usize) -> LoopMessage {
        LoopMessage::ToolResult {
            tool_call_id: "c1".into(),
            content: (0..n).map(|i| format!("line {i}\n")).collect(),
        }
    }

    /// §8's rule, as a test rather than a promise: a transcript that shrank
    /// must say so.
    #[test]
    fn a_shrunk_transcript_is_never_silent() {
        let mut messages = vec![big(4000), big(4000)];
        let report = compress(&mut messages);
        assert!(
            report.chars_after < report.chars_before,
            "nothing was saved"
        );
        assert!(
            !report.is_silent(),
            "a transcript that shrank with an empty report is silent \
             truncation: {:?}",
            report
        );
        let line = report.to_line().expect("a report that did work has a line");
        assert!(line.contains("characters"), "{line}");
        assert!(
            line.contains("elided") || line.contains("removed"),
            "{line}"
        );
    }

    /// The cheap strategies first. Summarisation costs a request and loses
    /// detail, so it is last — and until it is built, the report must not
    /// claim it ran.
    #[test]
    fn the_strategies_run_in_that_order() {
        let mut messages = vec![
            LoopMessage::System("you are a coding agent".into()),
            LoopMessage::System("you are a coding agent".into()),
            big(4000),
        ];
        let report = compress(&mut messages);
        let names: Vec<Strategy> = report.applied.iter().map(|a| a.strategy).collect();
        assert_eq!(
            names,
            vec![
                Strategy::SnipDuplicateSystem,
                Strategy::MicrocompactToolResults,
                Strategy::CollapseFileReads
            ],
            "cheap and exact first, and no strategy that was not run: {names:?}"
        );
    }

    /// Duplicate system messages are identical, so dropping the repeats loses
    /// nothing. It is the only strategy with no downside.
    #[test]
    fn duplicate_system_messages_are_dropped_and_the_first_is_kept() {
        let mut messages = vec![
            LoopMessage::System("first".into()),
            LoopMessage::System("first".into()),
            LoopMessage::System("second".into()),
            LoopMessage::User("hi".into()),
        ];
        let report = compress(&mut messages);
        let systems: Vec<&LoopMessage> = messages
            .iter()
            .filter(|m| matches!(m, LoopMessage::System(_)))
            .collect();
        assert_eq!(systems.len(), 2, "one 'first' and one 'second' remain");
        let LoopMessage::System(kept) = systems[0] else {
            panic!("expected a system message");
        };
        assert_eq!(kept, "first", "and the *first* copy is the one kept");
        assert!(report.chars_after < report.chars_before);
    }

    /// The head and the tail survive; the middle says it went.
    #[test]
    fn a_long_result_keeps_its_head_and_its_tail() {
        let mut messages = vec![big(4000)];
        compress(&mut messages);
        let LoopMessage::ToolResult { content, .. } = &messages[0] else {
            panic!("expected a tool result");
        };
        assert!(
            content.contains("line 0\n"),
            "the head must survive: {}",
            &content[..content.len().min(120)]
        );
        assert!(
            content.contains("elided"),
            "and the middle must be accounted for, not just gone"
        );
        assert!(
            !content.contains("line 2000\n"),
            "the middle must actually be gone"
        );
    }

    /// Assistant prose and user messages are **not** compressed. The prose is
    /// the model's own reasoning about the work, and an elided user instruction
    /// is a task that changed without anyone deciding it should.
    #[test]
    fn only_tool_results_are_compressed() {
        let user = "please do the thing ".repeat(500);
        let assistant = "I will reason at length. ".repeat(500);
        let mut messages = vec![
            LoopMessage::User(user.clone()),
            LoopMessage::Assistant {
                content: assistant.clone(),
                tool_calls: vec![ToolCall {
                    id: "c1".into(),
                    name: "read".into(),
                    arguments: serde_json::json!({}),
                }],
            },
            big(4000),
        ];
        compress(&mut messages);
        assert!(
            matches!(&messages[0], LoopMessage::User(u) if *u == user),
            "user text must be untouched: {:?}",
            &messages[0]
        );
        match &messages[1] {
            LoopMessage::Assistant { content: c, .. } => assert_eq!(
                *c, assistant,
                "the model's own prose must be untouched — eliding its reasoning \
                 leaves it reasoning about a conversation it no longer has"
            ),
            other => panic!("{other:?}"),
        }
    }

    /// Nothing to do is not a report. A line on every turn is noise, and a
    /// reader learns to skip it.
    #[test]
    fn a_transcript_with_nothing_to_compress_reports_nothing() {
        let mut messages = vec![
            LoopMessage::User("do a thing".into()),
            LoopMessage::Assistant {
                content: "ok".into(),
                tool_calls: vec![],
            },
        ];
        let before = messages.len();
        let report = compress(&mut messages);
        assert_eq!(messages.len(), before, "and nothing was touched");
        assert!(report.applied.is_empty());
        assert_eq!(report.to_line(), None);
        assert!(!report.is_silent());
    }

    /// The quantitative form of §8's rule: every character that left the
    /// transcript is accounted for by a strategy, and no strategy claims
    /// a character that stayed. A report that is a plausible story rather
    /// than a tally is how a run ends up lying to whoever reads the log.
    #[test]
    fn the_report_accounts_for_exactly_what_was_removed() {
        let mut messages = vec![
            LoopMessage::System("sys".into()),
            LoopMessage::System("sys".into()),
            big(4000),
            big(4000),
        ];
        let report = compress(&mut messages);
        let claimed: usize = report.applied.iter().map(|a| a.chars_saved).sum();
        assert_eq!(
            claimed,
            report.chars_before - report.chars_after,
            "the strategies must account for the whole difference, with no \
             unclaimed remainder: {report:?}"
        );
        for a in &report.applied {
            assert!(
                a.units > 0 && a.chars_saved > 0,
                "a strategy that removed nothing must not appear in the report: {a:?}"
            );
            assert!(
                a.strategy != Strategy::Summarise,
                "summarisation is not implemented, so a run must never report it: {a:?}"
            );
        }
    }

    /// A file cannot fake its way past the compressor by quoting the marker.
    ///
    /// The obvious cheap guard here is to skip any content that already contains
    /// the elision note. That is a hole: anything the model reads — a fixture, a
    /// README, a hostile file — could include one line of that text and carry
    /// unlimited payload through untouched, silently. So the note is text, not
    /// state, and content containing it is compressed like anything else.
    #[test]
    fn content_that_quotes_the_marker_is_still_compressed() {
        let mut messages = vec![LoopMessage::ToolResult {
            tool_call_id: "c1".into(),
            content: "… 1 characters elided by context compression …\n".to_string()
                + &(0..4000).map(|i| format!("line {i}\n")).collect::<String>(),
        }];
        let report = compress(&mut messages);
        let LoopMessage::ToolResult { content, .. } = &messages[0] else {
            panic!("expected a result")
        };
        assert!(
            report.chars_after < report.chars_before,
            "text that quotes the marker must be compressed like anything else: {report:?}"
        );
        assert!(
            !content.contains("line 2000\n"),
            "the middle must actually be gone"
        );
    }
}
