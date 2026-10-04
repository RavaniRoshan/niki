use crate::artifacts::types::*;
use unicode_truncate::UnicodeTruncateStr;

pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        // UnicodeTruncateStr avoids slicing in the middle of a multi-byte char
        // (which previously panicked on e.g. em-dashes / non-Latin text).
        format!("{}...", s.unicode_truncate(max_len - 3).0)
    } else {
        s.to_string()
    }
}

pub fn render_task_spec_summary(spec: &TaskSpec) -> Vec<String> {
    vec![format!(
        "Spec: {} files to modify — {}",
        spec.files_to_modify.len(),
        truncate(&spec.summary, 60)
    )]
}

pub fn render_code_diff_summary(diff: &CodeDiff) -> Vec<String> {
    let mut lines = vec![format!("Changed {} files", diff.files_changed.len())];
    for file in &diff.files_changed {
        let tag = match file.action {
            FileAction::Create => "[new file]",
            FileAction::Modify => "[modified]",
            FileAction::Delete => "[deleted]",
        };
        lines.push(format!("  {}  {}", file.path, tag));
    }
    lines
}

/// The Tester's line, attributed to the Tester.
///
/// The counts here come from the model's own artifact. They are *its*
/// accounting, and NIKI does not measure per-test counts — it runs the suite
/// and gets an exit code. The distinction is load-bearing, and it was not
/// visible: the line used to read `4/4 tests passed`, which on a real
/// `nvidia/nemotron-3-super-120b-a12b` run accompanied a Tester that claimed
/// four passing tests, named the file it wrote them to, and had written no
/// file at all. The suite ran `1 passed`. The branch was still gated on the
/// real exit code, so nothing unsafe shipped — but the number a user reads was
/// the model's claim, stated as fact.
///
/// So the counts are named as a report. The measured result is printed
/// separately, from `TestExecution`, by `render_verification_line`.
pub fn render_test_report_summary(report: &TestReport) -> Vec<String> {
    vec![format!(
        "Tester reported {}/{} tests passed — {} edge cases identified",
        report.test_results.passed,
        report.test_results.total,
        report.edge_cases_found.len()
    )]
}

/// What NIKI actually measured, when it ran the suite itself.
///
/// This is the only line in the Tester output backed by something NIKI
/// executed rather than something a model wrote. It deliberately reports the
/// exit code rather than a pass count: counting individual tests means parsing
/// each runner's output format, and a parser that guesses is worse than no
/// number. `None` when no suite ran — in which case the Tester's line stands
/// alone and is visibly unattributed to a measurement that never happened.
pub fn render_verification_line(
    execution: Option<&crate::agents::tester::TestExecution>,
) -> Option<String> {
    let te = execution?;
    // No evidence, no line. A project with no resolvable test command used to
    // arrive here as `None`; it now arrives as an explicit `Unverified`
    // record, and printing it as "exited -1 — suite failed" would be a
    // fabricated failure for something that was never run.
    if !te.status.is_verified() {
        return None;
    }
    Some(if te.passed {
        format!("Verified: `{}` exited 0", te.command)
    } else {
        format!(
            "Verified: `{}` exited {} — suite failed",
            te.command, te.exit_code
        )
    })
}

pub fn render_review_verdict_summary(verdict: &ReviewVerdict) -> Vec<String> {
    let mut lines = vec![];
    match verdict.verdict {
        Verdict::Approved => {
            lines.push("Verdict: Approved".to_string());
            lines.push(format!(
                "Quality: correctness {}/10 · code quality {}/10 · coverage {}/10",
                verdict.quality_scores.correctness,
                verdict.quality_scores.code_quality,
                verdict.quality_scores.test_coverage,
            ));
        }
        Verdict::RevisionNeeded => {
            let critical_count = verdict
                .issues
                .iter()
                .filter(|i| matches!(i.severity, IssueSeverity::Critical | IssueSeverity::Major))
                .count();
            lines.push(format!(
                "Revision needed — {} critical issues found:",
                critical_count
            ));
            for issue in verdict
                .issues
                .iter()
                .filter(|i| matches!(i.severity, IssueSeverity::Critical | IssueSeverity::Major))
            {
                lines.push(format!("• {:?}: {}", issue.category, issue.description));
            }
        }
        Verdict::Rejected => {
            lines.push("Verdict: Rejected — escalating to human review".to_string());
        }
    }
    lines
}

pub fn render_synthesis_summary(s: &Synthesis) -> Vec<String> {
    vec![
        format!(
            "Synthesized {} coder branches → {} files changed",
            s.sources_merged,
            s.merged.files_changed.len()
        ),
        truncate(&s.reconciliation_notes, 80),
    ]
}

pub fn render_security_verdict_summary(v: &SecurityVerdict) -> Vec<String> {
    let mut lines = vec![match v.verdict {
        Verdict::Approved => "Security: Passed".to_string(),
        Verdict::RevisionNeeded => "Security: Changes requested".to_string(),
        Verdict::Rejected => "Security: Blocked".to_string(),
    }];
    let critical = v
        .findings
        .iter()
        .filter(|f| {
            matches!(
                f.severity,
                SecuritySeverity::Critical | SecuritySeverity::High
            )
        })
        .count();
    if critical > 0 {
        lines.push(format!("{} critical/high findings:", critical));
        for f in v.findings.iter().filter(|f| {
            matches!(
                f.severity,
                SecuritySeverity::Critical | SecuritySeverity::High
            )
        }) {
            lines.push(format!("• {:?}: {}", f.category, f.description));
        }
    }
    lines
}

pub fn render_red_challenge_summary(c: &RedChallenge) -> Vec<String> {
    let mut lines = vec![format!(
        "Red challenge: {} point(s) raised",
        c.challenges.len()
    )];
    for p in c.challenges.iter() {
        lines.push(format!(
            "• [{}] {:?}/{:?} (conf {}): {}",
            p.id,
            p.severity,
            p.category,
            p.confidence,
            truncate(&p.claim, 80)
        ));
    }
    lines
}

pub fn render_critique_summary(c: &Critique) -> Vec<String> {
    let mut lines = vec![match c.disposition {
        CriticDisposition::Approve => "Critic: verdict stands".to_string(),
        CriticDisposition::Reject => format!(
            "Critic: rejects verdict — {} unsupported claim(s)",
            c.unsupported_claims.len()
        ),
    }];
    for claim in c.unsupported_claims.iter().take(3) {
        lines.push(format!("• ungrounded: {}", truncate(claim, 80)));
    }
    lines.push(truncate(&c.summary, 100));
    lines
}
