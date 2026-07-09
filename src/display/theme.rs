use console::Style;

pub struct Theme {
    pub planner: AgentTheme,
    pub coder: AgentTheme,
    pub tester: AgentTheme,
    pub reviewer: AgentTheme,
    pub border: Style,
    pub heading: Style,
    pub subtext: Style,
    pub success: Style,
    pub warning: Style,
    pub error: Style,
    pub diff_add: Style,
    pub diff_remove: Style,
    pub file_path: Style,
}

pub struct AgentTheme {
    pub name: &'static str,
    pub icon: &'static str,
    pub color: Style,
    pub label_style: Style,
}

impl Default for Theme {
    fn default() -> Self {
        Self::new()
    }
}

impl Theme {
    pub fn new() -> Self {
        Theme {
            planner: AgentTheme {
                name: "Planner",
                icon: "◈",
                color: Style::new().blue(),
                label_style: Style::new().bold().blue(),
            },
            coder: AgentTheme {
                name: "Coder",
                icon: "⟠",
                color: Style::new().magenta(),
                label_style: Style::new().bold().magenta(),
            },
            tester: AgentTheme {
                name: "Tester",
                icon: "◉",
                color: Style::new().green(),
                label_style: Style::new().bold().green(),
            },
            reviewer: AgentTheme {
                name: "Reviewer",
                icon: "◆",
                color: Style::new().yellow(),
                label_style: Style::new().bold().yellow(),
            },
            border: Style::new().dim(),
            heading: Style::new().bold().white(),
            subtext: Style::new().dim(),
            success: Style::new().green(),
            warning: Style::new().yellow(),
            error: Style::new().red(),
            diff_add: Style::new().on_green().black(),
            diff_remove: Style::new().on_red().black(),
            file_path: Style::new().cyan().underlined(),
        }
    }
}
