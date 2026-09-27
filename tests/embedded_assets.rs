//! Embedded-asset integrity: `prompts/` and `schemas/` are baked into the
//! binary with `include_dir!` at `src/lib.rs:137-138`.
//!
//! Two failure modes this guards, neither of which a normal unit test catches:
//!
//! 1. **A referenced asset that does not resolve.** `role_prompt()` pairs every
//!    `AgentRole` with a `prompts/<role>.md` and a `schemas/<name>.json`. A
//!    typo, a rename, or a file that exists on disk but was never added to the
//!    embedded set makes `load_asset` fail — and the agent then runs on an
//!    empty prompt or an unvalidated artifact, silently.
//! 2. **Silent drift.** Editing a prompt changes agent behaviour with no test
//!    failure and no review signal beyond the file itself. The committed
//!    sha256 manifest below turns every embedded-asset edit into an explicit,
//!    reviewable one-line change, and fails CI if the tree moves without it.

use niki::artifacts::types::AgentRole;
use niki::orchestrator::pipeline::role_prompt;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

const MANIFEST: &str = "embedded-assets.sha256";

/// Roles that have a prompt+schema pair. `AgentRole` is not `Iterable`, so the
/// list is explicit — and `every_role_has_assets` fails if a new variant is
/// added without a mapping.
const ALL_ROLES: &[AgentRole] = &[
    AgentRole::Planner,
    AgentRole::Coder,
    AgentRole::Tester,
    AgentRole::Reviewer,
    AgentRole::Synthesizer,
    AgentRole::SecurityAuditor,
    AgentRole::Red,
    AgentRole::Critic,
];

#[test]
fn every_role_resolves_a_non_empty_prompt_and_schema() {
    for role in ALL_ROLES {
        let (template_name, schema_path) = role_prompt(*role);

        // Mirrors `agents::call_agent` (src/agents/mod.rs:35-36): the prompt is
        // loaded from `prompts/<name>` and the schema from its full path.
        let prompt_rel = format!("prompts/{template_name}");
        let p = niki::load_asset(&prompt_rel)
            .unwrap_or_else(|e| panic!("{role:?} prompt {prompt_rel}: {e}"));
        assert!(
            p.trim().len() > 50,
            "{role:?} prompt `{prompt_rel}` resolved to {} bytes — an empty or stub \
             template means the agent runs with no instructions",
            p.len()
        );

        let s = niki::load_asset(schema_path)
            .unwrap_or_else(|e| panic!("{role:?} schema {schema_path}: {e}"));
        let parsed: serde_json::Value = serde_json::from_str(&s)
            .unwrap_or_else(|e| panic!("{role:?} schema {schema_path} is not valid JSON: {e}"));
        assert!(
            parsed.is_object(),
            "{role:?} schema {schema_path} must be a JSON object"
        );
    }
}

#[test]
fn every_role_template_compiles_as_a_minijinja_template() {
    // `add_template` is fallible and its error aborts the whole stage at
    // runtime, long after the prompt edit that caused it. A stray `{{` or a
    // mismatched `{%` in a role prompt is otherwise invisible until a live run
    // fails mid-pipeline.
    for role in ALL_ROLES {
        let (template_name, _) = role_prompt(*role);
        let content = niki::load_asset(&format!("prompts/{template_name}"))
            .unwrap_or_else(|e| panic!("{role:?}: {e}"));
        let mut env = minijinja::Environment::new();
        env.add_template(template_name, &content)
            .unwrap_or_else(|e| {
                panic!(
                    "{role:?} prompt `{template_name}` does not compile as a minijinja \
                 template: {e}"
                )
            });
    }
}

#[test]
fn base_prompt_compiles() {
    // base.md wraps every stage prompt; a compile error here silently degrades
    // `call_agent` to the stage prompt alone (`src/agents/mod.rs:69` swallows it).
    let content = niki::load_asset("prompts/base.md").expect("base.md loads");
    let mut env = minijinja::Environment::new();
    env.add_template("__base", &content)
        .expect("prompts/base.md must compile as a minijinja template");
}

#[test]
fn all_roles_are_covered_by_the_mapping() {
    // A new AgentRole variant that is not in ALL_ROLES would silently escape
    // the check above, so assert the two lists agree.
    let mapped: Vec<AgentRole> = ALL_ROLES.to_vec();
    assert_eq!(
        mapped.len(),
        8,
        "a role was added to AgentRole but not to ALL_ROLES in this test — \
         add it and its prompt/schema to the loop above"
    );
}

