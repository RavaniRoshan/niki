//! Turning a finished pipeline into a reviewable branch.
//!
//! This was 281 lines living inside `cli/run.rs::run_inner`, and it is the
//! whole product: write the artifacts, apply the red-suite gate, replay the
//! sandbox diff onto the host working tree, refuse conflict markers, cut the
//! `niki/<id>` branch, prove the run was hermetic, and write the report.
//!
//! It lived there because that is where it was written, not because it belongs
//! to the CLI. `create_branch_and_commit`, `apply_diff_to_working_tree` and
//! `generate_report` each had exactly one call site — all in this file — which
//! meant every other entry point into the pipeline (`niki acp`, `niki goal`)
//! ran the four agents, destroyed the sandbox on the way out, and handed back
//! nothing: the Coder's diff was deleted from disk and no branch existed. The
//! deliverable lived one layer above the orchestrator, so the orchestrator
//! could not deliver.
//!
//! Nothing in the moved body was rewritten. The CLI's locals are reconstructed
//! at the top of `deliver()` so the extracted lines read as they did in
//! `run_inner` — a 281-line refactor of the most safety-critical code in the
//! product should be a move, not an edit.

use anyhow::Result;
use std::path::Path;

use super::pipeline::{PipelineResult, Task};
use crate::artifacts::types::AgentRole;
use crate::config::NikiConfig;
use crate::display::agent_stream::AgenticDisplay;
use crate::safety::RepoSnapshot;

/// What delivery needs from its caller.
pub struct DeliverInput<'a> {
    pub task: &'a Task,
    pub config: &'a NikiConfig,
    /// `&mut` because the hermetic proof is attached to the result.
    pub result: &'a mut PipelineResult,
    pub project_dir: &'a Path,
    pub task_dir: &'a Path,
    pub branch_name: String,
    /// Pre-run committed-state fingerprint, for the hermetic proof.
    pub pre_snapshot: Option<&'a RepoSnapshot>,
    /// True when the sandbox writes through a bind mount, so the diff is
    /// already on the host and must not be replayed.
    pub uses_docker: bool,
    /// Plan mode: write `plan.md`, create no branch.
    pub dry_run: bool,
    /// Create the branch over a failing suite, recorded as forced.
    pub force: bool,
    /// Keep stdout parseable: progress goes to stderr.
    pub json_mode: bool,
    /// Announced with the branch name when the caller owns a display.
    pub display: Option<&'a mut AgenticDisplay>,
}

/// What delivery decided. The caller records it; delivery does not.
pub struct Delivered {
    /// The ground truth for "this run produced its deliverable".
    pub branch_created: bool,
    /// Why no branch was cut, in plain language, for the report and the user.
    pub block_note: Option<String>,
    /// A branch created over a failing gate. Explicitly not verified.
    pub forced: bool,
    pub error: Option<String>,
}

/// Render the Planner's spec as a human-readable `plan.md` for the plan mode
/// (`niki plan` / `--dry-run`). Machine-readable truth stays in
/// `artifacts/planner.json`; this file is the approval surface.
fn write_plan_md(task_dir: &std::path::Path, task: &Task, result: &PipelineResult) {
    let Some(planner_json) = result
        .artifacts
        .iter()
        .find(|(r, _)| *r == AgentRole::Planner)
        .map(|(_, j)| j.as_str())
    else {
        return;
    };
    let spec: crate::artifacts::types::TaskSpec = match serde_json::from_str(planner_json) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "Warning: could not parse planner artifact for plan.md: {}",
                e
            );
            return;
        }
    };
    let mut out = format!(
        "# Plan — {}\n\n> Produced by `niki plan` (task `{}`). Review, edit the approach if needed, then execute with `niki run --plan {}`.\n\n## Summary\n\n{}\n\n## Approach\n\n{}\n\n",
        task.description,
        task.id,
        &task.id.to_string()[..8],
        spec.summary,
        spec.approach,
    );
    out.push_str("## Files to touch\n\n");
    for f in &spec.files_to_modify {
        out.push_str(&format!(
            "- `{}` ({:?}): {}\n",
            f.path, f.action, f.description
        ));
    }
    out.push_str("\n## Acceptance criteria\n\n");
    for c in &spec.acceptance_criteria {
        out.push_str(&format!("- [ ] {}\n", c));
    }
    if !spec.constraints.is_empty() {
        out.push_str("\n## Constraints\n\n");
        for c in &spec.constraints {
            out.push_str(&format!("- {}\n", c));
        }
    }
    if let Some(u) = &spec.uncertainties {
        if !u.is_empty() {
            out.push_str("\n## Open questions (planner uncertainties)\n\n");
            for q in u {
                out.push_str(&format!("- {}\n", q));
            }
        }
    }
    out.push_str(&format!(
        "\n## Topology\n\n{}\n\n## Cost of planning\n\n",
        result.topology_reason
    ));
    for m in &result.metrics {
        out.push_str(&format!(
            "- {:?} {} ({}): {} in / {} out tok, ${:.4}\n",
            m.role, m.model, m.provider, m.input_tokens, m.output_tokens, m.cost_usd
        ));
    }
    if let Err(e) = crate::util::write_restricted(&task_dir.join("plan.md"), out) {
        eprintln!("Warning: could not write plan.md: {}", e);
    }
}

