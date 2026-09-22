//! Skill distillation and promotion (Phase 4.4, Layer 7).
//!
//! Skills are versioned workflows distilled from successful runs. The flow is
//! deliberately two-step — nothing auto-activates:
//!
//! ```text
//! Approved run + green suite
//!   → stage_candidate()      writes <output_dir>/skills-staging/<id>/
//!   → `niki skills promote`  moves it to <output_dir>/skills/<name>/
//!                              + records skills-lock.json
//!   → `niki skills retire`   removes it from listing, reason kept in lock
//! ```
//!
//! `skill_list`/`skill_load` serve promoted skills alongside the shared
//! `~/.agents/skills/` layer. A skill whose source snapshot no longer matches
//! HEAD is flagged stale, never silently served as fresh.

use crate::config::NikiConfig;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Active vs retired lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillStatus {
    Active,
    Retired,
}

/// Versioned skill record (`skills/<name>/metadata.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub name: String,
    #[serde(default = "default_skill_version")]
    pub version: u32,
    #[serde(default)]
    pub description: String,
    /// Task ids this skill was distilled from.
    #[serde(default)]
    pub source_runs: Vec<String>,
    /// Verdicts observed for the source runs (e.g. `Approved`).
    #[serde(default)]
    pub verdicts: Vec<String>,
    /// Model that produced the source runs, when known.
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default = "default_skill_status")]
    pub status: SkillStatus,
    #[serde(default)]
    pub retire_reason: Option<String>,
    /// Commit sha (or `nongit`) the source run executed under. Used for
    /// stale-due-to-repo-drift detection.
    #[serde(default)]
    pub snapshot_ref: String,
    /// Previous versions, newest first (kept on supersede).
    #[serde(default)]
    pub history: Vec<SkillMetadata>,
}

fn default_skill_version() -> u32 {
    1
}

fn default_skill_status() -> SkillStatus {
    SkillStatus::Active
}

/// Lock entry (`skills-lock.json`: name → version → content hash → runs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillLockEntry {
    pub version: u32,
    pub content_hash: String,
    #[serde(default)]
    pub source_runs: Vec<String>,
    #[serde(default)]
    pub retired: bool,
    #[serde(default)]
    pub retire_reason: Option<String>,
}

/// A staged (not yet activated) distillation candidate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedCandidate {
    pub id: String,
    pub task_description: String,
    #[serde(default)]
    pub plan_shape: String,
    #[serde(default)]
    pub test_command: String,
    #[serde(default)]
    pub review_notes: String,
    #[serde(default)]
    pub verdict: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub snapshot_ref: String,
    #[serde(default)]
    pub created_at: String,
}

pub fn staging_dir(project_path: &Path, config: &NikiConfig) -> PathBuf {
    project_path
        .join(&config.general.output_dir)
        .join("skills-staging")
}

/// Promoted project skills. Served by `skill_list`/`skill_load` alongside the
/// shared layer. Uses the configured `output_dir`, not a hardcoded `.niki`.
pub fn project_skills_dir(project_path: &Path, config: &NikiConfig) -> PathBuf {
    project_path.join(&config.general.output_dir).join("skills")
}

