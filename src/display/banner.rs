use crate::config::NikiConfig;
use crate::orchestrator::pipeline::Task;
use console::Style;

pub fn show_banner(task: &Task, config: &NikiConfig, is_tty: bool) {
    if !is_tty {
        // Plain log-mode header for piped/CI output: no box, no color.
        let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let short_id = &task.id.to_string()[..8];
        let proj = task
            .project_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        println!(
            "[{}] [NIKI] Task {}: \"{}\"",
            ts, short_id, task.description
        );
        println!(
            "[{}] [NIKI] Project: {} | Models: {} / {}",
            ts,
            proj,
            config.agents.planner.model,
            config.agents.coder.model
        );
        return;
    }

    let term = console::Term::stdout();
    let _width = term.size().1 as usize;

    let border = Style::new().dim();
    let theme = crate::display::theme::Theme::new();

    let top = format!(" ╭{}╮", "─".repeat(49));
    let bottom = format!(" ╰{}╯", "─".repeat(49));
    let empty = format!(" │{}│", " ".repeat(49));

    let icons = format!(
        " {} {} {} {} ",
        theme.planner.color.apply_to(theme.planner.icon),
        theme.coder.color.apply_to(theme.coder.icon),
        theme.tester.color.apply_to(theme.tester.icon),
        theme.reviewer.color.apply_to(theme.reviewer.icon)
    );

    println!("{}", border.apply_to(&top));
    println!("{}", border.apply_to(&empty));
    println!(
        " {}   {}  {}                                │",
        border.apply_to("│"),
        icons,
        Style::new().bold().white().apply_to("NIKI")
    );
    println!("{}", border.apply_to(&empty));

    let desc = crate::display::artifact_render::truncate(&task.description, 40);
    println!(
        " {}   \"{}\"{:w$}│",
        border.apply_to("│"),
        desc,
        "",
        w = 49 - 5 - desc.len()
    );

    println!("{}", border.apply_to(&empty));

    let proj = task
        .project_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let proj_str = format!("Project   {}/", proj);
    println!(
        " {}   {}{:w$}│",
        border.apply_to("│"),
        proj_str,
        "",
        w = 49 - 3 - proj_str.chars().count()
    );

    let pipeline_str = "Pipeline  Planner → Coder → Tester → Reviewer";
    println!(
        " {}   {}{:w$}│",
        border.apply_to("│"),
        pipeline_str,
        "",
        w = 49 - 3 - pipeline_str.chars().count()
    );

    let models = format!("Models    {} / {}", config.agents.planner.model, config.agents.coder.model);
    let models_trunc = crate::display::artifact_render::truncate(&models, 40);
    println!(
        " {}   {}{:w$}│",
        border.apply_to("│"),
        models_trunc,
        "",
        w = 49 - 3 - models_trunc.chars().count()
    );

    let task_id_str = format!("Task ID   {}", &task.id.to_string()[..8]);
    println!(
        " {}   {}{:w$}│",
        border.apply_to("│"),
        task_id_str,
        "",
        w = 49 - 3 - task_id_str.chars().count()
    );

    println!("{}", border.apply_to(&empty));
    println!("{}", border.apply_to(&bottom));
    println!();
}
