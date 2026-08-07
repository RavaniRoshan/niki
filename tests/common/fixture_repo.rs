use std::path::Path;
use tempfile::TempDir;

pub struct FixtureRepo {
    pub dir: TempDir,
}

impl FixtureRepo {
    pub fn path(&self) -> &Path {
        self.dir.path()
    }
}

impl std::ops::Deref for FixtureRepo {
    type Target = Path;
    fn deref(&self) -> &Self::Target {
        self.dir.path()
    }
}

fn git(repo: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git command should succeed");
    assert!(
        out.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
}

pub fn create_fixture_repo() -> FixtureRepo {
    let dir = TempDir::new().expect("create temp dir");
    let path = dir.path();

    git(path, &["init", "-q"]);
    git(path, &["config", "user.email", "test@niki.local"]);
    git(path, &["config", "user.name", "niki-test"]);

    // A basic source file with an off-by-one bug for the coder to fix.
    let src_dir = path.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(
        src_dir.join("list.rs"),
        "pub fn paginate(items: &[u32], start: usize, size: usize) -> &[u32] {\n    let end = start + size - 1;\n    &items[start..end]\n}\n",
    )
    .unwrap();
    std::fs::write(
        src_dir.join("main.rs"),
        "mod list;\n\nfn main() {\n    println!(\"Hello\");\n}\n",
    )
    .unwrap();
    std::fs::write(
        path.join("README.md"),
        "# Test Project\n\nA fixture for hermetic tests.\n",
    )
    .unwrap();

    git(path, &["add", "-A"]);
    git(path, &["commit", "-q", "-m", "initial commit"]);

    FixtureRepo { dir }
}

pub struct FixtureRepoBuilder {
    source_files: Vec<(String, String)>,
    commit_msg: String,
}

impl FixtureRepoBuilder {
    pub fn new() -> Self {
        FixtureRepoBuilder {
            source_files: vec![],
            commit_msg: "initial commit".to_string(),
        }
    }

    pub fn add_file(mut self, path: &str, content: &str) -> Self {
        self.source_files
            .push((path.to_string(), content.to_string()));
        self
    }

    pub fn commit_message(mut self, msg: &str) -> Self {
        self.commit_msg = msg.to_string();
        self
    }

    pub fn build(self) -> FixtureRepo {
        let dir = TempDir::new().expect("create temp dir");
        let path = dir.path();

        git(path, &["init", "-q"]);
        git(path, &["config", "user.email", "test@niki.local"]);
        git(path, &["config", "user.name", "niki-test"]);

        for (rel_path, content) in &self.source_files {
            let full = path.join(rel_path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(full, content).unwrap();
        }

        git(path, &["add", "-A"]);
        git(path, &["commit", "-q", "-m", &self.commit_msg]);

        FixtureRepo { dir }
    }
}
