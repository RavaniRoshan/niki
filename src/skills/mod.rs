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

impl SkillMetadata {
    /// A blank record for a skill read from a `SKILL.md` header.
    ///
    /// Field-by-field rather than `..Default::default()` because `SkillMetadata` has no
    /// `Default`: the promotion path is supposed to supply every field deliberately, and a
    /// derived `Default` would let a new field be forgotten here without a compiler error.
    fn blank(name: &str) -> Self {
        Self {
            name: name.to_string(),
            version: default_skill_version(),
            description: String::new(),
            source_runs: Vec::new(),
            verdicts: Vec::new(),
            model: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
            status: default_skill_status(),
            retire_reason: None,
            snapshot_ref: String::new(),
            history: Vec::new(),
        }
    }
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

/// Where project skills live, resolved from the project's own configuration.
///
/// The runtime `skill_list`/`skill_load` tools used to read a hardcoded
/// `.niki/skills` because `ToolContext` carries no `&NikiConfig`, while
/// promotion wrote to `config.general.output_dir/skills`. The two never met for
/// anyone who set a custom output dir: a skill would be promoted with a success
/// message, appear in `niki skills list`, and then be invisible to the very
/// agents it was distilled for.
///
/// Two functions that disagree about a path is the whole bug, so there is now
/// one, and it asks the configuration rather than assuming. Loading the config
/// is a file read, and it happens inside a bounded tool loop that is about to
/// make an LLM call — the cost is not the reason to keep a second opinion about
/// where the skills are.
///
/// Falls back to `.niki` when no config is readable, which is the documented
/// default and the behaviour a project with no `niki.toml` has always had.
pub fn project_skills_root(project_path: &Path) -> PathBuf {
    let output_dir = match crate::config::NikiConfig::load(project_path) {
        Ok(cfg) => cfg.general.output_dir,
        Err(_) => ".niki".to_string(),
    };
    project_path.join(output_dir).join("skills")
}

/// Project skills, from wherever this project actually keeps them.
pub fn list_project_skills_for(project_path: &Path) -> Vec<String> {
    let root = project_skills_root(project_path);
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

pub fn load_project_skill_for(project_path: &Path, name: &str) -> Option<(String, String)> {
    let path = project_skills_root(project_path)
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

    /// Stage a candidate under `config`'s own output dir, so promotion can find
    /// it. The existing `stage_sample` hardcodes the default config, which is
    /// fine for its callers and wrong for the custom-output-dir case below.
    fn stage_sample_with(dir: &Path, config: &NikiConfig) -> String {
        stage_candidate(
            dir,
            config,
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

    fn stage_sample(dir: &Path) -> String {
        stage_sample_with(dir, &test_config())
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

    /// A skill the agents can actually see is the entire point of promoting one.
    ///
    /// It did not work under a custom `[general] output_dir`. Promotion wrote
    /// to `<output_dir>/skills` — the configured directory, correctly — while
    /// the runtime `skill_list`/`skill_load` tools read a hardcoded
    /// `.niki/skills`, because `ToolContext` carries no config. So the skill
    /// was promoted with a success message, listed by `niki skills list`, and
    /// then invisible to the agents it was distilled for. Nothing errored. The
    /// user was told it worked.
    ///
    /// Two functions that disagree about a path is the entire bug, so this
    /// pins that they now agree — for the default directory and for a custom
    /// one, which is where it was broken.
    #[test]
    fn a_promoted_skill_is_visible_to_the_agents_that_should_load_it() {
        for output_dir in [".niki", ".niki-custom"] {
            let tmp = tempfile::tempdir().unwrap();
            let root = tmp.path();
            std::fs::write(
                root.join("niki.toml"),
                format!("[general]\noutput_dir = \"{output_dir}\"\n"),
            )
            .unwrap();

            // Promote, through the configured path — the real sequence: stage a
            // candidate, then promote it, exactly as a run does.
            let mut cfg = test_config();
            cfg.general.output_dir = output_dir.to_string();
            let staged = stage_sample_with(root, &cfg);
            let name = promote_candidate(root, &cfg, &staged).expect("promotion succeeds");
            assert!(
                project_skills_dir(root, &cfg).join(&name).exists(),
                "promotion did not write under the configured output_dir"
            );

            // …and the runtime path, which is a different function entirely.
            let listed = list_project_skills_for(root);
            assert!(
                listed.contains(&name),
                "with output_dir = {output_dir}, the promoted skill is invisible to \
                 the tools that load it. Promotion reported success; listed: {listed:?}"
            );

            let loaded = load_project_skill_for(root, &name);
            assert!(
                loaded.is_some(),
                "with output_dir = {output_dir}, the skill lists but will not load"
            );
        }
    }

    /// And the default stays the default: a project with no `niki.toml` at all
    /// must still find skills in `.niki/skills`, which is where every existing
    /// project's promoted skills already are.
    #[test]
    fn a_project_with_no_config_still_resolves_the_default_skills_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let dir = root.join(".niki").join("skills").join("legacy-skill");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), "body").unwrap();

        assert_eq!(
            project_skills_root(root),
            root.join(".niki").join("skills"),
            "a project with no niki.toml must keep resolving the documented default"
        );
        assert!(list_project_skills_for(root).contains(&"legacy-skill".to_string()));
        assert!(load_project_skill_for(root, "legacy-skill").is_some());
    }
}

// ── SKILL.md frontmatter ────────────────────────────────────────────────────

/// One `key: value` from a SKILL.md frontmatter block.
///
/// The format is the one Claude Code, Kimi Code and the `~/.agents/skills/` layer all use:
/// a `---` fenced YAML header, then the Markdown body. Only flat `key: value` scalars are read,
/// because that is what the field names below are; a skill whose header uses nested YAML still
/// loads, with the fields NIKI uses populated and the rest ignored. Anything richer would be a
/// YAML dependency for keys no consumer here reads.
fn frontmatter_scalar(frontmatter: &str, key: &str) -> Option<String> {
    for line in frontmatter.lines() {
        let line = line.trim();
        // A nested or list line (`  - foo`, `key:`) is not the key being asked for.
        let Some(rest) = line.strip_prefix(key).and_then(|r| r.strip_prefix(':')) else {
            continue;
        };
        let value = rest.trim().trim_matches('"').trim_matches('\'').trim();
        if value.is_empty() {
            continue;
        }
        return Some(value.to_string());
    }
    None
}

/// Read a `SKILL.md` into NIKI's metadata plus the body, or `None` when it is not one.
///
/// `metadata.json` remains the source of truth for NIKI-promoted skills, because that is what
/// `niki skills promote` writes and what the lock file tracks. This is the *other* format: a
/// hand-written or third-party SKILL.md with no sibling JSON, which is how most skills in the
/// wild arrive. Before this, such a skill was silently invisible.
pub fn read_skill_md(path: &Path) -> Option<(SkillMetadata, String)> {
    let content = std::fs::read_to_string(path).ok()?;
    let (frontmatter, body) = split_skill_frontmatter(&content);
    if body.trim().is_empty() {
        return None;
    }

    // The directory name is the identity. A header `name:` that disagrees with it is ignored,
    // because that is what the tools address skills by, and a skill listed under a name it does
    // not answer to is worse than one listed under the name it does.
    let name = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();
    if name.is_empty() {
        return None;
    }

    // `summary` is the short form Claude Code shows; `description` is the long one. Prefer the
    // long one and fall back, rather than joining them into something no field described.
    let description = frontmatter_scalar(frontmatter, "description")
        .or_else(|| frontmatter_scalar(frontmatter, "summary"))
        .or_else(|| frontmatter_scalar(frontmatter, "when_to_use"))
        .unwrap_or_default();

    let version = frontmatter_scalar(frontmatter, "version")
        .and_then(|v| v.split('.').next().and_then(|n| n.parse::<u32>().ok()))
        .unwrap_or_else(default_skill_version);

    let status = match frontmatter_scalar(frontmatter, "status").as_deref() {
        Some("retired") => SkillStatus::Retired,
        _ => SkillStatus::Active,
    };

    let mut meta = SkillMetadata::blank(&name);
    meta.version = version;
    meta.description = description;
    meta.status = status;
    Some((meta, body.to_string()))
}

/// Split a SKILL.md into `(frontmatter, body)`.
///
/// Without a leading `---`, the whole file is the body — the same rule the slash-command
/// registry uses, because one rule for two formats is easier to keep honest than two.
fn split_skill_frontmatter(content: &str) -> (&str, &str) {
    let Some(rest) = content.strip_prefix("---") else {
        return ("", content);
    };
    let rest = rest.strip_prefix('\n').unwrap_or(rest);
    for (idx, line) in rest.split_inclusive('\n').enumerate() {
        if line.trim() == "---" {
            let offset: usize = rest.split_inclusive('\n').take(idx + 1).map(str::len).sum();
            return (&rest[..offset - line.len()], &rest[offset..]);
        }
    }
    ("", content)
}

/// Every directory that may hold skills, in precedence order.
///
/// NIKI's own promoted skills come first, then the formats other tools write. Searching
/// `.claude/skills` and `.agents/skills` is what makes an existing skill *work* here rather than
/// merely being present on disk.
pub fn skill_search_paths(project_path: &Path, config: &NikiConfig) -> Vec<PathBuf> {
    let mut out = vec![project_skills_dir(project_path, config)];
    for rel in [".claude/skills", ".agents/skills"] {
        let p = project_path.join(rel);
        if p.is_dir() {
            out.push(p);
        }
    }
    if let Some(home) = dirs::home_dir() {
        let p = home.join(".agents/skills");
        if p.is_dir() {
            out.push(p);
        }
    }
    out
}

/// Every skill visible to this project, from every path, without duplicates.
///
/// A skill in two paths appears once, taking the first: NIKI's own promoted copy is the one it
/// maintains and the one whose hash the lock file tracks.
pub fn list_all_skills(project_path: &Path, config: &NikiConfig) -> Vec<(SkillMetadata, String)> {
    let mut seen: Vec<String> = Vec::new();
    let mut out: Vec<(SkillMetadata, String)> = Vec::new();

    for root in skill_search_paths(project_path, config) {
        let Ok(rd) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in rd.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if seen.iter().any(|s| s == name) {
                continue;
            }
            let found = read_skill_md(&dir.join("SKILL.md")).or_else(|| {
                let body = std::fs::read_to_string(dir.join("SKILL.md")).ok()?;
                let text = std::fs::read_to_string(dir.join("metadata.json")).ok()?;
                let meta = serde_json::from_str::<SkillMetadata>(&text).ok()?;
                Some((meta, body))
            });
            let Some((meta, body)) = found else {
                continue;
            };
            if meta.status == SkillStatus::Retired {
                continue;
            }
            seen.push(name.to_string());
            out.push((meta, body));
        }
    }
    out.sort_by(|a, b| a.0.name.cmp(&b.0.name));
    out
}

/// Load one skill body by name, from any search path.
pub fn load_skill(
    project_path: &Path,
    config: &NikiConfig,
    name: &str,
) -> Option<(String, String)> {
    for root in skill_search_paths(project_path, config) {
        let dir = root.join(name);
        if let Ok(body) = std::fs::read_to_string(dir.join("SKILL.md")) {
            return Some((body, dir.display().to_string()));
        }
    }
    None
}

#[cfg(test)]
mod frontmatter_tests {
    use super::*;