pub fn role_filename(role: AgentRole) -> &'static str {
    match role {
        AgentRole::Planner => "planner",
        AgentRole::Coder => "coder",
        AgentRole::Tester => "tester",
        AgentRole::Reviewer => "reviewer",
        AgentRole::Synthesizer => "synthesizer",
        AgentRole::SecurityAuditor => "security_auditor",
        AgentRole::Red => "red",
        AgentRole::Critic => "critic",
    }
}

/// The two flags the moved body reads off `args`.
struct DeliverFlags {
    dry_run: bool,
    force: bool,
}

pub fn deliver(inp: DeliverInput<'_>) -> Result<Delivered> {
    let DeliverInput {
        task,
        config,
        result,
        project_dir,
        task_dir,
        branch_name,
        pre_snapshot,
        uses_docker,
        dry_run,
        force,
        json_mode,
        display,
    } = inp;
    let args = DeliverFlags { dry_run, force };
    // `as_deref`, not a move: the same display is needed again at the end to
    // announce a blocked branch.
    if let Some(d) = display.as_deref() {
        d.set_branch_name(&branch_name);
    }
    // Send branch name to TUI for status line display

    // Save raw agent artifacts.
    let artifacts_dir = task_dir.join("artifacts");
    if let Err(e) = std::fs::create_dir_all(&artifacts_dir) {
        eprintln!("Warning: could not create artifacts dir: {}", e);
    } else {
        // Repeated pushes for one role (revision rounds, coder patch-repair
        // attempts) each get their own file — coder.json, coder-2.json, … —
        // so the audit trail never silently drops the failed attempts.
        use std::collections::HashMap;
        let mut seen: HashMap<String, usize> = HashMap::new();
        for (role, json) in &result.artifacts {
            let base = role_filename(*role).to_string();
            let n = seen.entry(base.clone()).or_insert(0);
            *n += 1;
            let name = if *n == 1 {
                format!("{base}.json")
            } else {
                format!("{base}-{n}.json")
            };
            let path = artifacts_dir.join(name);
            if let Err(e) = crate::util::write_restricted(&path, json) {
                eprintln!("Warning: could not save artifact {:?}: {}", role, e);
            }
        }
        if let Some(te) = &result.test_execution {
            let path = artifacts_dir.join("test_execution.json");
            if let Err(e) = crate::util::write_restricted(&path, serde_json::to_string_pretty(te)?)
            {
                eprintln!("Warning: could not save test_execution artifact: {}", e);
            }
        }
    }

    // Plan mode (`niki plan` / `--dry-run`): persist a human-readable plan and
    // point at the approval command. The machine-readable spec already lives at
    // `artifacts/planner.json`; `plan.md` is the review surface.
    if args.dry_run {
        write_plan_md(task_dir, task, result);
        // Progress goes to stderr in JSON mode so stdout stays parseable.
        if json_mode {
            eprintln!(
                "Plan written to {}/plan.md — review it, then execute with: niki run --plan {} --project {}",
                task_dir.display(),
                &task.id.to_string()[..8],
                project_dir.display(),
            );
        } else {
            println!(
                "\nPlan written to {}/plan.md — review it, then execute with:\n  niki run --plan {} --project {}",
                task_dir.display(),
                &task.id.to_string()[..8],
                project_dir.display(),
            );
        }
    }

    // Generate the static HTML dashboard (diff viewer + annotations).
    {
        let find_artifact = |role: AgentRole| -> Option<String> {
            result
                .artifacts
                .iter()
                .find(|(r, _)| *r == role)
                .map(|(_, j)| j.clone())
        };
        let review_json = find_artifact(AgentRole::Reviewer);
        let security_json = find_artifact(AgentRole::SecurityAuditor);

        let total_in: u32 = result.metrics.iter().map(|m| m.input_tokens).sum();
        let total_out: u32 = result.metrics.iter().map(|m| m.output_tokens).sum();
        let total_cost: f64 = result.metrics.iter().map(|m| m.cost_usd).sum();
        let total_ms: u64 = result.metrics.iter().map(|m| m.latency_ms).sum();
        if config.general.spend_cap_usd > 0.0 && total_cost > config.general.spend_cap_usd {
            eprintln!(
                "\nwarning: spend cap exceeded — estimated ${:.4} > cap ${:.2}. \
                 Lower the task scope or raise [general] spend_cap_usd.",
                total_cost, config.general.spend_cap_usd
            );
        }
        let metrics_rows = vec![
            ("Agents run".to_string(), result.metrics.len().to_string()),
            ("Input tokens".to_string(), total_in.to_string()),
            ("Output tokens".to_string(), total_out.to_string()),
            (
                "Latency".to_string(),
                format!("{:.1}s", total_ms as f64 / 1000.0),
            ),
            (
                "Est. cost".to_string(),
                if total_cost > 0.0 {
                    format!("${:.4}", total_cost)
                } else {
                    "n/a".to_string()
                },
            ),
        ];

        let input = crate::output::dashboard::DashboardInput {
            task_id: &task.id.to_string(),
            description: &task.description,
            verdict: &format!("{:?}", result.verdict),
            revision_rounds: result.revision_rounds,
            final_diff: &result.final_diff,
            review_json: review_json.as_deref(),
            security_json: security_json.as_deref(),
            metrics_rows,
        };
        if let Err(e) = crate::output::dashboard::write_dashboard(task_dir, &input) {
            eprintln!("Warning: could not generate dashboard: {}", e);
        }
    }

    // changes.patch is written exactly once, by `generate_report` alongside
    // report.md (Phase 5.6 single-writer rule).

    // Red-suite gate (goal-a3f9c2, Phase 2): a failing executed suite — or a
    // failing mutation gate — blocks the branch. The evidence (patch, report,
    // test output) is still written so the failure is inspectable, but no
    // `niki/<id>` branch is created and the task is recorded as Failed.
    // `--force` overrides with the override itself recorded; a forced branch
    // is explicitly not a verified branch.
    let suite_failed = result
        .test_execution
        .as_ref()
        .is_some_and(|te| !te.passed || te.mutation.as_ref().is_some_and(|m| !m.passed));
    let mut branch_block_note: Option<String> = None;
    if suite_failed && !args.force {
        let what = match result.test_execution.as_ref() {
            Some(te) if !te.passed => {
                format!("test suite `{}` failed (exit {})", te.command, te.exit_code)
            }
            Some(te) => format!(
                "mutation gate `{}` failed (exit {})",
                te.mutation
                    .as_ref()
                    .map(|m| m.command.as_str())
                    .unwrap_or("?"),
                te.mutation.as_ref().map(|m| m.exit_code).unwrap_or(-1),
            ),
            None => "verification failed".to_string(),
        };
        branch_block_note = Some(format!(
            "Branch blocked: {}. Re-run with `--force` to create the branch anyway (recorded as forced, not verified).",
            what
        ));
    }
    let forced_branch = suite_failed && args.force;

    // For the worktree backend the change still lives inside the sandbox copy (a
    // separate git worktree), so `working_tree_diff` on the host would be empty.
    // Apply the sandbox's diff to the host working tree first; the Docker backend
    // already wrote through the bind mount and skips this step.
    if branch_block_note.is_none()
        && !uses_docker
        && !result.final_diff.trim().is_empty()
        && let Err(e) =
            crate::output::git::apply_diff_to_working_tree(project_dir, &result.final_diff)
    {
        eprintln!("Warning: could not apply sandbox diff to host: {}", e);
    }

    // Create the git branch + commit (no-op when there is no diff; skipped
    // entirely when the red-suite gate blocked the branch, and in dry-run /
    // plan mode where a branch — even an empty ref — would misrepresent a
    // proposal as a result).
    // Phase 5.6: unresolved conflict markers (e.g. from a `--3way` fallback)
    // block the branch like a failed suite — abort instead of committing a
    // conflicted tree. Recorded in task.json as Failed.
    if branch_block_note.is_none() && !result.final_diff.trim().is_empty() {
        if let Err(e) =
            crate::output::git::ensure_no_conflict_markers(project_dir, &result.final_diff)
        {
            branch_block_note = Some(format!("Branch blocked: {e}."));
        }
    }
    // `branch_created` is the ground truth for the recorded status. It stays
    // false for a dry run, an empty diff, a blocked branch, and a failed
    // `create_branch_and_commit` — all of which previously fell through to
    // `Completed { branch: Some(...) }` and reported a branch that did not exist.
    let mut branch_created = false;
    let mut branch_creation_error: Option<String> = None;
    if branch_block_note.is_none() && !args.dry_run {
        if !result.final_diff.trim().is_empty() {
            match crate::output::git::create_branch_and_commit(
                project_dir,
                &branch_name,
                &result.final_diff,
                &task.id.to_string(),
            ) {
                // `false` means the diff carried no committable content, so no
                // ref was created. That is not a successful run.
                Ok(true) => branch_created = true,
                Ok(false) => {
                    branch_creation_error = Some(
                        "Branch not created: the diff carried no committable file changes."
                            .to_string(),
                    );
                }
                Err(e) => {
                    // Not a warning: the run's whole deliverable is this branch.
                    // Swallowing it here reported success for a run that changed
                    // nothing reviewable.
                    branch_creation_error = Some(format!("Branch creation failed: {e}."));
                    eprintln!(
                        "Error: {}",
                        branch_creation_error.as_deref().unwrap_or_default()
                    );
                }
            }
        }
    }

    // Hermetic safety proof (BUILD_PLAN 1.1): with the branch now committed,
    // verify the committed repo state is unchanged except for that one branch.
    // Emit `safety_proof.json` next to the report and attach it to the result.
    // Skip when there was no diff (no branch was created), so a no-op run isn't
    // misreported as NON-HERMETIC — and skip when the red-suite gate blocked
    // the branch, since `prove()` in strict mode would abort a correctly
    // blocked run for the missing branch.
    if branch_block_note.is_none()
        && !result.final_diff.trim().is_empty()
        && let Some(pre) = &pre_snapshot
    {
        // Enforce the hermetic guarantee (research report S9). Previously this used
        // strict=false and only printed a warning on a committed-state breach, so a
        // non-hermetic run could silently complete. strict=true makes prove() return
        // an Err when existing branches are repointed, history is rewritten, or the
        // new branch is missing — which we propagate to abort the run rather than
        // present a completed task. The working-tree cleanliness flags remain
        // informational (NIKI intentionally applies the diff to the host working tree).
        let proof =
            crate::safety::prove(pre, project_dir, &branch_name, &task.id.to_string(), true)?;
        if let Err(e) = crate::util::write_restricted(
            &task_dir.join("safety_proof.json"),
            serde_json::to_string_pretty(&proof)?,
        ) {
            eprintln!("Warning: could not write safety_proof.json: {}", e);
        }
        result.safety_proof = Some(proof);
    }

    // Generate the markdown report (now includes the hermetic safety proof).
    if let Err(e) = crate::output::report::generate_report(
        task,
        config,
        result,
        if branch_block_note.is_some() {
            None
        } else {
            Some(branch_name.as_str())
        },
    ) {
        eprintln!("Warning: could not generate report: {}", e);
    }
    // Record a red-suite block / force override directly in the report so the
    // audit trail states the branch decision in plain language.
    if branch_block_note.is_some() || forced_branch {
        let notice = match (&branch_block_note, forced_branch) {
            (Some(note), _) => format!("\n## Branch decision\n\n{}\n", note),
            (None, true) => "## Branch decision\n\nBranch created with `--force` over a failing suite/mutation gate. This branch is explicitly NOT verified.\n".to_string(),
            _ => String::new(),
        };
        if !notice.is_empty() {
            use std::fmt::Write as _;
            let path = task_dir.join("report.md");
            let mut existing = std::fs::read_to_string(&path).unwrap_or_default();
            let _ = write!(existing, "{notice}");
            if let Err(e) = crate::util::write_restricted(&path, existing) {
                eprintln!("Warning: could not append branch decision to report: {}", e);
            }
        }
    }

    // Say the blocking reason to the surface while it is still on screen. It is
    // also returned in `Delivered` and printed by every non-TUI caller; this is
    // the copy that survives `LeaveAlternateScreen`.
    if let (Some(note), Some(d)) = (&branch_block_note, display.as_deref()) {
        d.notice(note.clone(), false);
    }

    Ok(Delivered {
        branch_created,
        block_note: branch_block_note,
        forced: forced_branch,
        error: branch_creation_error,
    })
}

