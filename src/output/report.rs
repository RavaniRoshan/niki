use anyhow::Result;
use minijinja::{Environment, context};
use crate::orchestrator::pipeline::{Task, PipelineResult};
use crate::config::NikiConfig;
use std::fs;

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
