use crate::artifacts::types::*;

pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len - 3])
    } else {
        s.to_string()
    }
}

pub fn render_task_spec_summary(spec: &TaskSpec) -> Vec<String> {
    vec![format!("Spec: {} files to modify — {}",
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

pub fn render_test_report_summary(report: &TestReport) -> Vec<String> {
    vec![format!("{}/{} tests passed — {} edge cases identified",
        report.test_results.passed,
        report.test_results.total,
        report.edge_cases_found.len()
    )]
}

pub fn render_review_verdict_summary(verdict: &ReviewVerdict) -> Vec<String> {
    let mut lines = vec![];
    match verdict.verdict {
        Verdict::Approved => {
            lines.push("Verdict: Approved".to_string());
            lines.push(format!("Quality: correctness {}/10 · code quality {}/10 · coverage {}/10",
                verdict.quality_scores.correctness,
                verdict.quality_scores.code_quality,
                verdict.quality_scores.test_coverage,
            ));
        }
        Verdict::RevisionNeeded => {
            let critical_count = verdict.issues.iter()
                .filter(|i| matches!(i.severity, IssueSeverity::Critical | IssueSeverity::Major))
                .count();
            lines.push(format!("Revision needed — {} critical issues found:", critical_count));
            for issue in verdict.issues.iter()
                .filter(|i| matches!(i.severity, IssueSeverity::Critical | IssueSeverity::Major)) {
                lines.push(format!("• {:?}: {}", issue.category, issue.description));
            }
        }
        Verdict::Rejected => {
            lines.push("Verdict: Rejected — escalating to human review".to_string());
        }
    }
    lines
}
