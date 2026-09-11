use niki::artifacts::types::{AgentRole, Complexity, FileAction, FileChange, TaskSpec};
use niki::config::NikiConfig;
use niki::orchestrator::pipeline::{apply_risk_stages, resolve_stages};
use niki::risk::{RiskLevel, classify};

fn spec(summary: &str, files: &[&str]) -> TaskSpec {
    TaskSpec {
        summary: summary.to_string(),
        approach: "test approach".to_string(),
        files_to_modify: files
            .iter()
            .map(|p| FileChange {
                path: p.to_string(),
                action: FileAction::Modify,
                description: "x".to_string(),
            })
            .collect(),
        acceptance_criteria: vec![],
        constraints: vec![],
        estimated_complexity: Complexity::Low,
        uncertainties: None,
    }
}

fn roles_of(stages: &[niki::config::types::PipelineStageConfig]) -> Vec<AgentRole> {
    stages.iter().map(|s| s.role).collect()
}

#[test]
fn default_stages_have_no_critic() {
    let stages = resolve_stages(&NikiConfig::default());
    assert!(!roles_of(&stages).contains(&AgentRole::Critic));
}

#[test]
fn low_risk_leaves_topology_untouched() {
    let config = NikiConfig::default();
    let risk = classify(&spec("Fix typo", &["README.md"]), &config);
    assert_eq!(risk.level, RiskLevel::Low);
    let stages = resolve_stages(&config);
    let before = roles_of(&stages);
    let after = roles_of(&apply_risk_stages(stages, &risk, &config));
    assert_eq!(
        before, after,
        "Low risk must not change today's default path"
    );
}

#[test]
fn normal_risk_inserts_critic_after_reviewer() {
    let config = NikiConfig::default();
    let files: Vec<String> = (0..12).map(|i| format!("src/f{i}.rs")).collect();
    let refs: Vec<&str> = files.iter().map(|s| s.as_str()).collect();
    let risk = classify(&spec("Broad refactor", &refs), &config);
    assert_eq!(risk.level, RiskLevel::Normal);
    let stages = apply_risk_stages(resolve_stages(&config), &risk, &config);
    let roles = roles_of(&stages);
    let reviewer = roles
        .iter()
        .position(|r| *r == AgentRole::Reviewer)
        .unwrap();
    let critic = roles.iter().position(|r| *r == AgentRole::Critic).unwrap();
    assert_eq!(critic, reviewer + 1);
    // The Critic is deliberately cheap.
    let critic_stage = stages.iter().find(|s| s.role == AgentRole::Critic).unwrap();
    assert_eq!(critic_stage.max_tokens, 4096);
}

#[test]
fn high_risk_forces_security_audit_plus_critic() {
    let config = NikiConfig::default();
    assert!(!config.security.enabled);
    let risk = classify(&spec("Rotate auth tokens", &["src/auth/token.rs"]), &config);
    assert_eq!(risk.level, RiskLevel::High);
    let stages = apply_risk_stages(resolve_stages(&config), &risk, &config);
    let roles = roles_of(&stages);
    assert!(roles.contains(&AgentRole::SecurityAuditor));
    assert!(roles.contains(&AgentRole::Critic));
}

#[test]
fn critic_gating_when_disabled() {
    let mut config = NikiConfig::default();
    config.critic.enabled = false;
    let files: Vec<String> = (0..12).map(|i| format!("src/f{i}.rs")).collect();
    let refs: Vec<&str> = files.iter().map(|s| s.as_str()).collect();
    let risk = classify(&spec("Broad refactor", &refs), &config);
    assert_eq!(risk.level, RiskLevel::Normal);
    let stages = apply_risk_stages(resolve_stages(&config), &risk, &config);
    assert!(!roles_of(&stages).contains(&AgentRole::Critic));
}

#[test]
fn explicit_custom_topology_is_never_rewritten() {
    let mut config = NikiConfig::default();
    config.pipeline.stages = vec![niki::config::types::PipelineStageConfig {
        role: AgentRole::Coder,
        provider: "anthropic".to_string(),
        model: "m".to_string(),
        skip: false,
        max_tokens: 0,
        temperature: 0.0,
        fallbacks: vec![],
    }];
    config.risk.mode = niki::config::types::RiskMode::Security;
    let risk = classify(&spec("Anything", &["src/auth/x.rs"]), &config);
    assert_eq!(risk.level, RiskLevel::Security);
    let stages = apply_risk_stages(resolve_stages(&config), &risk, &config);
    assert_eq!(roles_of(&stages), vec![AgentRole::Coder]);
}

#[test]
fn existing_critic_is_not_duplicated() {
    let config = NikiConfig::default();
    let mut stages = resolve_stages(&config);
    stages.push(niki::config::types::PipelineStageConfig {
        role: AgentRole::Critic,
        provider: "anthropic".to_string(),
        model: "m".to_string(),
        skip: false,
        max_tokens: 0,
        temperature: 0.0,
        fallbacks: vec![],
    });
    let files: Vec<String> = (0..12).map(|i| format!("src/f{i}.rs")).collect();
    let refs: Vec<&str> = files.iter().map(|s| s.as_str()).collect();
    let risk = classify(&spec("Broad refactor", &refs), &config);
    let stages = apply_risk_stages(stages, &risk, &config);
    assert_eq!(
        roles_of(&stages)
            .iter()
            .filter(|r| **r == AgentRole::Critic)
            .count(),
        1
    );
}
