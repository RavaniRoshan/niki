use anyhow::Result;
use minijinja::{Environment, context};
use crate::orchestrator::pipeline::{Task, PipelineResult};
use crate::config::NikiConfig;
use std::fs;

/// Build the "## Cost & Performance" markdown section from per-agent metrics.
fn render_cost_section(result: &PipelineResult) -> String {
    if result.metrics.is_empty() {
        return String::new();
    }

    let mut out = String::from(
        "## Cost & Performance\n\n\
         | Agent | Provider | Model | In tok | Out tok | Latency | Cost |\n\
         |-------|----------|-------|-------:|--------:|--------:|-----:|\n",
    );

    let mut total_in: u32 = 0;
    let mut total_out: u32 = 0;
    let mut total_ms: u64 = 0;
    let mut total_cost: f64 = 0.0;

    for m in &result.metrics {
        total_in += m.input_tokens;
        total_out += m.output_tokens;
        total_ms += m.latency_ms;
        total_cost += m.cost_usd;
        let cost = if m.cost_usd > 0.0 {
            format!("${:.4}", m.cost_usd)
        } else {
            "n/a".to_string()
        };
        out.push_str(&format!(
            "| {:?} | {} | {} | {} | {} | {:.1}s | {} |\n",
            m.role,
            m.provider,
            m.model,
            m.input_tokens,
            m.output_tokens,
            m.latency_ms as f64 / 1000.0,
            cost,
        ));
    }

    let total_cost_str = if total_cost > 0.0 {
        format!("${:.4}", total_cost)
    } else {
        "n/a".to_string()
    };
    out.push_str(&format!(
        "| **Total** | | | **{}** | **{}** | **{:.1}s** | **{}** |\n",
        total_in, total_out, total_ms as f64 / 1000.0, total_cost_str,
    ));
    out.push('\n');
    out
}

pub fn generate_report(
    task: &Task,
    config: &NikiConfig,
    result: &PipelineResult,
) -> Result<()> {
    let mut env = Environment::new();

    let template = r#"
# NIKI Execution Report

**Task ID**: {{ task_id }}
**Description**: {{ description }}
**Project Path**: {{ project_path }}

## Pipeline Result
- Verdict: {{ verdict }}
- Revision Rounds: {{ revision_rounds }}

{{ cost_section }}
## Final Diff
```diff
{{ final_diff }}
```
"#;

    env.add_template("report.md", template)?;
    let tmpl = env.get_template("report.md")?;

    let rendered = tmpl.render(context! {
        task_id => task.id.to_string(),
        description => task.description.clone(),
        project_path => task.project_path.to_string_lossy().to_string(),
        verdict => format!("{:?}", result.verdict),
        revision_rounds => result.revision_rounds,
        cost_section => render_cost_section(result),
        final_diff => result.final_diff.clone(),
    })?;

    let output_dir = task
        .project_path
        .join(&config.general.output_dir)
        .join("tasks")
        .join(task.id.to_string());
    fs::create_dir_all(&output_dir)?;

    let report_path = output_dir.join("report.md");
    fs::write(&report_path, rendered)?;

    let diff_path = output_dir.join("changes.patch");
    fs::write(&diff_path, &result.final_diff)?;

    Ok(())
}