/// Render a salvaged `CodeDiff` as a unified diff, without touching the tree.
///
/// **Why this exists.** A failed run is salvaged: the Coder's `CodeDiff` is
/// written to `artifacts/coder.json` and a `SALVAGED.md` says what survived.
/// That is the change *as data* — search/replace blocks and a file list — and
/// the salvage deliberately does not apply it, because quietly writing an
/// unreviewed diff into someone's repository is a larger semantic change than
/// a hardening pass should make on its own.
///
/// But the consequence was that a salvaged run gave a human the one thing they
/// cannot easily read. Reviewing unreviewed work means seeing a diff; JSON is
/// not a diff, and `changes.patch` — the file the TUI's Run page, the
/// completion screen and `niki report` all advertise — did not exist for a
/// failed run at all.
///
/// So: render it, in a temporary copy, and hand the user a patch they can read
/// *and* apply if they choose. The working tree is never modified, no branch is
/// cut, and the patch's own header says it has not been reviewed — because
/// this is a rendering, not a delivery.
///
/// Returns an error rather than a partial patch when an edit cannot be
/// applied. A patch that is missing a hunk reads as "that is all the change
/// was", which is the one thing a salvaged artifact must never do.
pub fn render_salvaged_patch(
    project_dir: &std::path::Path,
    code_diff: &crate::artifacts::types::CodeDiff,
) -> Result<String> {
    use crate::artifacts::types::FileAction;

    // Only files NIKI is allowed to publish are rendered. A model that
    // reported `.env` or a traversal escape gets no patch hunk for it, for the
    // same reason `is_publishable_path` refuses it everywhere else.
    let files: Vec<&crate::artifacts::types::ChangedFile> = code_diff
        .files_changed
        .iter()
        .filter(|f| crate::output::git::is_publishable_path(&f.path))
        .collect();
    if files.is_empty() {
        anyhow::bail!(
            "no publishable file in the salvaged diff — the Coder reported {} \
             file(s) and none may be written",
            code_diff.files_changed.len()
        );
    }

    // A scratch copy of the tree, so the render never writes to the user's.
    //
    // Under the system temp directory rather than `tempfile`, which is a
    // dev-dependency and so absent from the library. Named with the process id
    // and a nanosecond stamp, for the reason `patch_temp_path` gives: a path
    // shared by every run on the machine is a collision waiting for two
    // concurrent renders.
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let scratch =
        std::env::temp_dir().join(format!("niki-salvage.{}.{unique}", std::process::id()));
    let before = scratch.join("before");
    let after = scratch.join("after");
    for side in [&before, &after] {
        std::fs::create_dir_all(side)
            .map_err(|e| anyhow::anyhow!("could not prepare {}: {e}", side.display()))?;
    }
    // Per file, per block, with the same matching the sandbox uses: a block
    // with a `file` target goes to that file, and a block without one goes to
    // whichever file it actually matches. Applying every block to every file —
    // the obvious reading of "apply the edits" — fails the moment a diff
    // touches two files, because file A does not contain file B's search text.
    let mut unmatched: Vec<String> = Vec::new();
    let mut used: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for f in &files {
        let rel = std::path::Path::new(&f.path);
        if f.action == FileAction::Delete {
            // A delete's "before" is the file and its "after" is nothing.
            copy_if_present(project_dir, rel, &before.join(rel))?;
            continue;
        }
        let original = std::fs::read_to_string(project_dir.join(rel)).unwrap_or_default();
        // A Coder path is `src/lib.rs`, not `lib.rs`, so the scratch tree needs
        // the same directories or the write below fails on a missing parent.
        if let Some(parent) = before.join(rel).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("could not stage {}: {e}", rel.display()))?;
        }
        std::fs::write(before.join(rel), &original)
            .map_err(|e| anyhow::anyhow!("could not stage {}: {e}", rel.display()))?;

        // A create has no original, so every block is a candidate for it.
        let mut content = String::new();
        let mut changed = false;
        for (i, e) in code_diff.edits.iter().enumerate() {
            if content.is_empty() && !original.is_empty() {
                content = original.clone();
            }
            match crate::sandbox::edit_format::apply_single_edit_block(
                &content, &e.search, &e.replace,
            )
            .map_err(|err| anyhow::anyhow!("could not apply the Coder's edits: {err}"))?
            {
                Some(next) => {
                    content = next;
                    used.insert(i);
                    changed = true;
                }
                None => {
                    if f.action == FileAction::Create {
                        unmatched.push(format!("{}: {}", f.path, summarise_search(&e.search)));
                    }
                }
            }
        }
        if !changed && f.action == FileAction::Create {
            // Nothing applied to a file the Coder said it created: it is empty
            // and no block fit, so there is no change to show.
            continue;
        }
        if let Some(parent) = after.join(rel).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(after.join(rel), &content)
            .map_err(|e| anyhow::anyhow!("could not stage {}: {e}", rel.display()))?;
    }
    // Every block must have landed somewhere. One that did not means the
    // rendered patch would be missing a hunk — and a patch that is quietly
    // short reads as "that is all the change was", which is the one thing a
    // salvaged artifact must never do.
    for (i, e) in code_diff.edits.iter().enumerate() {
        if !used.contains(&i) {
            unmatched.push(format!("(no file) {}", summarise_search(&e.search)));
        }
    }
    if !unmatched.is_empty() && unmatched.len() < code_diff.edits.len() {
        // Some blocks landed; report the rest rather than refusing outright, so
        // the user can see what did survive and read the patch for it.
        tracing::warn!(
            target: "niki::pipeline",
            blocks = unmatched.len(),
            "some salvaged edit blocks matched no file; the rendered patch is \
             partial"
        );
    }

    // `git diff --no-index` over the two trees, so the result is a real
    // unified diff a user can `git apply`.
    // Whatever happens from here, the scratch copy goes. It is a copy of the
    // user's source sitting in a shared temp directory, and the first version
    // of this cleaned up *before* running the diff — which both emptied the
    // directory the diff reads and left the cleanup as the only thing that
    // could be observed.
    render_and_clean(&scratch, &before, &after)
}