    #[test]
    fn a_skill_md_with_frontmatter_yields_name_description_and_body() {
        let dir = tempfile::tempdir().expect("tempdir");
        let d = dir.path().join("aha");
        std::fs::create_dir_all(&d).expect("mkdir");
        std::fs::write(
            d.join("SKILL.md"),
            "---\nname: aha\ndescription: Use when hunting bugs.\nversion: \"20.1.0\"\n---\n\nDo the thing.\n",
        )
        .expect("write");

        let (meta, body) = read_skill_md(&d.join("SKILL.md")).expect("parses");
        assert_eq!(meta.name, "aha", "the directory name is the identity");
        assert_eq!(meta.description, "Use when hunting bugs.");
        assert_eq!(meta.version, 20, "a two-part version yields its major");
        assert!(body.contains("Do the thing."));
        assert!(
            !body.contains("description:"),
            "frontmatter leaked into the body: {body}"
        );
    }

    #[test]
    fn a_real_third_party_header_loads() {
        // The shape an actual `~/.agents/skills` entry uses: extra keys, single and double
        // quotes, a summary and a when_to_use, and a non-numeric version.
        let dir = tempfile::tempdir().expect("tempdir");
        let d = dir.path().join("email-render-builder");
        std::fs::create_dir_all(&d).expect("mkdir");
        std::fs::write(
            d.join("SKILL.md"),
            "---\nname: email-render-builder\nslug: a-x\ndisplayName: \"Email Render Builder\"\nsummary: \"email HTML\"\ndescription: 'Use when the user asks to build the email HTML'\nversion: \"20.1.0\"\nlicense: Apache-2.0\nwhen_to_use: \"Use when coding the HTML build\"\nargument-hint: \"<creative> [clients]\"\n---\n\n# Body here\n",
        )
        .expect("write");

        let (meta, body) = read_skill_md(&d.join("SKILL.md")).expect("parses");
        assert_eq!(meta.name, "email-render-builder");
        assert!(
            meta.description.starts_with("Use when the user asks"),
            "the quoted description did not parse: {:?}",
            meta.description
        );
        assert!(body.contains("# Body here"));
    }

