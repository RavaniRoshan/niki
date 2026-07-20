use crate::display::agent_stream::{StageState, StageStatus};
use crate::display::theme::Theme;
use crate::artifacts::types::AgentRole;

pub fn update_pipeline_status(stages: &[StageState], theme: &Theme) {
    let term = console::Term::stdout();
    let mut parts = vec![];
    
    let all_roles = [
        AgentRole::Planner,
        AgentRole::Coder,
        AgentRole::Tester,
        AgentRole::Reviewer,
        AgentRole::Synthesizer,
        AgentRole::SecurityAuditor,
        AgentRole::Red,
    ];

    for (i, role) in all_roles.iter().enumerate() {
        let (icon, color) = match role {
            AgentRole::Planner => (theme.planner.icon, theme.planner.color.clone()),
            AgentRole::Coder => (theme.coder.icon, theme.coder.color.clone()),
            AgentRole::Tester => (theme.tester.icon, theme.tester.color.clone()),
            AgentRole::Reviewer => (theme.reviewer.icon, theme.reviewer.color.clone()),
            AgentRole::Synthesizer => (theme.synthesizer.icon, theme.synthesizer.color.clone()),
            AgentRole::SecurityAuditor => (theme.security_auditor.icon, theme.security_auditor.color.clone()),
            AgentRole::Red => (theme.red.icon, theme.red.color.clone()),
        };
        
        let mut status = None;
        for stage in stages.iter().rev() {
            if stage.role == *role {
                status = Some(&stage.status);
                break;
            }
        }
        
        let marker = match status {
            Some(StageStatus::Done) => theme.success.apply_to(icon).to_string(),
            Some(StageStatus::Running) => color.apply_to(icon).to_string(),
            Some(StageStatus::Failed) => theme.error.apply_to(icon).to_string(),
            Some(StageStatus::Revision) => theme.warning.apply_to(icon).to_string(),
            _ => theme.border.apply_to("○").to_string(),
        };
        
        parts.push(marker);
        if i < 3 {
            parts.push(theme.border.apply_to("──").to_string());
        }
    }
    
    let pipeline_str = format!(" {}   {}", theme.border.apply_to("│"), parts.join(" "));
    let _ = term.write_line(&pipeline_str);
}
