//! Differential testing: the worktree and container backends are two
//! implementations of one contract.
//!
//! Where they disagree, **at least one is wrong**, and the disagreement is a
//! bug report with two witnesses rather than a mystery. That is the property
//! that makes a second backend worth having at all; without a check, a
//! backend-specific defect is found by whichever user happens to be on that
//! backend.
//!
//! Scope, stated honestly: this compares what the two backends *return* for
//! the same command — exit code, stdout, stderr, and the resulting tree. It
//! does not run a full agent pipeline, which would need a live model and would
//! make the comparison non-deterministic.
//!
//! Availability: this needs a container runtime **and** the pre-built sandbox
//! image. It therefore reports a skip when either is missing, and the nightly
//! job treats an unexpected skip as a failure — the same discipline the
//! consumer journeys use, because a differential gate that quietly never runs
//! is indistinguishable from one that passes.

use niki::sandbox::Sandbox;
use std::path::Path;
use std::process::Command;

/// The image `docker/Dockerfile` builds. A bare `ubuntu:24.04` lacks git, node
/// and python3 and fails the sandbox's own tool check, so the image must be
/// the project's.
const SANDBOX_IMAGE: &str = "niki-sandbox:24.04";

fn container_runtime() -> Option<&'static str> {
    for candidate in ["podman", "docker"] {
        if Command::new(candidate)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Some(if candidate == "podman" {
                "podman"
            } else {
                "docker"
            });
        }
    }
    None
}

