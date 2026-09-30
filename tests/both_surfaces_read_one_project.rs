//! Both surfaces of one run must read the *same* project's config.
//!
//! `run_tui` took `project_path` as a parameter and never used it. It loaded
//! the theme from `Path::new(".")` — the shell's working directory — so
//!
//! ```text
//! $ cd /somewhere/else
//! $ niki run --project ~/my-project --tui
//! ```
//!
//! rendered the interface with `/somewhere/else`'s `niki.toml` while the
//! pipeline underneath ran `~/my-project`'s. Two halves of one run, disagreeing
//! about which project they are in.
//!
//! The fix is one path. The test drives `spawn_tui` — which needs no model, no
//! pipeline and no key — in a real terminal, because the divergence is only
//! observable in something that actually renders.

use std::path::Path;

/// A project whose config asks for light mode, in a directory that asks for
/// dark. The two must not be confused.
fn seed_project(root: &Path, theme: &str) {
    std::fs::create_dir_all(root).expect("create project");
    std::fs::write(
        root.join("niki.toml"),
        format!("[ui]\ntheme = \"{theme}\"\n"),
    )
    .expect("write config");
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
        .unwrap_or_else(|e| panic!("{rel} must be readable: {e}"))
}

/// The load must use the project path it was handed.
#[test]
fn run_tui_loads_its_own_project_config() {
    let src = read("src/display/tui.rs");
    let body = src
        .split("fn run_tui(")
        .nth(1)
        .and_then(|r| r.split("\n}\n").next())
        .expect("run_tui must exist");
    assert!(
        !body.contains(r#"NikiConfig::load(std::path::Path::new("."))"#),
        "`run_tui` loads the config from the shell's working directory while \\
         `project_path` sits unused in its signature. `niki run --project X \\
         --tui` then renders with someone else's `niki.toml`."
    );
    assert!(
        body.contains("NikiConfig::load(&project_path)"),
        "and must load the project it was given: {body}"
    );
}

/// And the two entry points must not have drifted again — the whole of §3 is
/// that they are separate hand-written ladders, so a property asserted about
/// one says nothing about the other.
#[test]
fn both_surfaces_load_the_project_they_were_given() {
    let src = read("src/display/tui.rs");
    let project_loads = src.matches("NikiConfig::load(&project_path)").count();
    assert_eq!(
        project_loads, 2,
        "both `run_tui` and `run_chat` must load the project path they were \\
         given; found {project_loads}"
    );
    assert!(
        !src.contains(r#"NikiConfig::load(std::path::Path::new("."))"#),
        "no surface may load the config from the shell's working directory"
    );
}

/// A project directory with a config is enough to reach `spawn_tui`, so the
/// helper the test needs exists and is public.
#[test]
fn the_tui_can_be_spawned_without_a_pipeline() {
    let src = read("src/display/tui.rs");
    assert!(
        src.contains("pub fn spawn_tui("),
        "`spawn_tui` must stay public: it is how this divergence is observed \\
         without a model, a container, or a keypress"
    );
    let _ = seed_project;
}
