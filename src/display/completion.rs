use crate::display::theme::Theme;
use crate::orchestrator::pipeline::PipelineResult;
use crate::orchestrator::state::PipelineState;
use crate::NikiError;
use crate::artifacts::types::Verdict;
use std::path::Path;

pub fn render_completion(
    result: &PipelineResult,
    theme: &Theme,
    is_tty: bool,
    branch: &str,
    task_dir: &Path,
) {
    if !is_tty {
        let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let verdict = match result.verdict {
            Verdict::Approved => "Approved",
            Verdict::Rejected => "Rejected",
            _ => "Unknown",
        };
        println!(
            "[{}] [NIKI] Task complete — Branch: {} | Verdict: {} | Revisions: {}",
            ts, branch, verdict, result.revision_rounds
        );
        println!(
            "[{}] [NIKI] Patch: {} | Report: {}",
            ts,
            task_dir.join("changes.patch").display(),
            task_dir.join("report.md").display()
        );
        return;
    }

    let term = console::Term::stdout();
    let _ = term.write_line("");

    match result.verdict {
        Verdict::Approved => {
            let _ = term.write_line(&format!(
                " {} {}",
                theme.success.apply_to("✨"),
                theme.heading.apply_to("Task Completed Successfully")
            ));
        }
        Verdict::Rejected => {
            let _ = term.write_line(&format!(
                " {} {}",
                theme.error.apply_to("✗"),
                theme.heading.apply_to("Task Rejected by Reviewer")
            ));
        }
        _ => {
            let _ = term.write_line(&format!(
                " {} {}",
                theme.warning.apply_to("!"),
                theme.heading.apply_to("Task Completed (Unknown Status)")
            ));
        }
    }

    let _ = term.write_line(&format!(
        "   {} {}",
        theme.subtext.apply_to("Task ID:"),
        result.task_id
    ));
    let _ = term.write_line(&format!(
        "   {} {}",
        theme.subtext.apply_to("Revisions:"),
        result.revision_rounds
    ));
    let _ = term.write_line(&format!(
        "   {} {}",
        theme.subtext.apply_to("Branch:"),
        branch
    ));
    let _ = term.write_line(&format!(
        "   {} {}",
        theme.subtext.apply_to("Patch:"),
        task_dir.join("changes.patch").display()
    ));
    let _ = term.write_line(&format!(
        "   {} {}",
        theme.subtext.apply_to("Report:"),
        task_dir.join("report.md").display()
    ));
    let _ = term.write_line("");
    let _ = term.write_line(&format!(
        "   {} git checkout {}",
        theme.subtext.apply_to("Next:"),
        branch
    ));
    let _ = term.write_line("");
}

pub fn render_failure(error: &NikiError, _state: &PipelineState, theme: &Theme, is_tty: bool) {
    if !is_tty {
        println!("Task failed: {}", error);
        return;
    }
    let term = console::Term::stdout();
    let _ = term.write_line("");
    let _ = term.write_line(&format!(
        " {} {}",
        theme.error.apply_to("✗"),
        theme.heading.apply_to("Task Failed")
    ));
    let _ = term.write_line(&format!("   {}", error));
    let _ = term.write_line("");
}
