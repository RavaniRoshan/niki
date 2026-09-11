//! Deterministic risk classification: maps a TaskSpec to a risk tier that
//! gates pipeline topology. No LLM call — cheap keyword/path heuristics over
//! the spec, so it runs before any stage executes.

use crate::artifacts::types::TaskSpec;
use crate::config::NikiConfig;
use serde::{Deserialize, Serialize};

/// Risk tier for a run.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    #[default]
    Low,
    Normal,
    High,
    Security,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::Low => "low",
            RiskLevel::Normal => "normal",
            RiskLevel::High => "high",
            RiskLevel::Security => "security",
        }
    }

    fn bump(self) -> Self {
        match self {
            RiskLevel::Low => RiskLevel::Normal,
            RiskLevel::Normal => RiskLevel::High,
            RiskLevel::High => RiskLevel::Security,
            RiskLevel::Security => RiskLevel::Security,
        }
    }
}

/// Classification outcome with a human-readable rationale.
#[derive(Debug, Clone)]
pub struct TaskRisk {
    pub level: RiskLevel,
    pub rationale: String,
    /// True when forced by `[risk].mode` rather than derived from the spec.
    pub config_driven: bool,
}

/// Classify a TaskSpec into a risk tier.
///
/// Ladder (deterministic, documented):
/// - Start at `Low`.
/// - More files than `file_count_threshold` → bump one tier (broad blast radius).
/// - Any `severity_keywords` hit in spec text → bump one tier.
/// - Any `denylist_patterns` hit in touched paths → at least `High`;
///   combined with another signal → `Security`.
/// - `mode != auto` forces the tier outright.
pub fn classify(spec: &TaskSpec, config: &NikiConfig) -> TaskRisk {
    use crate::config::types::RiskMode;
    match config.risk.mode {
        RiskMode::Low => {
            return forced(RiskLevel::Low, &config.risk.mode);
        }
        RiskMode::Normal => {
            return forced(RiskLevel::Normal, &config.risk.mode);
        }
        RiskMode::High => {
            return forced(RiskLevel::High, &config.risk.mode);
        }
        RiskMode::Security => {
            return forced(RiskLevel::Security, &config.risk.mode);
        }
        RiskMode::Auto => {}
    }

    let text = format!(
        "{} {}\n{}",
        spec.summary,
        spec.approach,
        spec.constraints.join("\n")
    )
    .to_lowercase();
    let paths: Vec<String> = spec
        .files_to_modify
        .iter()
        .map(|f| f.path.to_lowercase())
        .collect();

    let mut level = RiskLevel::Low;
    let mut reasons: Vec<String> = Vec::new();

    if spec.files_to_modify.len() > config.risk.file_count_threshold {
        level = level.bump();
        reasons.push(format!(
            "{} files touched (threshold {})",
            spec.files_to_modify.len(),
            config.risk.file_count_threshold
        ));
    }

    let severity_hit = config
        .risk
        .severity_keywords
        .iter()
        .find(|k| text.contains(k.to_lowercase().as_str()))
        .cloned();
    if let Some(keyword) = severity_hit {
        level = level.bump();
        reasons.push(format!("severity keyword `{keyword}` in spec"));
    }

    let denylist_hit = config.risk.denylist_patterns.iter().find(|p| {
        let needle = p.to_lowercase();
        paths.iter().any(|path| path.contains(&needle)) || text.contains(&needle)
    });
    if let Some(pattern) = denylist_hit {
        if level == RiskLevel::Low && reasons.is_empty() {
            level = RiskLevel::High;
        } else {
            level = RiskLevel::Security;
        }
        reasons.push(format!("sensitive pattern `{pattern}`"));
    }

    if reasons.is_empty() {
        reasons.push("no risk signals in spec".to_string());
    }
    TaskRisk {
        level,
        rationale: format!("auto: {}", reasons.join("; ")),
        config_driven: false,
    }
}

fn forced(level: RiskLevel, mode: &crate::config::types::RiskMode) -> TaskRisk {
    TaskRisk {
        level,
        rationale: format!("explicit [risk].mode = {mode:?}"),
        config_driven: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifacts::types::{Complexity, FileAction, FileChange};

    fn spec(summary: &str, approach: &str, files: &[&str]) -> TaskSpec {
        TaskSpec {
            summary: summary.to_string(),
            approach: approach.to_string(),
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

    #[test]
    fn severity_keyword_bumps() {
        let risk = classify(
            &spec(
                "Fix parser panic",
                "guard against panic on empty input",
                &["src/parse.rs"],
            ),
            &NikiConfig::default(),
        );
        assert_eq!(risk.level, RiskLevel::Normal);
    }

    #[test]
    fn many_files_bump_to_normal() {
        let files = (0..12).map(|i| format!("src/f{i}.rs"));
        let owned: Vec<String> = files.collect();
        let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
        let risk = classify(
            &spec("Refactor", "touch many", &refs),
            &NikiConfig::default(),
        );
        assert_eq!(risk.level, RiskLevel::Normal);
    }

    #[test]
    fn sensitive_path_is_high() {
        let risk = classify(
            &spec("Update login", "change flow", &["src/auth/middleware.rs"]),
            &NikiConfig::default(),
        );
        assert_eq!(risk.level, RiskLevel::High);
    }

    #[test]
    fn sensitive_plus_broad_is_security() {
        let files = (0..12).map(|i| format!("src/f{i}.rs"));
        let mut owned: Vec<String> = files.collect();
        owned.push("src/auth/token.rs".to_string());
        let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
        let risk = classify(
            &spec("Rotate tokens", "broad change", &refs),
            &NikiConfig::default(),
        );
        assert_eq!(risk.level, RiskLevel::Security);
    }

    #[test]
    fn explicit_mode_forces_tier() {
        let mut config = NikiConfig::default();
        config.risk.mode = crate::config::types::RiskMode::Security;
        let risk = classify(&spec("Typo", "one line", &["a.md"]), &config);
        assert_eq!(risk.level, RiskLevel::Security);
        assert!(risk.config_driven);
    }
}