pub fn lock_path(project_path: &Path, config: &NikiConfig) -> PathBuf {
    project_path
        .join(&config.general.output_dir)
        .join("skills-lock.json")
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn content_hash(text: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    format!("{:x}", h.finish())
}

/// Slugify a task description into a skill-name stem.
pub fn slugify(task: &str) -> String {
    let slug: String = task
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(5)
        .collect::<Vec<_>>()
        .join("-");
    let slug: String = slug.chars().take(40).collect();
    if slug.is_empty() {
        "untitled".to_string()
    } else {
        slug
    }
}

/// Render the SKILL.md body for a candidate. Format v1: title, provenance
/// quote, when-to-use, what-worked, freshness. See
/// `docs/content/05-cli-reference/07-skills.mdx`.
pub fn render_skill_md(candidate: &StagedCandidate, skill_name: &str) -> String {
    format!(
        "# Skill: {skill_name}\n\n\
         > Distilled from niki run `{task}` (verdict: {verdict}, model: {model}).\n\
         > Promoted skills are versioned workflows, not facts: verify paths and\n\
         > commands against the current tree before following them.\n\n\
         ## When to use\n\n\
         Tasks shaped like: {task}\n\n\
         ## What worked\n\n\
         - Plan shape: {plan}\n\
         - Test command: `{tests}`\n\
         - Review notes: {notes}\n\n\
         ## Freshness\n\n\
         - Source snapshot: `{snapshot}` (flag stale when HEAD differs).\n",
        skill_name = skill_name,
        task = candidate
            .task_description
            .chars()
            .take(300)
            .collect::<String>(),
        verdict = candidate.verdict,
        model = if candidate.model.is_empty() {
            "unknown".to_string()
        } else {
            candidate.model.clone()
        },
        plan = if candidate.plan_shape.is_empty() {
            "(not recorded)".to_string()
        } else {
            candidate.plan_shape.chars().take(500).collect()
        },
        tests = if candidate.test_command.is_empty() {
            "(none)"
        } else {
            &candidate.test_command
        },
        notes = if candidate.review_notes.is_empty() {
            "(none)".to_string()
        } else {
            candidate.review_notes.chars().take(500).collect()
        },
        snapshot = candidate.snapshot_ref,
    )
}

/// Distillation trigger: stage a candidate from an Approved run with a green
/// executed suite. Never auto-activates — promotion is an explicit CLI step.
/// Returns the candidate id. Best-effort by contract: callers ignore errors.
pub fn stage_candidate(
    project_path: &Path,
    config: &NikiConfig,
    task_description: &str,
    plan_shape: &str,
    test_command: &str,
    review_notes: &str,
    verdict: &str,
    model: &str,
    snapshot_ref: &str,
    task_id: &str,
) -> anyhow::Result<String> {
    let id = format!(
        "{}-{}",
        slugify(task_description),
        task_id.chars().take(8).collect::<String>()
    );
    let candidate = StagedCandidate {
        id: id.clone(),
        task_description: task_description.chars().take(500).collect(),
        plan_shape: plan_shape.chars().take(1000).collect(),
        test_command: test_command.to_string(),
        review_notes: review_notes.chars().take(1000).collect(),
        verdict: verdict.to_string(),
        model: model.to_string(),
        snapshot_ref: snapshot_ref.to_string(),
        created_at: now_rfc3339(),
    };
    let dir = staging_dir(project_path, config).join(&id);
    std::fs::create_dir_all(&dir)?;
    let name = slugify(task_description);
    crate::knowledge::kb::write_atomic(
        &dir.join("SKILL.md"),
        render_skill_md(&candidate, &name).as_bytes(),
    )?;
    crate::knowledge::kb::write_atomic(
        &dir.join("candidate.json"),
        serde_json::to_string_pretty(&candidate)?.as_bytes(),
    )?;
    Ok(id)
}

/// List staged candidates (id + task), oldest first.
pub fn list_candidates(project_path: &Path, config: &NikiConfig) -> Vec<(String, String)> {
    let root = staging_dir(project_path, config);
    let Ok(rd) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in rd.flatten() {
        let path = entry.path().join("candidate.json");
        if !path.is_file() {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&path)
            && let Ok(c) = serde_json::from_str::<StagedCandidate>(&text)
        {
            out.push((c.id.clone(), c.task_description.clone()));
        }
    }
    out.sort();
    out
}

fn read_lock(
    project_path: &Path,
    config: &NikiConfig,
) -> serde_json::Map<String, serde_json::Value> {
    std::fs::read_to_string(lock_path(project_path, config))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn write_lock(
    project_path: &Path,
    config: &NikiConfig,
    lock: &serde_json::Map<String, serde_json::Value>,
) -> anyhow::Result<()> {
    crate::knowledge::kb::write_atomic(
        &lock_path(project_path, config),
        serde_json::to_string_pretty(lock)?.as_bytes(),
    )?;
    Ok(())
}

/// Activation path: move a staged candidate into `skills/<name>/` and record
/// the lock entry. A same-name skill with an older version is superseded
/// (version bump, previous metadata kept in `history`).
pub fn promote_candidate(
    project_path: &Path,
    config: &NikiConfig,
    candidate_id: &str,
) -> anyhow::Result<String> {
    let stage = staging_dir(project_path, config).join(candidate_id);
    let text = std::fs::read_to_string(stage.join("candidate.json"))
        .map_err(|e| anyhow::anyhow!("candidate '{candidate_id}' not found in staging: {e}"))?;
    let candidate: StagedCandidate = serde_json::from_str(&text)?;
    let name = slugify(&candidate.task_description);
    let dest = project_skills_dir(project_path, config).join(&name);
    std::fs::create_dir_all(&dest)?;

    let body = render_skill_md(&candidate, &name);
    let hash = content_hash(&body);
    crate::knowledge::kb::write_atomic(&dest.join("SKILL.md"), body.as_bytes())?;

    // Supersede: keep the previous metadata in history, bump the version.
    let mut history = Vec::new();
    let mut version = 1u32;
    let meta_path = dest.join("metadata.json");
    if meta_path.is_file()
        && let Ok(text) = std::fs::read_to_string(&meta_path)
        && let Ok(mut prev) = serde_json::from_str::<SkillMetadata>(&text)
    {
        version = prev.version + 1;
        prev.history.clear();
        history.push(prev);
    }
    let meta = SkillMetadata {
        name: name.clone(),
        version,
        description: candidate.task_description.clone(),
        source_runs: vec![candidate.id.clone()],
        verdicts: vec![candidate.verdict.clone()],
        model: candidate.model.clone(),
        created_at: if history.is_empty() {
            now_rfc3339()
        } else {
            history[0].created_at.clone()
        },
        updated_at: now_rfc3339(),
        status: SkillStatus::Active,
        retire_reason: None,
        snapshot_ref: candidate.snapshot_ref.clone(),
        history,
    };
    crate::knowledge::kb::write_atomic(
        &meta_path,
        serde_json::to_string_pretty(&meta)?.as_bytes(),
    )?;

    let mut lock = read_lock(project_path, config);
    let mut runs = vec![candidate.id.clone()];
    if let Some(prev) = lock
        .get(&name)
        .and_then(|v| serde_json::from_str::<SkillLockEntry>(v.to_string().as_str()).ok())
    {
        for r in prev.source_runs {
            if !runs.contains(&r) {
                runs.push(r);
            }
        }
    }
    lock.insert(
        name.clone(),
        serde_json::to_value(&SkillLockEntry {
            version,
            content_hash: hash,
            source_runs: runs,
            retired: false,
            retire_reason: None,
        })
        .unwrap_or(serde_json::Value::Null),
    );
    write_lock(project_path, config, &lock)?;

    // Candidate is consumed by promotion.
    let _ = std::fs::remove_dir_all(&stage);
    Ok(name)
}

/// Retirement: remove from listing, preserve the reason in the lock.
/// The skill directory is removed; history in the lock is the record.
pub fn retire_skill(
    project_path: &Path,
    config: &NikiConfig,
    name: &str,
    reason: &str,
) -> anyhow::Result<()> {
    let dir = project_skills_dir(project_path, config).join(name);
    if !dir.is_dir() {
        anyhow::bail!("skill '{name}' is not promoted");
    }
    let meta: Option<SkillMetadata> = std::fs::read_to_string(dir.join("metadata.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    std::fs::remove_dir_all(&dir)?;
    let mut lock = read_lock(project_path, config);
    let entry = lock
        .get(name)
        .and_then(|v| serde_json::from_str::<SkillLockEntry>(v.to_string().as_str()).ok());
    let (version, hash, runs) = match (entry, meta) {
        (Some(e), _) => (e.version, e.content_hash, e.source_runs),
        (None, Some(m)) => (m.version, String::new(), m.source_runs.clone()),
        (None, None) => (1, String::new(), Vec::new()),
    };
    lock.insert(
        name.to_string(),
        serde_json::to_value(&SkillLockEntry {
            version,
            content_hash: hash,
            source_runs: runs,
            retired: true,
            retire_reason: Some(reason.to_string()),
        })
        .unwrap_or(serde_json::Value::Null),
    );
    write_lock(project_path, config, &lock)?;
    Ok(())
}

/// Active promoted skills: `(metadata, skill-body)`. Retired skills are
/// excluded; the lock is the record of why.
pub fn list_project_skills(
    project_path: &Path,
    config: &NikiConfig,
) -> Vec<(SkillMetadata, String)> {
    let root = project_skills_dir(project_path, config);
    let Ok(rd) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in rd.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let (Ok(body), Ok(mtext)) = (
            std::fs::read_to_string(dir.join("SKILL.md")),
            std::fs::read_to_string(dir.join("metadata.json")),
        ) else {
            continue;
        };
        let Ok(meta) = serde_json::from_str::<SkillMetadata>(&mtext) else {
            continue;
        };
        if meta.status == SkillStatus::Retired {
            continue;
        }
        out.push((meta, body));
    }
    out.sort_by(|a, b| a.0.name.cmp(&b.0.name));
    out
}

/// Load one promoted project skill body by name.
pub fn load_project_skill(
    project_path: &Path,
    config: &NikiConfig,
    name: &str,
) -> Option<(String, String)> {
    let dir = project_skills_dir(project_path, config).join(name);
    let body = std::fs::read_to_string(dir.join("SKILL.md")).ok()?;
    Some((body, dir.display().to_string()))
}

/// Project skills under the default output dir (for tool contexts that do not
/// carry a full config — e.g. the runtime `skill_list`/`skill_load` tools).
pub fn list_project_skills_default_dir(project_path: &Path) -> Vec<String> {
    let root = project_path.join(".niki").join("skills");
    let Ok(rd) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut names: Vec<String> = rd
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
        // Retired skills have no directory, so everything listed here is active.
        .collect();
    names.sort();
    names
}

pub fn load_project_skill_default_dir(project_path: &Path, name: &str) -> Option<(String, String)> {
    let path = project_path
        .join(".niki")
        .join("skills")
        .join(name)
        .join("SKILL.md");
    let body = std::fs::read_to_string(&path).ok()?;
    Some((body, path.display().to_string()))
}

/// Current HEAD short sha for snapshot stamps. `nongit` when unavailable —
/// staleness is then unknown and the skill is never flagged on that basis.
pub fn head_snapshot_ref(project_path: &Path) -> String {
    let sha: Option<String> = (|| {
        let repo = git2::Repository::open(project_path).ok()?;
        let obj = repo.revparse_single("HEAD").ok()?;
        Some(obj.id().to_string())
    })();
    sha.map(|s| s.chars().take(8).collect())
        .unwrap_or_else(|| "nongit".to_string())
}

/// Stale-due-to-repo-drift: the source snapshot no longer matches HEAD.
/// Unknown (`nongit`) snapshots never flag.
pub fn skill_is_stale(meta: &SkillMetadata, head_ref: &str) -> bool {
    !meta.snapshot_ref.is_empty()
        && meta.snapshot_ref != "nongit"
        && !head_ref.is_empty()
        && head_ref != "nongit"
        && meta.snapshot_ref != head_ref
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> NikiConfig {
        NikiConfig::default()
    }

    fn stage_sample(dir: &Path) -> String {
        stage_candidate(
            dir,
            &test_config(),
            "Add health endpoint",
            "planner spec, coder diff, tester report",
            "cargo test",
            "reviewer approved",
            "Approved",
            "test-model",
            "abc12345",
            "aaaaaaaa-1111",
        )
        .unwrap()
    }

    #[test]
    fn candidate_stages_without_activating() {
        // Phase 4.4: an Approved run yields a staged candidate that is NOT
        // yet visible via skill_list.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let id = stage_sample(dir);
        assert!(staging_dir(dir, &test_config()).join(&id).is_dir());
        assert!(
            list_project_skills(dir, &test_config()).is_empty(),
            "staged candidates must not auto-activate"
        );
        let cands = list_candidates(dir, &test_config());
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].0, id);
    }

    #[test]
    fn promote_makes_skill_visible_and_retire_removes_it() {
        // Phase 4.4 acceptance: promote → visible via skill_list;
        // retire → removed from listing with reason preserved in the lock.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let config = test_config();
        let id = stage_sample(dir);

        let name = promote_candidate(dir, &config, &id).unwrap();
        let skills = list_project_skills(dir, &config);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].0.name, name);
        assert_eq!(skills[0].0.version, 1);
        assert!(skills[0].1.contains("Add health endpoint"));

        let loaded = load_project_skill(dir, &config, &name).expect("loadable after promote");
        assert!(loaded.0.contains("cargo test"));

        retire_skill(dir, &config, &name, "superseded by better evidence").unwrap();
        assert!(
            list_project_skills(dir, &config).is_empty(),
            "retired skills leave the listing"
        );
        let lock_text = std::fs::read_to_string(lock_path(dir, &config)).unwrap();
        let lock: serde_json::Value = serde_json::from_str(&lock_text).unwrap();
        assert_eq!(lock[&name]["retired"], true);
        assert_eq!(
            lock[&name]["retire_reason"],
            "superseded by better evidence"
        );
    }

    #[test]
    fn repromote_supersedes_with_version_bump_and_history() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let config = test_config();
        let id1 = stage_sample(dir);
        let name = promote_candidate(dir, &config, &id1).unwrap();
        let id2 = stage_candidate(
            dir,
            &config,
            "Add health endpoint",
            "new plan shape with matrix tests",
            "cargo test --all",
            "second approval",
            "Approved",
            "test-model",
            "def67890",
            "bbbbbbbb-2222",
        )
        .unwrap();
        // Same task text but a different source run: ids embed the task id.
        assert_ne!(id1, id2);
        let name2 = promote_candidate(dir, &config, &id2).unwrap();
        assert_eq!(name, name2, "same task shape supersedes the same skill");
        let skills = list_project_skills(dir, &config);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].0.version, 2);
        assert_eq!(skills[0].0.history.len(), 1);
        assert_eq!(skills[0].0.history[0].version, 1);
    }

    #[test]
    fn stale_detection_flags_drifted_snapshot_only() {
        let meta = SkillMetadata {
            name: "s".into(),
            version: 1,
            description: String::new(),
            source_runs: vec![],
            verdicts: vec![],
            model: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
            status: SkillStatus::Active,
            retire_reason: None,
            snapshot_ref: "abc12345".into(),
            history: vec![],
        };
        assert!(skill_is_stale(&meta, "def67890"));
        assert!(!skill_is_stale(&meta, "abc12345"));
        assert!(!skill_is_stale(&meta, "nongit"), "unknown HEAD never flags");
        let mut unknown = meta.clone();
        unknown.snapshot_ref = "nongit".into();
        assert!(!skill_is_stale(&unknown, "def67890"));
    }

    #[test]
    fn promote_unknown_candidate_errors() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(promote_candidate(tmp.path(), &test_config(), "no-such-id").is_err());
    }

    #[test]
    fn retire_unpromoted_skill_errors() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(retire_skill(tmp.path(), &test_config(), "ghost", "reason").is_err());
    }
}