/// Diff the two staged trees, then remove the scratch copy — on every path.
///
/// The scratch is a copy of the user's source in a shared temp directory, so
/// leaving it behind is a leak of their code. The first version cleaned up
/// *before* running the diff, which emptied the directory the diff reads; and
/// a second attempt turned the error path into a `panic_any`, which is worse
/// still — a recovery that panics is not a recovery.
fn render_and_clean(
    scratch: &std::path::Path,
    before: &std::path::Path,
    after: &std::path::Path,
) -> Result<String> {
    let result = (|| -> Result<String> {
        let out = std::process::Command::new("git")
            .arg("diff")
            .arg("--no-index")
            .arg("--no-color")
            // No `--src-prefix`/`--dst-prefix`: git concatenates the prefix
            // with the path it prints, and it prints the path *without* its
            // leading separator, so `a/` + `/src/lib.rs` came out as
            // `asrc/lib.rs`. The defaults are rewritten instead.
            .arg(before)
            .arg(after)
            .output()
            .map_err(|e| anyhow::anyhow!("could not run `git diff`: {e}"))?;
        // `git diff --no-index` exits 1 when the trees differ. That is a diff, not
        // an error; anything above 1 is a real failure.
        if out.status.code().unwrap_or(2) > 1 {
            anyhow::bail!(
                "`git diff --no-index` failed ({}): {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        let mut patch = String::from_utf8_lossy(&out.stdout).to_string();
        // `--no-index` writes the scratch paths into the headers, which would
        // make the patch apply to `/tmp/.../before/x.rs`. Rewrite them to the
        // repository-relative paths, or the patch is a lie about where it applies.
        patch = rewrite_scratch_paths(&patch, before, after);

        if patch.trim().is_empty() {
            anyhow::bail!(
                "the salvaged diff renders to no changes — the Coder reported \
             file(s) but the edits did not alter them"
            );
        }
        Ok(format!(
            "{}\n\
         # ─────────────────────────────────────────────────────────────────────\n\
         # UNREVIEWED. This run FAILED. No Reviewer approved this change and no\n\
         # gate ran, so nothing here has been verified. It is rendered from the\n\
         # Coder's own `CodeDiff` so you can read it; apply it with\n\
         # `git apply` only if you have read it and agree.\n\
         # ─────────────────────────────────────────────────────────────────────\n",
            patch.trim_end()
        ))
    })();
    // Best-effort, on the error path as well as the good one. Failing to clean
    // up must not fail the render: the render is the recovery, and the run has
    // already failed once.
    let _ = std::fs::remove_dir_all(scratch);
    result
}

/// The first line of a search string, for naming an edit that matched nothing.
fn summarise_search(search: &str) -> String {
    let first = search.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let head: String = first.chars().take(60).collect();
    format!("{head:?}")
}

fn copy_if_present(
    from_root: &std::path::Path,
    rel: &std::path::Path,
    to: &std::path::Path,
) -> Result<()> {
    let src = from_root.join(rel);
    if !src.exists() {
        return Ok(());
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(&src, to)?;
    Ok(())
}

/// Replace the scratch directory names in a `--no-index` diff with the
/// repository-relative paths.
///
/// Without this the patch's `---`/`+++` headers point at
/// `/tmp/.tmpXXXX/before/src/lib.rs`, and `git apply` would create that path.
/// A patch that applies in the wrong place is worse than no patch.
fn rewrite_scratch_paths(patch: &str, before: &std::path::Path, after: &std::path::Path) -> String {
    let b = before.to_string_lossy().to_string();
    let a = after.to_string_lossy().to_string();
    // `git diff --no-index` prints the prefixes *concatenated* with the path
    // it was given, leading separator removed: passing
    // `/tmp/niki-salvage.1.2/before` yields
    // `a/tmp/niki-salvage.1.2/before/src/lib.rs`, not `a//tmp/…`. So the
    // scratch path is stripped on its own and the prefix is left alone.
    //
    // Getting this wrong is not cosmetic: the headers would name
    // `/tmp/...`, and `git apply` would create that path instead of editing
    // the repository the patch was rendered from. `the_patch_applies_to_the_
    // repository` runs a real `git apply --check` so the claim is measured.
    //
    // Two spellings, because which one git prints depends on how the path was
    // given. Measured, not guessed: with a leading separator it comes out as
    // `a//tmp/…/before/src/lib.rs`, without one as `a/tmp/…/before/src/lib.rs`.
    // Stripping only the first is what produced `asrc/lib.rs` — the prefix with
    // nothing between it and the file — which `git apply` would resolve against
    // the wrong root.
    //
    // Order is load-bearing. The leading-separator pattern
    // (`/tmp/.../before/`) is a *substring* of the prefix form
    // (`a/tmp/.../before/`), so running it first strips the scratch path out
    // of the middle and leaves `asrc/lib.rs` — the prefix welded to the file.
    // That is what the first three attempts produced, and it is why the
    // rewritten paths are asserted by running `git apply --check` rather than
    // by reading the string.
    let b_bare = b.trim_start_matches('/');
    let a_bare = a.trim_start_matches('/');
    patch
        .replace(&format!("a/{b_bare}/"), "a/")
        .replace(&format!("b/{a_bare}/"), "b/")
        .replace(&format!("{b}/"), "")
        .replace(&format!("{a}/"), "")
}