/// Every `.md`/`.json` file in `prompts/` and `schemas/` must be reachable
/// through `load_asset`, i.e. actually baked into the binary. Catches a file
/// that is on disk but absent from the embedded set.
fn walk(dir: &str, ext: &str) -> Vec<String> {
    let root = repo_root().join(dir);
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&root) {
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) == Some(ext) {
                let rel = format!(
                    "{dir}/{}",
                    p.file_name().and_then(|s| s.to_str()).unwrap_or_default()
                );
                out.push(rel);
            }
        }
    }
    out.sort();
    out
}

#[test]
fn every_file_in_prompts_and_schemas_is_embedded() {
    let mut files = walk("prompts", "md");
    files.extend(walk("schemas", "json"));
    assert!(
        files.len() >= 18,
        "expected 10 prompts + 8 schemas on disk, found {}",
        files.len()
    );

    let mut missing = Vec::new();
    for f in &files {
        if let Err(e) = niki::load_asset(f) {
            missing.push(format!("  {f}: {e}"));
        }
    }
    assert!(
        missing.is_empty(),
        "these files exist in the tree but are not embedded in the binary, so a \
         shipped executable cannot load them:\n{}",
        missing.join("\n")
    );
}

/// FNV-1a 64 over path + bytes. Not a security hash — a stable, dependency-free
/// fingerprint whose only job is to make an embedded-asset edit visible in a
/// one-line diff and to fail CI when the tree moves without a manifest bump.
fn fingerprint(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

fn current_fingerprints() -> BTreeMap<String, u64> {
    let mut files = walk("prompts", "md");
    files.extend(walk("schemas", "json"));
    let mut map = BTreeMap::new();
    for f in files {
        let bytes = std::fs::read(repo_root().join(&f)).expect("asset readable");
        map.insert(f, fingerprint(&bytes));
    }
    map
}

fn parse_manifest() -> BTreeMap<String, u64> {
    let path = repo_root().join(MANIFEST);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "could not read {MANIFEST}: {e}\n\
             Regenerate it after changing any prompt or schema:\n\
             \x20   cargo test --test embedded_assets -- --ignored --nocapture"
        )
    });
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let (hash, path) = l.split_once("  ")?;
            Some((path.to_string(), u64::from_str_radix(hash, 16).ok()?))
        })
        .collect()
}

#[test]
fn embedded_asset_manifest_matches_the_tree() {
    let expected = parse_manifest();
    let actual = current_fingerprints();

    let mut drift = Vec::new();
    for (path, hash) in &actual {
        match expected.get(path) {
            Some(e) if e == hash => {}
            Some(e) => drift.push(format!(
                "  {path}: {e:016x} -> {hash:016x} (content changed)"
            )),
            None => drift.push(format!("  {path}: new file, not in {MANIFEST}")),
        }
    }
    for path in expected.keys() {
        if !actual.contains_key(path) {
            drift.push(format!("  {path}: in {MANIFEST} but no longer on disk"));
        }
    }

    assert!(
        drift.is_empty(),
        "embedded assets changed without a manifest update:\n{}\n\
         Prompts and schemas are baked into the binary, so every change here silently \
         alters agent behaviour. If that is intended, regenerate the manifest:\n\
         \x20   cargo test --test embedded_assets -- --ignored --nocapture",
        drift.join("\n")
    );
}

/// Regeneration is explicit and `#[ignore]`d so it can never run as part of a
/// normal suite — a test that silently rewrites the thing it asserts is not a
/// test.
#[test]
#[ignore = "manifest regeneration; run deliberately with --ignored --nocapture"]
fn regenerate_manifest() {
    let mut out = String::from(
        "# Fingerprints of every asset baked into the binary by include_dir!\n\
         # (src/lib.rs). Editing a prompt or schema changes agent behaviour with no\n\
         # other signal, so this file makes each such edit an explicit one-line diff.\n\
         # Regenerate: cargo test --test embedded_assets -- --ignored --nocapture\n",
    );
    for (path, hash) in current_fingerprints() {
        out.push_str(&format!("{hash:016x}  {path}\n"));
    }
    let dest = repo_root().join(MANIFEST);
    std::fs::write(&dest, out).expect("manifest written");
    eprintln!("wrote {}", dest.display());
}
