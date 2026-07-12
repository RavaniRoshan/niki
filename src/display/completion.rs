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

        let (total_in, total_out, total_cost, total_ms) = cost_totals(result);
        if total_cost > 0.0 {
            println!(
                "[{}] [NIKI] Cost: ${:.4} | Tokens in/out: {}/{} | Latency: {:.1}s",
                ts, total_cost, total_in, total_out, total_ms as f64 / 1000.0
            );
        } else {
            println!(
                "[{}] [NIKI] Tokens in/out: {}/{} | Latency: {:.1}s",
                ts, total_in, total_out, total_ms as f64 / 1000.0
            );
        }

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

    let (total_in, total_out, total_cost, total_ms) = cost_totals(result);
    if total_cost > 0.0 {
        let _ = term.write_line(&format!(
            "   {} ${:.4} across {} agent(s) · {} in / {} out · {:.1}s",
            theme.subtext.apply_to("Cost:"),
            total_cost,
            result.metrics.len(),
            total_in,
            total_out,
            total_ms as f64 / 1000.0
        ));
    } else if !result.metrics.is_empty() {
        let _ = term.write_line(&format!(
            "   {} {} in / {} out · {:.1}s (cost n/a for model)",
            theme.subtext.apply_to("Tokens:"),
            total_in,
            total_out,
            total_ms as f64 / 1000.0
        ));
    }

    let _ = term.write_line("");
    let _ = term.write_line(&format!(
        "   {} git checkout {}",
        theme.subtext.apply_to("Next:"),
        branch
    ));
    let _ = term.write_line("");
}

/// Sum token/cost/latency across all recorded stages.
fn cost_totals(result: &PipelineResult) -> (u32, u32, f64, u64) {
    let mut total_in = 0u32;
    let mut total_out = 0u32;
    let mut total_cost = 0.0f64;
    let mut total_ms = 0u64;
    for m in &result.metrics {
        total_in += m.input_tokens;
        total_out += m.output_tokens;
        total_cost += m.cost_usd;
        total_ms += m.latency_ms;
    }
    (total_in, total_out, total_cost, total_ms)
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
