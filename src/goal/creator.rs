use anyhow::Result;

use crate::goal::state::{GoalCriterion, GoalState, GoalStatus, GoalTask, TaskStatus};

pub struct GoalCreator;

struct SurveyResult {
    languages: Vec<String>,
    file_count: usize,
    src_files: Vec<String>,
    test_files: Vec<String>,
    has_cargo: bool,
    has_package_json: bool,
    has_ci: bool,
    todo_count: usize,
}

impl GoalCreator {
    pub fn create(objective: &str, scope: Option<&str>, max_iterations: u32) -> Result<GoalState> {
        let slug = slugify(objective);
        let id = generate_id();
        let branch_name = format!("goal/{}-{}", slug, id);

        let scope_lock = match scope {
            Some(s) => vec![s.to_string()],
            None => vec![".".to_string()],
        };

        let survey = survey_codebase(scope.unwrap_or("."));
        let criteria = derive_criteria(objective, &survey);
        let tasks = decompose_tasks(objective, &survey);
        let context_summary = build_context_summary(objective, &survey, &criteria, &tasks);

        let state = GoalState {
            id: id.clone(),
            slug: format!("{}-{}", slug, id),
            objective: objective.to_string(),
            status: GoalStatus::Active,
            branch: branch_name,
            scope: scope.unwrap_or(".").to_string(),
            scope_lock,
            scope_flex: vec![],
            criteria,
            tasks,
            current_task: 0,
            iterations: 0,
            budget_used: 0,
            max_iterations,
            negative_knowledge: vec![],
            context_summary,
            created_at: chrono::Utc::now().to_rfc3339(),
            completed_at: None,
        };

        Ok(state)
    }
}