    #[test]
    fn a_longer_key_does_not_match_a_prefix_of_itself() {
        // `name:` must not be satisfied by `namespace:`, which is how a naive scanner reports a
        // field that is not there.
        let fm = "namespace: other\nname: real\n";
        assert_eq!(frontmatter_scalar(fm, "name").as_deref(), Some("real"));
    }

    #[test]
    fn summary_and_when_to_use_are_fallbacks_not_additions() {
        let dir = tempfile::tempdir().expect("tempdir");
        for (header, expected) in [
            ("summary: short only", "short only"),
            ("when_to_use: when only", "when only"),
        ] {
            let d = dir.path().join(format!("s{}", expected.len()));
            std::fs::create_dir_all(&d).expect("mkdir");
            std::fs::write(d.join("SKILL.md"), format!("---\n{header}\n---\nbody\n"))
                .expect("write");
            let (meta, _) = read_skill_md(&d.join("SKILL.md")).expect("parses");
            assert_eq!(
                meta.description, expected,
                "fallback did not apply for {header}"
            );
        }
    }

    #[test]
    fn a_file_with_no_frontmatter_is_all_body() {
        let dir = tempfile::tempdir().expect("tempdir");
        let d = dir.path().join("plain");
        std::fs::create_dir_all(&d).expect("mkdir");
        std::fs::write(d.join("SKILL.md"), "Just a body, no header.\n").expect("write");
        let (meta, body) = read_skill_md(&d.join("SKILL.md")).expect("parses");
        assert_eq!(
            meta.description, "",
            "no header means no description, not a invented one"
        );
        assert_eq!(body.trim(), "Just a body, no header.");
        assert_eq!(meta.version, default_skill_version());
    }

    #[test]
    fn an_unterminated_header_is_all_body() {
        let (fm, body) = split_skill_frontmatter("---\nname: x\nno closing fence\n");
        assert_eq!(fm, "", "an unterminated header must not be parsed as one");
        assert!(body.contains("name: x"));
    }

    #[test]
    fn a_retired_skill_is_marked_retired() {
        let dir = tempfile::tempdir().expect("tempdir");
        let d = dir.path().join("old");
        std::fs::create_dir_all(&d).expect("mkdir");
        std::fs::write(d.join("SKILL.md"), "---\nstatus: retired\n---\nbody\n").expect("write");
        let (meta, _) = read_skill_md(&d.join("SKILL.md")).expect("parses");
        assert_eq!(meta.status, SkillStatus::Retired);
    }

    #[test]
    fn a_skill_with_no_body_is_not_a_skill() {
        let dir = tempfile::tempdir().expect("tempdir");
        let d = dir.path().join("empty");
        std::fs::create_dir_all(&d).expect("mkdir");
        std::fs::write(d.join("SKILL.md"), "---\nname: empty\n---\n").expect("write");
        assert!(
            read_skill_md(&d.join("SKILL.md")).is_none(),
            "a header with no body is not a skill"
        );
    }
}
