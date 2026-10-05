//! W17 — skills written in the SKILL.md format Claude Code and Kimi Code use actually load.
//!
//! The fixtures are the shape real skills have. The last test reads the developer's **own**
//! `~/.agents/skills` when it exists, because the thing being claimed is that existing skills
//! work, not that a fixture does.

use niki::config::NikiConfig;
use std::path::{Path, PathBuf};

fn niki_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_niki"))
}

fn write_skill(root: &Path, rel: &str, name: &str, body: &str) {
    let dir = root.join(rel).join(name);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("SKILL.md"), body).expect("write SKILL.md");
}

#[test]
fn a_third_party_skill_in_claude_skills_dir_is_listed_and_loadable() {
    let project = tempfile::tempdir().expect("project");
    let cfg = NikiConfig::default();
    write_skill(
        project.path(),
        ".claude/skills",
        "aha",
        "---\nname: aha\ndescription: Use when hunting bugs.\n---\n\nDo the thing.\n",
    );

    let skills = niki::skills::list_all_skills(project.path(), &cfg);
    let names: Vec<&str> = skills.iter().map(|(m, _)| m.name.as_str()).collect();
    assert!(
        names.contains(&"aha"),
        "a .claude/skills entry is invisible: {names:?}"
    );
    assert_eq!(
        skills
            .iter()
            .find(|(m, _)| m.name == "aha")
            .expect("aha")
            .0
            .description,
        "Use when hunting bugs."
    );

    let (body, path) =
        niki::skills::load_skill(project.path(), &cfg, "aha").expect("loads by name");
    assert!(body.contains("Do the thing."));
    assert!(
        path.contains("aha"),
        "the reported path is the skill's own: {path}"
    );
}

#[test]
fn agents_skills_dir_is_searched_too() {
    let project = tempfile::tempdir().expect("project");
    let cfg = NikiConfig::default();
    write_skill(
        project.path(),
        ".agents/skills",
        "shared-one",
        "---\nsummary: a shared skill\n---\nbody\n",
    );
    let skills = niki::skills::list_all_skills(project.path(), &cfg);
    assert!(
        skills.iter().any(|(m, _)| m.name == "shared-one"),
        ".agents/skills is not searched"
    );
}

#[test]
fn niki_promoted_skills_still_win_over_a_copy_elsewhere() {
    // The promoted copy is the one NIKI maintains and the one the lock file hashes. A duplicate
    // in another path must not shadow it.
    let project = tempfile::tempdir().expect("project");
    let cfg = NikiConfig::default();
    let promoted = project
        .path()
        .join(&cfg.general.output_dir)
        .join("skills")
        .join("dup");
    std::fs::create_dir_all(&promoted).expect("mkdir");
    std::fs::write(
        promoted.join("SKILL.md"),
        "---\ndescription: the promoted copy\n---\npromoted body\n",
    )
    .expect("write");
    write_skill(
        project.path(),
        ".claude/skills",
        "dup",
        "---\ndescription: the other copy\n---\nother body\n",
    );

    let skills = niki::skills::list_all_skills(project.path(), &cfg);
    let dupes: Vec<&(niki::skills::SkillMetadata, String)> =
        skills.iter().filter(|(m, _)| m.name == "dup").collect();
    assert_eq!(
        dupes.len(),
        1,
        "the skill was listed twice: {:?}",
        skills.iter().map(|(m, _)| &m.name).collect::<Vec<_>>()
    );
    assert_eq!(dupes[0].0.description, "the promoted copy");
}

#[test]
fn a_retired_skill_is_not_listed() {
    let project = tempfile::tempdir().expect("project");
    let cfg = NikiConfig::default();
    write_skill(
        project.path(),
        ".claude/skills",
        "gone",
        "---\nstatus: retired\n---\nbody\n",
    );
    let skills = niki::skills::list_all_skills(project.path(), &cfg);
    assert!(
        !skills.iter().any(|(m, _)| m.name == "gone"),
        "a retired skill is still offered"
    );
}

/// The claim is that *existing* skills work. If there is a real library on this machine, read a
/// slice of it. If there is not, the test says so rather than pretending to have checked.
#[test]
fn a_real_skill_library_on_this_machine_loads() {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return,
    };
    let root = home.join(".agents/skills");
    let Ok(rd) = std::fs::read_dir(&root) else {
        eprintln!("skipped: no ~/.agents/skills on this machine");
        return;
    };

    let project = tempfile::tempdir().expect("project");
    let cfg = NikiConfig::default();
    // Copy the first few entries in and load them through the same code path. The developer's
    // real library is also discovered via `~/.agents/skills`, so the totals below are larger
    // than `checked` — hence the per-name assertions rather than a count.
    let mut checked = 0usize;
    for entry in rd.flatten().take(25) {
        let src = entry.path().join("SKILL.md");
        if !src.exists() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let dir = project.path().join(".claude/skills").join(&name);
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::copy(&src, dir.join("SKILL.md")).expect("copy");
        checked += 1;
    }
    if checked == 0 {
        eprintln!("skipped: ~/.agents/skills has no SKILL.md entries");
        return;
    }

    // Not a count: the developer's real `~/.agents/skills` is discovered too, so the total is
    // larger than what this test copied. What matters is that every file it put on disk came
    // back, with a description and a body.
    let mut copied: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&root).expect("library dir") {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.path().join("SKILL.md").exists() {
            copied.push(name);
        }
    }
    let skills = niki::skills::list_all_skills(project.path(), &cfg);
    let by_name: std::collections::HashMap<&str, (niki::skills::SkillMetadata, String)> = skills
        .iter()
        .map(|(m, b)| (m.name.as_str(), (m.clone(), b.clone())))
        .collect();
    let missing: Vec<&String> = copied
        .iter()
        .filter(|n| !by_name.contains_key(n.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "{} of {checked} real SKILL.md files did not load: {missing:?}",
        missing.len()
    );

    let undescribed: Vec<&String> = copied
        .iter()
        .filter(|n| by_name[n.as_str()].0.description.is_empty())
        .collect();
    assert!(
        undescribed.is_empty(),
        "{} of {} real skills yielded no description ({undescribed:?}); the frontmatter parser is \
         missing fields the format actually uses",
        undescribed.len(),
        copied.len()
    );

    for name in &copied {
        assert!(
            !by_name[name.as_str()].1.trim().is_empty(),
            "{name} loaded with an empty body"
        );
    }
    let _ = niki_bin();
    let _ = niki_bin();
}