fn run_cmd(cmd: &str, args: &[&str]) -> String {
    std::process::Command::new(cmd)
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

fn survey_codebase(scope: &str) -> SurveyResult {
    let src_files = run_cmd("find", &[scope, "-type", "f", "-name", "*.rs"]);
    let test_files = run_cmd("find", &[scope, "-type", "f", "-name", "*test*"]);
    let all_files = run_cmd("find", &[scope, "-type", "f"]);
    let file_count = all_files.lines().filter(|l| !l.is_empty()).count();

    let mut languages = Vec::new();
    if !src_files.trim().is_empty() {
        languages.push("rust".to_string());
    }
    if !run_cmd("find", &[scope, "-type", "f", "-name", "*.ts"])
        .trim()
        .is_empty()
    {
        languages.push("typescript".to_string());
    }
    if !run_cmd("find", &[scope, "-type", "f", "-name", "*.py"])
        .trim()
        .is_empty()
    {
        languages.push("python".to_string());
    }

    let has_cargo = std::path::Path::new("Cargo.toml").exists();
    let has_package_json = std::path::Path::new("package.json").exists();
    let has_ci = std::path::Path::new(".github/workflows").exists()
        || std::path::Path::new(".gitlab-ci.yml").exists();

    let todo_output = run_cmd(
        "grep",
        &["-r", "--include=*.rs", "-c", "TODO\\|FIXME\\|HACK", scope],
    );
    let todo_count: usize = todo_output
        .lines()
        .filter_map(|l| l.split(':').last()?.parse::<usize>().ok())
        .sum();

    SurveyResult {
        languages,
        file_count,
        src_files: src_files
            .lines()
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect(),
        test_files: test_files
            .lines()
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect(),
        has_cargo,
        has_package_json,
        has_ci,
        todo_count,
    }
}

fn derive_criteria(objective: &str, survey: &SurveyResult) -> Vec<GoalCriterion> {
    let mut criteria = Vec::new();
    let obj_lower = objective.to_lowercase();

    if survey.todo_count > 0 {
        criteria.push(GoalCriterion {
            label: "No TODO/FIXME markers in scope".to_string(),
            check: format!(
                "! grep -r --include='*.rs' -c 'TODO\\|FIXME\\|HACK' {} 2>/dev/null | grep -v ':0$' | grep .",
                survey
                    .src_files
                    .first()
                    .and_then(|f| std::path::Path::new(f).parent())
                    .and_then(|p| p.to_str())
                    .unwrap_or(".")
            ),
            must_pass: true,
            coverage_gate: false,
            result: None,
        });
    } else {
        criteria.push(GoalCriterion {
            label: "No new TODO/FIXME introduced".to_string(),
            check: "! grep -r --include='*.rs' 'TODO\\|FIXME\\|HACK' . 2>/dev/null | grep ."
                .to_string(),
            must_pass: true,
            coverage_gate: false,
            result: None,
        });
    }

    if survey.has_cargo {
        criteria.push(GoalCriterion {
            label: "cargo check passes".to_string(),
            check: "cargo check 2>&1 | tail -1 | ! grep -i error".to_string(),
            must_pass: true,
            coverage_gate: false,
            result: None,
        });

        criteria.push(GoalCriterion {
            label: "Tests pass".to_string(),
            check: "cargo test --lib 2>&1 | tail -5 | ! grep -E 'FAILED|test result: 0'"
                .to_string(),
            must_pass: true,
            coverage_gate: false,
            result: None,
        });
    }

    if survey.has_package_json {
        criteria.push(GoalCriterion {
            label: "npm build succeeds".to_string(),
            check: "npm run build 2>&1 | tail -1 | ! grep -i error".to_string(),
            must_pass: true,
            coverage_gate: false,
            result: None,
        });
    }

    if obj_lower.contains("all")
        || obj_lower.contains("every")
        || obj_lower.contains("entire")
        || obj_lower.contains("comprehensive")
    {
        let src_count = survey.src_files.len().max(1);
        criteria.push(GoalCriterion {
            label: "All source files touched or verified".to_string(),
            check: format!(
                "test $(find {} -name '*.rs' -newer .niki-goal-marker 2>/dev/null | wc -l) -ge {}",
                ".", src_count
            ),
            must_pass: false,
            coverage_gate: true,
            result: None,
        });
    }

    if criteria.len() < 2 {
        criteria.push(GoalCriterion {
            label: "Objective interpretation check".to_string(),
            check: format!(
                "echo '{}' | wc -w | awk '{{if ($1 >= 2) exit 0; else exit 1}}'",
                objective
            ),
            must_pass: true,
            coverage_gate: false,
            result: None,
        });
    }

    criteria.truncate(5);
    criteria
}

fn decompose_tasks(objective: &str, survey: &SurveyResult) -> Vec<GoalTask> {
    let mut tasks = Vec::new();
    let obj_lower = objective.to_lowercase();
    let mut id = 1;

    let langs = if survey.languages.is_empty() {
        "unknown".to_string()
    } else {
        survey.languages.join(", ")
    };
    tasks.push(GoalTask {
        id,
        desc: format!(
            "Analyze codebase under scope ({} files found, languages: {})",
            survey.file_count, langs
        ),
        status: TaskStatus::Todo,
    });
    id += 1;

    tasks.push(GoalTask {
        id,
        desc: "Identify affected modules and entry points".to_string(),
        status: TaskStatus::Todo,
    });
    id += 1;

    tasks.push(GoalTask {
        id,
        desc: format!("Implement: {}", objective),
        status: TaskStatus::Todo,
    });
    id += 1;

    if survey.has_cargo {
        tasks.push(GoalTask {
            id,
            desc: "Verify structural correctness (cargo check)".to_string(),
            status: TaskStatus::Todo,
        });
        id += 1;
    }

    if obj_lower.contains("test")
        || obj_lower.contains("fix")
        || obj_lower.contains("bug")
        || obj_lower.contains("add")
    {
        tasks.push(GoalTask {
            id,
            desc: "Add or update tests for the change".to_string(),
            status: TaskStatus::Todo,
        });
        id += 1;
    }

    tasks.push(GoalTask {
        id,
        desc: "Run full test suite and verify pass".to_string(),
        status: TaskStatus::Todo,
    });
    id += 1;

    if !survey.test_files.is_empty() {
        tasks.push(GoalTask {
            id,
            desc: "Check for regressions in existing tests".to_string(),
            status: TaskStatus::Todo,
        });
        id += 1;
    }

    if obj_lower.contains("clean")
        || obj_lower.contains("refactor")
        || obj_lower.contains("remove")
        || obj_lower.contains("delete")
    {
        tasks.push(GoalTask {
            id,
            desc: "Clean up dead code and unused imports".to_string(),
            status: TaskStatus::Todo,
        });
        id += 1;
    }

    if survey.file_count > 10 {
        tasks.push(GoalTask {
            id,
            desc: "Commit changes with descriptive message".to_string(),
            status: TaskStatus::Todo,
        });
        id += 1;
    }

    while tasks.len() < 3 {
        tasks.push(GoalTask {
            id,
            desc: format!("Verification step {}", id),
            status: TaskStatus::Todo,
        });
        id += 1;
    }

    tasks.truncate(15);
    tasks
}

fn build_context_summary(
    objective: &str,
    survey: &SurveyResult,
    criteria: &[GoalCriterion],
    tasks: &[GoalTask],
) -> String {
    let mut lines = vec![format!("Goal: {}", objective)];

    let langs = if survey.languages.is_empty() {
        "unknown".to_string()
    } else {
        survey.languages.join(", ")
    };
    lines.push(format!(
        "Codebase survey: {} files, languages: {}, cargo: {}, package.json: {}, ci: {}",
        survey.file_count, langs, survey.has_cargo, survey.has_package_json, survey.has_ci
    ));

    lines.push(format!(
        "Derived {} criteria, {} tasks",
        criteria.len(),
        tasks.len()
    ));

    lines.push("Iteration 0: created.".to_string());

    lines.join("\n")
}

fn slugify(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-")
}

fn generate_id() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    chrono::Utc::now().timestamp().hash(&mut hasher);
    format!("{:x}", hasher.finish())[..6].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify() {
        let slug = slugify("Fix N+1 queries in the dashboard");
        assert_eq!(slug, "fix-n-1-queries-in-the");
    }

    #[test]
    fn test_slugify_short() {
        let slug = slugify("Build login page");
        assert_eq!(slug, "build-login-page");
    }

    #[test]
    fn test_generate_id_length() {
        let id = generate_id();
        assert_eq!(id.len(), 6);
    }

    #[test]
    fn test_create_basic() {
        let state = GoalCreator::create("Fix login bug", None, 30).unwrap();
        assert_eq!(state.objective, "Fix login bug");
        assert_eq!(state.status, GoalStatus::Active);
        assert_eq!(state.max_iterations, 30);
        assert_eq!(state.iterations, 0);
        assert_eq!(state.current_task, 0);
        assert!(state.branch.starts_with("goal/fix-login-bug-"));
        assert!(!state.criteria.is_empty());
        assert!(!state.tasks.is_empty());
        assert!(state.tasks.len() >= 3);
        assert!(state.criteria.len() >= 2);
        assert!(state.criteria.len() <= 5);
        assert!(state.tasks.len() <= 15);
    }

    #[test]
    fn test_create_with_scope() {
        let state = GoalCreator::create("Add feature", Some("src/goal"), 50).unwrap();
        assert_eq!(state.scope, "src/goal");
        assert_eq!(state.scope_lock, vec!["src/goal"]);
        assert_eq!(state.max_iterations, 50);
    }

    #[test]
    fn test_create_no_scope() {
        let state = GoalCreator::create("Refactor code", None, 30).unwrap();
        assert_eq!(state.scope, ".");
        assert_eq!(state.scope_lock, vec!["."]);
    }

    #[test]
    fn test_criteria_have_structural_check() {
        let state = GoalCreator::create("Fix bug", None, 30).unwrap();
        let has_structural = state
            .criteria
            .iter()
            .any(|c| c.label.contains("TODO") || c.label.contains("structural"));
        assert!(has_structural, "must have at least one structural check");
    }

    #[test]
    fn test_criteria_have_user_facing_check() {
        let state = GoalCreator::create("Fix bug", None, 30).unwrap();
        let has_user_facing = state
            .criteria
            .iter()
            .any(|c| c.label.contains("cargo check") || c.label.contains("Tests"));
        assert!(has_user_facing, "must have at least one user-facing check");
    }

    #[test]
    fn test_coverage_gate_for_all_objective() {
        let state = GoalCreator::create("Fix all security issues", None, 30).unwrap();
        let has_coverage = state.criteria.iter().any(|c| c.coverage_gate);
        assert!(has_coverage, "must have coverage gate for 'all' objective");
    }

    #[test]
    fn test_no_coverage_gate_for_narrow_objective() {
        let state = GoalCreator::create("Fix login bug", None, 30).unwrap();
        let has_coverage = state.criteria.iter().any(|c| c.coverage_gate);
        assert!(
            !has_coverage,
            "should not have coverage gate for narrow objective"
        );
    }

    #[test]
    fn test_tasks_are_numbered_sequentially() {
        let state = GoalCreator::create("Build feature", None, 30).unwrap();
        for (i, task) in state.tasks.iter().enumerate() {
            assert_eq!(task.id, (i + 1) as u32);
            assert_eq!(task.status, TaskStatus::Todo);
        }
    }

    #[test]
    fn test_context_summary_contains_objective() {
        let state = GoalCreator::create("Deploy to production", None, 30).unwrap();
        assert!(state.context_summary.contains("Deploy to production"));
    }

    #[test]
    fn test_context_summary_contains_survey() {
        let state = GoalCreator::create("Test objective", None, 30).unwrap();
        assert!(state.context_summary.contains("Codebase survey"));
    }

    #[test]
    fn test_all_criteria_have_check_command() {
        let state = GoalCreator::create("Fix all bugs", None, 30).unwrap();
        for c in &state.criteria {
            assert!(!c.check.is_empty(), "check command must not be empty");
            assert!(!c.label.is_empty(), "label must not be empty");
        }
    }

    #[test]
    fn test_max_iterations_respected() {
        let state = GoalCreator::create("Do something", None, 5).unwrap();
        assert_eq!(state.max_iterations, 5);
    }

    #[test]
    fn test_timestamps_present() {
        let state = GoalCreator::create("Test", None, 30).unwrap();
        assert!(!state.created_at.is_empty());
        assert!(state.completed_at.is_none());
    }

    #[test]
    fn test_every_objective_gets_coverage_gate() {
        let state = GoalCreator::create("Update every module", None, 30).unwrap();
        assert!(
            state.criteria.iter().any(|c| c.coverage_gate),
            "'every' should trigger coverage gate"
        );
    }

    #[test]
    fn test_comprehensive_gets_coverage_gate() {
        let state = GoalCreator::create("Comprehensive security audit", None, 30).unwrap();
        assert!(
            state.criteria.iter().any(|c| c.coverage_gate),
            "'comprehensive' should trigger coverage gate"
        );
    }
}
