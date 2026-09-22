use anyhow::Result;
use clap::Subcommand;
use std::path::PathBuf;

use crate::config::NikiConfig;

#[derive(Subcommand)]
pub enum SkillsCommands {
    /// List promoted project skills (active only; retired stay in the lock).
    List {
        /// Project directory (defaults to the current directory).
        #[arg(long)]
        project: Option<PathBuf>,
    },
    /// List staged distillation candidates (Approved runs awaiting promotion).
    Candidates {
        /// Project directory (defaults to the current directory).
        #[arg(long)]
        project: Option<PathBuf>,
    },
    /// Promote a staged candidate into a versioned project skill.
    Promote {
        /// Candidate id (see `candidates`).
        candidate: String,
        /// Project directory (defaults to the current directory).
        #[arg(long)]
        project: Option<PathBuf>,
    },
    /// Retire a promoted skill (removed from listing, reason kept in lock).
    Retire {
        /// Skill name (see `list`).
        name: String,
        /// Why the skill is retired (required).
        #[arg(long)]
        reason: String,
        /// Project directory (defaults to the current directory).
        #[arg(long)]
        project: Option<PathBuf>,
    },
    /// Show a skill's SKILL.md body.
    Show {
        /// Skill name (see `list`).
        name: String,
        /// Project directory (defaults to the current directory).
        #[arg(long)]
        project: Option<PathBuf>,
    },
    /// Diff a staged candidate against the promoted skill of the same shape.
    Diff {
        /// Candidate id (see `candidates`).
        candidate: String,
        /// Project directory (defaults to the current directory).
        #[arg(long)]
        project: Option<PathBuf>,
    },
}

pub fn handle(command: &SkillsCommands) -> Result<()> {
    match command {
        SkillsCommands::List { project } => cmd_list(project.clone()),
        SkillsCommands::Candidates { project } => cmd_candidates(project.clone()),
        SkillsCommands::Promote { candidate, project } => cmd_promote(project.clone(), candidate),
        SkillsCommands::Retire {
            name,
            reason,
            project,
        } => cmd_retire(project.clone(), name, reason),
        SkillsCommands::Show { name, project } => cmd_show(project.clone(), name),
        SkillsCommands::Diff { candidate, project } => cmd_diff(project.clone(), candidate),
    }
}

fn project_or_cwd(project: Option<PathBuf>) -> PathBuf {
    project.unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

fn load(project_dir: &std::path::Path) -> NikiConfig {
    NikiConfig::load(project_dir).unwrap_or_default()
}

fn cmd_list(project: Option<PathBuf>) -> Result<()> {
    let dir = project_or_cwd(project);
    let config = load(&dir);
    let skills = crate::skills::list_project_skills(&dir, &config);
    if skills.is_empty() {
        println!("No promoted skills. Approved runs with a green suite stage candidates;");
        println!("promote one with `niki skills promote --candidate <id>`.");
        return Ok(());
    }
    let head = crate::skills::head_snapshot_ref(&dir);
    for (meta, _) in skills {
        let stale = if crate::skills::skill_is_stale(&meta, &head) {
            " [stale: source snapshot differs from HEAD]"
        } else {
            ""
        };
        println!(
            "- {} v{} (from {}, snapshot {}){}",
            meta.name,
            meta.version,
            meta.source_runs.join(","),
            meta.snapshot_ref,
            stale,
        );
    }
    Ok(())
}

fn cmd_candidates(project: Option<PathBuf>) -> Result<()> {
    let dir = project_or_cwd(project);
    let config = load(&dir);
    let cands = crate::skills::list_candidates(&dir, &config);
    if cands.is_empty() {
        println!("No staged candidates.");
        return Ok(());
    }
    for (id, task) in cands {
        println!("- {id}: {task}");
    }
    Ok(())
}

fn cmd_promote(project: Option<PathBuf>, candidate: &str) -> Result<()> {
    let dir = project_or_cwd(project);
    let config = load(&dir);
    let name = crate::skills::promote_candidate(&dir, &config, candidate)?;
    println!("Promoted skill '{name}'.");
    Ok(())
}

fn cmd_retire(project: Option<PathBuf>, name: &str, reason: &str) -> Result<()> {
    let dir = project_or_cwd(project);
    let config = load(&dir);
    crate::skills::retire_skill(&dir, &config, name, reason)?;
    println!("Retired skill '{name}': {reason}");
    Ok(())
}

fn cmd_show(project: Option<PathBuf>, name: &str) -> Result<()> {
    let dir = project_or_cwd(project);
    let config = load(&dir);
    match crate::skills::load_project_skill(&dir, &config, name) {
        Some((body, source)) => {
            println!("--- {source} ---\n{body}");
            Ok(())
        }
        None => anyhow::bail!("skill '{name}' is not promoted"),
    }
}

fn cmd_diff(project: Option<PathBuf>, candidate: &str) -> Result<()> {
    let dir = project_or_cwd(project);
    let config = load(&dir);
    let stage = crate::skills::staging_dir(&dir, &config).join(candidate);
    let text = std::fs::read_to_string(stage.join("candidate.json"))
        .map_err(|_| anyhow::anyhow!("candidate '{candidate}' not found in staging"))?;
    let cand: crate::skills::StagedCandidate = serde_json::from_str(&text)?;
    let name = crate::skills::slugify(&cand.task_description);
    let promoted = crate::skills::load_project_skill(&dir, &config, &name)
        .map(|(b, _)| b)
        .unwrap_or_else(|| "(not yet promoted — everything below is new)\n".to_string());
    let staged = crate::skills::render_skill_md(&cand, &name);
    println!("--- promoted: {name}\n+++ candidate: {candidate}");
    let diff = similar::TextDiff::from_lines(&promoted, &staged);
    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            similar::ChangeTag::Delete => "-",
            similar::ChangeTag::Insert => "+",
            similar::ChangeTag::Equal => " ",
        };
        print!("{sign}{change}");
    }
    Ok(())
}