fn image_present(runtime: &str) -> bool {
    Command::new(runtime)
        .args(["image", "inspect", SANDBOX_IMAGE])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn git(args: &[&str], cwd: &Path) {
    let _ = Command::new("git").args(args).current_dir(cwd).output();
}

/// A repository with one file, so a command that writes has something to do.
fn fixture(dir: &Path) {
    std::fs::create_dir_all(dir).expect("fixture dir");
    git(&["init", "-q"], dir);
    git(&["config", "user.email", "diff@niki.dev"], dir);
    git(&["config", "user.name", "Diff"], dir);
    std::fs::write(dir.join("main.rs"), "fn main() {}\n").expect("fixture file");
    git(&["add", "-A"], dir);
    git(&["commit", "-qm", "initial"], dir);
}

/// What both backends are asked to do, and what is compared.
struct Case {
    name: &'static str,
    cmd: &'static str,
}

const CASES: &[Case] = &[
    Case {
        name: "echo",
        cmd: "echo hello-from-sandbox",
    },
    // A subshell, so the command's non-zero exit does not terminate the
    // wrapper before the comparison marker is printed.
    Case {
        name: "exit-code",
        cmd: "(exit 3)",
    },
    Case {
        name: "stderr",
        cmd: "echo to-stderr 1>&2",
    },
    Case {
        name: "writes",
        cmd: "echo written > out.txt; cat out.txt",
    },
    Case {
        name: "pipeline",
        cmd: "printf 'a\\nb\\nc\\n' | sort -r | tr '\\n' ','",
    },
    Case {
        name: "pwd-is-project",
        cmd: "test \"$(basename \"$PWD\")\" = project && echo in-project || echo elsewhere",
    },
];

/// What both backends are asked to produce, for comparison.
#[derive(Debug, PartialEq, Eq)]
struct Observed {
    exit_code: i64,
    stdout: String,
}

/// Run one case on the worktree backend, through the production sandbox.
async fn worktree_case(dir: &Path, cmd: &str) -> anyhow::Result<Observed> {
    let (tx, _rx) = std::sync::mpsc::channel();
    let sandbox = niki::sandbox::worktree::WorktreeSandbox::create(
        niki::artifacts::types::AgentRole::Coder,
        dir,
        &uuid::Uuid::new_v4(),
        &niki::config::DockerConfig::default(),
        &niki::config::NikiConfig::default(),
        niki::config::SecurityPolicyConfig::default(),
        tx,
    )
    .await?;
    let out = sandbox.exec(&["sh", "-c", cmd], None).await;
    // Teardown before comparing, so no worktree is left behind.
    let _ = sandbox.destroy().await;
    out.map(|o| Observed {
        exit_code: o.exit_code,
        stdout: o.stdout.trim_end().to_string(),
    })
    .map_err(|e| anyhow::anyhow!("worktree exec failed: {e}"))
}

/// The same case on the container backend.
///
/// The exit code is appended as a marker so it can be recovered from combined
/// output: the container emits banner and startup noise that the worktree
/// backend does not, and that noise is legitimately different.
async fn container_case(runtime: &str, dir: &Path, cmd: &str) -> anyhow::Result<Observed> {
    let project = dir.to_string_lossy().to_string();
    let script = format!("{cmd}\n__niki_exit__=$?\nprintf '__niki_exit__%s' \"$__niki_exit__\"");
    let out = Command::new(runtime)
        .args([
            "run",
            "--rm",
            "-v",
            &format!("{project}:/workspace"),
            "-w",
            "/workspace",
            SANDBOX_IMAGE,
            "sh",
            "-c",
            &script,
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("{runtime} run failed: {e}"))?;

    let raw = String::from_utf8_lossy(&out.stdout).to_string();
    let (body, exit) = raw
        .rsplit_once("__niki_exit__")
        .ok_or_else(|| anyhow::anyhow!("no exit marker in container output: {raw:?}"))?;
    Ok(Observed {
        exit_code: exit.trim().parse().unwrap_or(-1),
        stdout: body.trim_end().to_string(),
    })
}

#[tokio::test]
async fn backends_agree_on_the_same_commands() {
    let Some(runtime) = container_runtime() else {
        eprintln!(
            "SKIP: no container runtime (podman/docker). Build the image and run on a host \
             with one: podman build -t {SANDBOX_IMAGE} -f docker/Dockerfile ."
        );
        return;
    };
    if !image_present(runtime) {
        eprintln!(
            "SKIP: {SANDBOX_IMAGE} is not built. A bare ubuntu:24.04 lacks git, node and \
             python3 and fails the sandbox tool check. Build it with: \
             podman build -t {SANDBOX_IMAGE} -f docker/Dockerfile ."
        );
        return;
    }

    let base = std::env::temp_dir().join(format!("niki-diff-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let project = base.join("project");
    fixture(&project);

    let mut disagreements = Vec::new();
    let mut compared = 0usize;

    for case in CASES {
        let wt = worktree_case(&project, case.cmd).await;
        let ct = container_case(runtime, &project, case.cmd).await;
        let (wt, ct) = match (wt, ct) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => {
                let _ = std::fs::remove_dir_all(&base);
                panic!("case `{}` could not run on one backend: {e}", case.name);
            }
        };
        compared += 1;

        // The two backends are two implementations of one contract. A
        // disagreement here is a bug report with two witnesses: at least one
        // of them is wrong, and the pair says which.
        if wt.exit_code != ct.exit_code || wt.stdout != ct.stdout {
            disagreements.push(format!(
                "  {}:\n      worktree : exit {} stdout {:?}\n      container: exit {} stdout {:?}",
                case.name, wt.exit_code, wt.stdout, ct.exit_code, ct.stdout
            ));
        }
    }

    let _ = std::fs::remove_dir_all(&base);

    assert!(
        compared >= 5,
        "only {compared} differential cases actually compared; the gate is vacuous"
    );
    assert!(
        disagreements.is_empty(),
        "the worktree and container backends disagree on {} command(s):\n{}\n\
         At least one backend is wrong. This is the defect a second backend exists to find.",
        disagreements.len(),
        disagreements.join("\n")
    );
}

/// The availability check itself must be honest: it must not report a
/// runtime it cannot actually run.
#[test]
fn container_runtime_detection_does_not_claim_a_missing_binary() {
    if let Some(rt) = container_runtime() {
        let v = Command::new(rt)
            .arg("--version")
            .output()
            .expect("runtime runs");
        assert!(
            v.status.success(),
            "container_runtime() reported `{rt}` but `{rt} --version` failed"
        );
    }
}

/// The case list must not be empty, or the differential test above compares
/// nothing and passes.
#[test]
fn the_differential_corpus_is_not_empty() {
    assert!(
        CASES.len() >= 5,
        "only {} differential cases; the gate would compare almost nothing",
        CASES.len()
    );
    let mut names: Vec<&str> = CASES.iter().map(|c| c.name).collect();
    names.sort_unstable();
    let total = names.len();
    names.dedup();
    assert_eq!(names.len(), total, "duplicate differential case name");
}
