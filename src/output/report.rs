use anyhow::Result;
use minijinja::{Environment, context};
use crate::orchestrator::pipeline::{Task, PipelineResult, TopologyMode};
use crate::config::NikiConfig;
use crate::safety::SafetyProof;
use crate::artifacts::types::{AgentRole, RedChallenge, ReviewVerdict, SecurityVerdict};
use std::fs;

/// Build the "## Hermetic Safety Proof" markdown section from the run's proof.
/// Returns an empty string when no proof was computed (e.g. prior to 1.1).
fn render_safety_section(result: &PipelineResult) -> String {
    let proof: &SafetyProof = match &result.safety_proof {
        Some(p) => p,
        None => return String::new(),
    };

    let mut out = String::from("## Hermetic Safety Proof\n\n");
    out.push_str(&format!("{}\n\n", proof.blast_radius));
    for d in &proof.details {
        out.push_str(&format!("- {}\n", d));
    }
    out.push('\n');
    if proof.hermetic {
        out.push_str(
            "_Your working tree and existing branches were never mutated. The Coder's output \
             landed only on the new branch shown above._\n",
        );
    } else {
        out.push_str(
            "_⚠️ This run was NOT hermetic. Inspect the details above before trusting the branch._\n",
        );
    }
    out
}

/// Build the "## Adversarial Review (Red/Blue)" section (#1.2).
///
/// Renders the independent Red agent's challenges alongside the Reviewer's
/// per-challenge reconciliation. This is what *proves* the Reviewer engaged with
/// the adversarial critique instead of ratifying the Coder. Returns an empty
/// string when the Red/Blue pass was disabled (no Red artifact).
fn render_red_blue_section(result: &PipelineResult) -> String {
    let red_json = match result.artifacts.iter().find(|(r, _)| *r == AgentRole::Red) {
        Some((_, j)) => j,
        None => return String::new(),
    };
    let red: RedChallenge = match serde_json::from_str(red_json) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    // The Reviewer's reconciliation, keyed by Red challenge id.
    let mut dispositions: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();
    if let Some((_, rev_json)) = result.artifacts.iter().find(|(r, _)| *r == AgentRole::Reviewer) {
        if let Ok(verdict) = serde_json::from_str::<ReviewVerdict>(rev_json) {
            if let Some(recon) = verdict.red_reconciliation {
                for r in recon {
                    dispositions.insert(
                        r.challenge_id,
                        (
                            format!("{:?}", r.disposition),
                            r.rationale,
                        ),
                    );
                }
            }
        }
    }

    let mut out = String::from("## Adversarial Review (Red/Blue)\n\n");
    out.push_str(&format!("{}\n\n", red.overall_red_assessment));
    if red.challenges.is_empty() {
        out.push_str("_The Red agent raised no challenges — the change withstood adversarial scrutiny._\n");
    } else {
        out.push_str("| ID | Severity | Category | Red claim | Reviewer |\n");
        out.push_str("|----|----------|---------|-----------|----------|\n");
        for p in &red.challenges {
            let (disp, rationale) = dispositions
                .get(&p.id)
                .cloned()
                .unwrap_or_else(|| ("(not reconciled)".to_string(), String::new()));
            let reviewer = if rationale.is_empty() {
                disp
            } else {
                format!("{} — {}", disp, rationale)
            };
            out.push_str(&format!(
                "| {} | {:?} | {:?} | {} | {} |\n",
                p.id, p.severity, p.category, p.claim, reviewer
            ));
        }
        out.push('\n');
    }
    out.push_str(
        "_An independent Red agent — which never saw the Coder's reasoning — probed this change; \
         the Reviewer was required to uphold or refute each point above._\n",
    );
    out
}

/// Build the "## Agent Isolation" section (BUILD_PLAN 2.1, P1.2).
///
/// Renders one row per agent recording which published artifacts it saw and the
/// sandbox backend it executed in, plus the paragraph that explains why this is
/// not mere context-sharing "subagents". Returns empty when no isolation records
/// were captured (e.g. replayed eval fixtures).
fn render_isolation_section(result: &PipelineResult) -> String {
    if result.isolation.is_empty() {
        return String::new();
    }

    let mut out = String::from("## Agent Isolation\n\n");
    out.push_str(
        "Every agent below ran as an **independent LLM session**. It received only the \
         *published output artifacts* of earlier roles — never another agent's \
         chain-of-thought, scratchpad, or the parent conversation.\n\n",
    );
    out.push_str(
        "| Agent | Sandbox | Saw only published artifacts from | Shared reasoning? |\n\
         |-------|---------|-------------------------------------|--------------------|\n",
    );
    for rec in &result.isolation {
        let sources = if rec.context_sources.is_empty() {
            "— (entry point)".to_string()
        } else {
            rec.context_sources
                .iter()
                .map(|r| format!("{:?}", r))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let shared = if rec.saw_other_reasoning { "Yes ⚠️" } else { "No" };
        out.push_str(&format!(
            "| {:?} | {:?} | {} | {} |\n",
            rec.role, rec.backend, sources, shared
        ));
    }
    out.push('\n');
    out.push_str(
        "**Why this isn't just subagents:** context-sharing subagents inherit the parent's \
         running conversation, so they converge on the same reasoning and can rubber-stamp \
         each other (sycophantic convergence). NIKI's agents do not. The Coder never sees the \
         Reviewer's verdict before it writes code; the Reviewer never sees the Coder's private \
         reasoning — only its diff. Because no agent can read another's intermediate thoughts, \
         they cannot collude, and the adversarial Red agent (which sees only the spec, the diff, \
         and the tests) probes the work cold. Isolation is the product's core thesis, and the \
         table above is proof it held for this run.\n",
    );
    out
}

/// Parse a diff line-range string like "42", "42-58" into an inclusive (start, end).
fn parse_range(r: &str) -> Option<(u32, u32)> {
    let r = r.trim();
    if let Some((a, b)) = r.split_once('-') {
        let a: u32 = a.trim().parse().ok()?;
        let b: u32 = b.trim().parse().ok()?;
        Some((a, b))
    } else {
        let n: u32 = r.parse().ok()?;
        Some((n, n))
    }
}

/// Given a unified `diff`, find the hunk for `b/<file>` and annotate the lines
/// whose new-file number falls inside `line_range` with a `▶` marker. Returns
/// `None` when the file has no entry in the diff (so callers can degrade to a
/// plain textual finding instead of a bogus empty block).
fn extract_and_annotate(diff: &str, file: &str, line_range: &Option<String>) -> Option<String> {
    let lines: Vec<&str> = diff.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].starts_with("diff --git") {
            let mut block_end = i + 1;
            let mut matched = false;
            while block_end < lines.len() && !lines[block_end].starts_with("diff --git") {
                if lines[block_end].starts_with("+++ ") {
                    let p = lines[block_end][4..].trim();
                    let p = p.strip_prefix("b/").unwrap_or(p);
                    if p == file {
                        matched = true;
                    }
                }
                block_end += 1;
            }
            if matched {
                let block: Vec<&str> = lines[i..block_end].to_vec();
                return Some(annotate_hunk(&block.join("\n"), line_range));
            }
            i = block_end;
        } else {
            i += 1;
        }
    }
    None
}

/// Annotate a single-file diff block: lines whose new-file number is inside
/// `line_range` get a `▶` prefix so the reader can see exactly which changed
/// line the audit flagged.
fn annotate_hunk(hunk: &str, line_range: &Option<String>) -> String {
    let range = line_range.as_ref().and_then(|r| parse_range(r));
    let mut new_line: u32 = 0;
    let mut in_hunk = false;
    let mut out = String::new();
    for line in hunk.lines() {
        if line.starts_with("@@") {
            // new-file start is the number after the '+' in "@@ -a,b +c,d @@"
            if let Some(pos) = line.find('+') {
                let rest = &line[pos + 1..];
                let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                new_line = digits.parse().unwrap_or(0);
            }
            in_hunk = true;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if !in_hunk {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let marker = if line.starts_with('+') || line.starts_with(' ') {
            let cur = new_line;
            new_line += 1;
            match range {
                Some((s, e)) if cur >= s && cur <= e => " ▶ ",
                _ => "   ",
            }
        } else {
            "   "
        };
        out.push_str(marker);
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Build the "## Audit Trail" markdown section (BUILD_PLAN 2.2, P1.3).
///
/// This is the difference between "fully auditable" as a *label* and as an
/// *outcome*. It walks the Reviewer's and Security Auditor's actual findings,
/// anchors each to the diff line it was found on (annotated with `▶`), and
/// reports how many defects the Coder introduced that the trail caught. Returns
/// empty when no issues were recorded — i.e. nothing for the audit to catch.
fn render_audit_section(result: &PipelineResult) -> String {
    struct Catch {
        stage: String,
        severity: String,
        category: String,
        file: Option<String>,
        line: Option<String>,
        description: String,
    }

    let mut catches: Vec<Catch> = Vec::new();
    for (role, json) in &result.artifacts {
        if *role == AgentRole::Reviewer {
            if let Ok(v) = serde_json::from_str::<ReviewVerdict>(json) {
                for issue in &v.issues {
                    catches.push(Catch {
                        stage: "Reviewer".into(),
                        severity: format!("{:?}", issue.severity),
                        category: format!("{:?}", issue.category),
                        file: issue.file_path.clone(),
                        line: issue.line_range.clone(),
                        description: issue.description.clone(),
                    });
                }
            }
        } else if *role == AgentRole::SecurityAuditor {
            if let Ok(v) = serde_json::from_str::<SecurityVerdict>(json) {
                for f in &v.findings {
                    catches.push(Catch {
                        stage: "Security Auditor".into(),
                        severity: format!("{:?}", f.severity),
                        category: format!("{:?}", f.category),
                        file: f.file_path.clone(),
                        line: f.line_range.clone(),
                        description: f.description.clone(),
                    });
                }
            }
        }
    }

    if catches.is_empty() {
        return String::new();
    }

    let mut out = String::from("## Audit Trail\n\n");
    out.push_str(
        "This is not an \"auditable\" label — it is the record of the Reviewer and \
         Security Auditor **catching** defects the Coder introduced, each anchored to \
         the line it was found on. The `▶` mark points at the exact changed line.\n\n",
    );

    let mut crit = 0usize;
    let mut high = 0usize;
    let mut major = 0usize;
    let mut other = 0usize;
    for c in &catches {
        match c.severity.as_str() {
            "Critical" => crit += 1,
            "High" => high += 1,
            "Major" => major += 1,
            _ => other += 1,
        }
    }
    out.push_str(&format!(
        "**Caught {} issue(s) the Coder introduced:** {} Critical, {} High, {} Major, {} other.\n\n",
        catches.len(),
        crit,
        high,
        major,
        other,
    ));

    for c in &catches {
        let loc = match (&c.file, &c.line) {
            (Some(f), Some(l)) => format!("`{}` (lines {})", f, l),
            (Some(f), None) => format!("`{}`", f),
            (None, Some(l)) => format!("(lines {})", l),
            (None, None) => "location unspecified".into(),
        };
        out.push_str(&format!(
            "### [{} · {} · {}] {}\n",
            c.stage, c.severity, c.category, loc
        ));
        if let (Some(f), Some(_)) = (&c.file, &c.line) {
            if let Some(annotated) = extract_and_annotate(&result.final_diff, f, &c.line) {
                out.push_str("```diff\n");
                out.push_str(&annotated);
                out.push_str("```\n\n");
            }
        }
        out.push_str(&format!("{}\n\n", c.description));
    }
    out
}

/// Build the "## Cost & Performance" markdown section from per-agent metrics.
fn render_cost_section(result: &PipelineResult) -> String {
    if result.metrics.is_empty() {
        return String::new();
    }

    let mut out = String::from(
        "## Cost & Performance\n\n\
         | Agent | Provider | Model | In tok | Out tok | Latency | Cost |\n\
         |-------|----------|-------|-------:|--------:|--------:|-----:|\n",
    );

    let mut total_in: u32 = 0;
    let mut total_out: u32 = 0;
    let mut total_ms: u64 = 0;
    let mut total_cost: f64 = 0.0;

    for m in &result.metrics {
        total_in += m.input_tokens;
        total_out += m.output_tokens;
        total_ms += m.latency_ms;
        total_cost += m.cost_usd;
        let cost = if m.cost_usd > 0.0 {
            format!("${:.4}", m.cost_usd)
        } else {
            "n/a".to_string()
        };
        out.push_str(&format!(
            "| {:?} | {} | {} | {} | {} | {:.1}s | {} |\n",
            m.role,
            m.provider,
            m.model,
            m.input_tokens,
            m.output_tokens,
            m.latency_ms as f64 / 1000.0,
            cost,
        ));
    }

    let total_cost_str = if total_cost > 0.0 {
        format!("${:.4}", total_cost)
    } else {
        "n/a".to_string()
    };
    out.push_str(&format!(
        "| **Total** | | | **{}** | **{}** | **{:.1}s** | **{}** |\n",
        total_in, total_out, total_ms as f64 / 1000.0, total_cost_str,
    ));

    // --- Cost transparency vs a single autonomous agent (BUILD_PLAN 2.3, P1.4) ---
    let total_tokens = total_in as u64 + total_out as u64;
    let peak_input = result.metrics.iter().map(|m| m.input_tokens).max().unwrap_or(0);
    let baseline_tokens = peak_input as u64 + total_out as u64;
    let token_multiple = if baseline_tokens > 0 {
        total_tokens as f64 / baseline_tokens as f64
    } else {
        0.0
    };
    let redundant_input = total_in.saturating_sub(peak_input);

    out.push_str("\n### vs a single autonomous agent\n\n");
    out.push_str(
        "A single agent doing this in one pass would ingest the full context **once** \
         (peak input) and generate the same total output. It would not pay the \
         multi-agent tax of re-feeding shared context to every stage. That tax is the \
         measurable cost of the isolation that makes NIKI trustworthy.\n\n",
    );
    out.push_str(&format!(
        "- NIKI total tokens: **{}** (in {} / out {})\n\
         - Single-agent estimate: **{}** tokens (peak input {} + output {})\n\
         - **Token multiple: {:.2}×** — the cost of isolation.\n\
         - Redundant context re-ingested across stages: {} input tokens.\n\n",
        total_tokens,
        total_in,
        total_out,
        baseline_tokens,
        peak_input,
        total_out,
        token_multiple,
        redundant_input,
    ));

    if total_cost > 0.0 && total_tokens > 0 {
        let blended_rate = total_cost / total_tokens as f64;
        let baseline_cost = baseline_tokens as f64 * blended_rate;
        let extra = total_cost - baseline_cost;
        out.push_str(&format!(
            "- NIKI cost: **${:.4}**\n\
             - Single-agent estimate: **${:.4}** (at NIKI's blended rate)\n\
             - Extra paid for multi-agent independence: **${:.4}**\n\n",
            total_cost, baseline_cost, extra,
        ));
    } else {
        out.push_str(
            "_Cost not measured for this run (no pricing in metrics) — the token \
             multiple above is the comparable, price-independent figure._\n\n",
        );
    }

    // Cheap-model-for-Tester mixing is the explicit knob that lowers the tax.
    let primary_model = result
        .metrics
        .iter()
        .find(|m| m.role == AgentRole::Coder || m.role == AgentRole::Planner)
        .map(|m| m.model.clone());
    let tester_model = result
        .metrics
        .iter()
        .find(|m| m.role == AgentRole::Tester)
        .map(|m| m.model.clone());
    if let (Some(p), Some(t)) = (&primary_model, &tester_model) {
        if p != t {
            out.push_str(&format!(
                "- **Token-tax control:** the Tester runs on `{}` while the Coder uses \
                 `{}`, mixing a cheaper model for the high-volume test-execution stage \
                 to keep the multi-agent cost down.\n\n",
                t, p,
            ));
        } else {
            out.push_str(
                "- All stages use the same model; mixing a cheaper model for the Tester \
                 would lower the multi-agent token tax.\n\n",
            );
        }
    }

    out.push('\n');
    out
}

pub fn generate_report(
    task: &Task,
    config: &NikiConfig,
    result: &PipelineResult,
) -> Result<()> {
    let mut env = Environment::new();

    let template = r#"
# NIKI Execution Report

**Task ID**: {{ task_id }}
**Description**: {{ description }}
**Project Path**: {{ project_path }}

## Pipeline Result
- Verdict: {{ verdict }}
- Revision Rounds: {{ revision_rounds }}
- Topology: {{ topology_line }}

{{ safety_section }}
{{ red_blue_section }}
{{ isolation_section }}
{{ audit_section }}
{{ cost_section }}
## Final Diff
```diff
{{ final_diff }}
```
"#;

    env.add_template("report.md", template)?;
    let tmpl = env.get_template("report.md")?;

    let rendered = tmpl.render(context! {
        task_id => task.id.to_string(),
        description => task.description.clone(),
        project_path => task.project_path.to_string_lossy().to_string(),
        verdict => format!("{:?}", result.verdict),
        revision_rounds => result.revision_rounds,
        topology_line => topology_line(result),
        safety_section => render_safety_section(result),
        red_blue_section => render_red_blue_section(result),
        isolation_section => render_isolation_section(result),
        audit_section => render_audit_section(result),
        cost_section => render_cost_section(result),
        final_diff => result.final_diff.clone(),
    })?;

    let output_dir = task
        .project_path
        .join(&config.general.output_dir)
        .join("tasks")
        .join(task.id.to_string());
    fs::create_dir_all(&output_dir)?;

    let report_path = output_dir.join("report.md");
    fs::write(&report_path, rendered)?;

    let diff_path = output_dir.join("changes.patch");
    fs::write(&diff_path, &result.final_diff)?;

    Ok(())
}

/// Human-readable topology line for the report's `## Pipeline Result` block
/// (BUILD_PLAN 3.2, P2.2). The single-agent fast-path is named honestly: it
/// collapses Tester/Reviewer/Red into one solo Coder, so there is no
/// independent adversarial review — the trade-off is surfaced, never hidden.
pub fn topology_line(result: &PipelineResult) -> String {
    match result.topology {
        TopologyMode::SingleAgent => {
            "single-agent fast-path (Planner + solo Coder; Tester/Reviewer/Red collapsed)".to_string()
        }
        TopologyMode::MultiAgent | TopologyMode::Auto => {
            "multi-agent (Planner → Coder → Tester → Red → Reviewer)".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::state::{PipelineState, StageMetric};
    use crate::artifacts::types::{AgentRole, IsolationRecord, Verdict};
    use crate::safety::SafetyProof;
    use crate::sandbox::SandboxBackend;
    use uuid::Uuid;

    fn result_with_proof(proof: Option<SafetyProof>) -> PipelineResult {
        PipelineResult {
            task_id: Uuid::nil(),
            state: PipelineState::new(Uuid::nil()),
            final_diff: String::from("+hello"),
            verdict: Verdict::Approved,
            revision_rounds: 1,
            artifacts: vec![],
            metrics: vec![StageMetric {
                role: AgentRole::Planner,
                provider: "anthropic".into(),
                model: "claude".into(),
                input_tokens: 1,
                output_tokens: 1,
                latency_ms: 1,
                cost_usd: 0.0,
            }],
            safety_proof: proof,
            isolation: vec![],
            topology: TopologyMode::Auto,
        }
    }

    #[test]
    fn renders_safety_section_when_proof_present() {
        let proof = SafetyProof {
            hermetic: true,
            branch_added: true,
            existing_branches_preserved: true,
            new_branch_parent_is_base: true,
            new_branch: "niki/abc12345".into(),
            pre_working_tree_clean: true,
            post_working_tree_clean: true,
            blast_radius: "Hermetic: working tree never mutated.".into(),
            details: vec!["PASS existing branches preserved.".into()],
        };
        let result = result_with_proof(Some(proof));

        // Drive the real report generator into a temp dir and read it back, so we
        // exercise the full template render path, not just the section builder.
        let dir = std::env::temp_dir().join(format!("niki-report-test-{}-{}", std::process::id(), Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let task = Task {
            id: Uuid::nil(),
            description: "test".into(),
            project_path: dir.clone(),
        };
        let cfg = crate::config::NikiConfig::default();
        generate_report(&task, &cfg, &result).expect("report should render");

        let report = std::fs::read_to_string(dir.join(".niki").join("tasks").join(Uuid::nil().to_string()).join("report.md"))
            .expect("report.md should exist");
        assert!(report.contains("## Hermetic Safety Proof"));
        assert!(report.contains("Hermetic: working tree never mutated."));
        assert!(report.contains("PASS existing branches preserved."));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn renders_no_safety_section_when_absent() {
        let result = result_with_proof(None);
        let section = render_safety_section(&result);
        assert!(section.is_empty());
    }

    #[test]
    fn renders_topology_line_for_both_modes() {
        // Both the default multi-agent chain and the single-agent fast-path
        // must surface a Topology line in `## Pipeline Result` (BUILD_PLAN 3.2).
        let mut multi = result_with_proof(None);
        multi.topology = TopologyMode::MultiAgent;
        let multi_report = generate_report_text(&multi);
        assert!(multi_report.contains("## Pipeline Result"));
        assert!(multi_report.contains("Topology:"));
        assert!(multi_report.contains("multi-agent (Planner → Coder → Tester → Red → Reviewer)"));

        let mut single = result_with_proof(None);
        single.topology = TopologyMode::SingleAgent;
        let single_report = generate_report_text(&single);
        assert!(single_report.contains("Topology:"));
        assert!(single_report.contains(
            "single-agent fast-path (Planner + solo Coder; Tester/Reviewer/Red collapsed)",
        ));
    }

    #[test]
    fn renders_red_blue_section_with_reconciliation() {
        let red_artifact = serde_json::json!({
            "overall_red_assessment": "This change is risky.",
            "challenges": [
                {"id": "R1", "severity": "major", "category": "logic",
                 "claim": "Off-by-one in the loop bound.", "confidence": 8}
            ]
        })
        .to_string();
        let reviewer_artifact = serde_json::json!({
            "verdict": "revision_needed",
            "overall_assessment": "Fixing the bound.",
            "quality_scores": {"correctness": 5, "code_quality": 7, "test_coverage": 6, "spec_adherence": 8},
            "issues": [],
            "strengths": [],
            "feedback": null,
            "red_reconciliation": [
                {"challenge_id": "R1", "disposition": "upheld", "rationale": "Confirmed off-by-one."}
            ]
        })
        .to_string();

        let result = PipelineResult {
            task_id: Uuid::nil(),
            state: PipelineState::new(Uuid::nil()),
            final_diff: String::from("+x"),
            verdict: Verdict::RevisionNeeded,
            revision_rounds: 1,
            artifacts: vec![
                (AgentRole::Red, red_artifact),
                (AgentRole::Reviewer, reviewer_artifact),
            ],
            metrics: vec![],
            safety_proof: None,
            isolation: vec![],
            topology: TopologyMode::Auto,
        };

        let section = render_red_blue_section(&result);
        assert!(section.contains("## Adversarial Review (Red/Blue)"));
        assert!(section.contains("R1"));
        assert!(section.contains("Upheld"));
        assert!(section.contains("Confirmed off-by-one."));
    }

    #[test]
    fn renders_no_red_blue_section_when_disabled() {
        let result = result_with_proof(None);
        let section = render_red_blue_section(&result);
        assert!(section.is_empty());
    }

    #[test]
    fn renders_isolation_section_with_proof() {
        let result = PipelineResult {
            task_id: Uuid::nil(),
            state: PipelineState::new(Uuid::nil()),
            final_diff: String::new(),
            verdict: Verdict::Approved,
            revision_rounds: 1,
            artifacts: vec![],
            metrics: vec![],
            safety_proof: None,
            isolation: vec![
                IsolationRecord {
                    role: AgentRole::Planner,
                    backend: SandboxBackend::Docker,
                    context_sources: vec![],
                    saw_other_reasoning: false,
                },
                IsolationRecord {
                    role: AgentRole::Coder,
                    backend: SandboxBackend::Docker,
                    context_sources: vec![AgentRole::Planner],
                    saw_other_reasoning: false,
                },
                IsolationRecord {
                    role: AgentRole::Reviewer,
                    backend: SandboxBackend::Docker,
                    context_sources: vec![
                        AgentRole::Planner,
                        AgentRole::Coder,
                        AgentRole::Tester,
                    ],
                    saw_other_reasoning: false,
                },
            ],
            topology: TopologyMode::Auto,
        };
        let section = render_isolation_section(&result);
        assert!(section.contains("## Agent Isolation"));
        assert!(section.contains("Why this isn't just subagents"));
        assert!(section.contains("Planner"));
        assert!(section.contains("Coder"));
        assert!(section.contains("Reviewer"));
        // No agent may report that it saw another agent's reasoning.
        assert!(!section.contains("Yes"));
    }

    #[test]
    fn renders_no_isolation_section_when_empty() {
        let result = result_with_proof(None);
        assert!(render_isolation_section(&result).is_empty());
    }

    /// Drive the real `generate_report` into a temp dir and return the rendered
    /// `report.md` text, so audit-section tests exercise the full template path.
    fn generate_report_text(result: &PipelineResult) -> String {
        let dir = std::env::temp_dir().join(format!("niki-audit-test-{}-{}", std::process::id(), Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let task = Task {
            id: Uuid::nil(),
            description: "test".into(),
            project_path: dir.clone(),
        };
        let cfg = crate::config::NikiConfig::default();
        generate_report(&task, &cfg, result).expect("report should render");
        let report = std::fs::read_to_string(
            dir.join(".niki").join("tasks").join(Uuid::nil().to_string()).join("report.md"),
        )
        .expect("report.md should exist");
        let _ = std::fs::remove_dir_all(&dir);
        report
    }

    fn audit_result(reviewer_json: Option<&str>, security_json: Option<&str>, diff: &str) -> PipelineResult {
        let mut artifacts: Vec<(AgentRole, String)> = vec![];
        if let Some(j) = reviewer_json {
            artifacts.push((AgentRole::Reviewer, j.to_string()));
        }
        if let Some(j) = security_json {
            artifacts.push((AgentRole::SecurityAuditor, j.to_string()));
        }
        PipelineResult {
            task_id: Uuid::nil(),
            state: PipelineState::new(Uuid::nil()),
            final_diff: diff.to_string(),
            verdict: Verdict::RevisionNeeded,
            revision_rounds: 1,
            artifacts,
            metrics: vec![],
            safety_proof: None,
            isolation: vec![],
            topology: TopologyMode::Auto,
        }
    }

    #[test]
    fn renders_audit_section_with_reviewer_catch() {
        let reviewer = r#"{
            "verdict": "revision_needed",
            "overall_assessment": "SQL injection risk in query builder",
            "quality_scores": {"correctness": 1, "code_quality": 2, "test_coverage": 3, "spec_adherence": 4},
            "issues": [
                {"severity": "critical", "category": "security", "file_path": "src/db.rs",
                 "line_range": "42-58", "description": "User input is concatenated directly into the SQL query."}
            ],
            "strengths": [],
            "feedback": null,
            "red_reconciliation": null
        }"#;
        let diff = "\
diff --git a/src/db.rs b/src/db.rs
index 1111111..2222222 100644
--- a/src/db.rs
+++ b/src/db.rs
@@ -40,3 +40,4 @@ fn run_query
     let conn = pool.get();
-    let sql = \"SELECT * FROM users\";
+    let sql = format!(\"SELECT * FROM users WHERE id = {}\", user_id);
+    let rows = conn.query(sql);
     Ok(rows)
";
        let result = audit_result(Some(reviewer), None, diff);
        let report = generate_report_text(&result);
        assert!(report.contains("## Audit Trail"));
        assert!(report.contains("Caught 1 issue(s) the Coder introduced"));
        assert!(report.contains("Critical"));
        assert!(report.contains("Security"));
        assert!(report.contains("src/db.rs"));
        // The `▶` marker must point at the exact changed line in the diff.
        assert!(report.contains("▶"));
        // The raw finding description is shown too.
        assert!(report.contains("User input is concatenated directly into the SQL query."));
    }

    #[test]
    fn renders_audit_section_with_security_auditor_catch() {
        let security = r#"{
            "verdict": "revision_needed",
            "overall_assessment": "Untrusted input reaches eval",
            "findings": [
                {"severity": "high", "category": "injection", "file_path": "src/api.rs",
                 "line_range": "12", "description": "Request body flows unvalidated into eval()."}
            ],
            "strengths": []
        }"#;
        let diff = "\
diff --git a/src/api.rs b/src/api.rs
index 3333333..4444444 100644
--- a/src/api.rs
+++ b/src/api.rs
@@ -10,3 +10,4 @@ fn handler
     let body = req.body();
     let cmd = body.get(\"cmd\");
+    eval(cmd);
     send(cmd)
";
        let result = audit_result(None, Some(security), diff);
        let report = generate_report_text(&result);
        assert!(report.contains("## Audit Trail"));
        assert!(report.contains("Security Auditor"));
        assert!(report.contains("High"));
        assert!(report.contains("Injection"));
        assert!(report.contains("src/api.rs"));
        assert!(report.contains("▶"));
        assert!(report.contains("Request body flows unvalidated into eval()."));
    }

    #[test]
    fn renders_no_audit_section_when_empty() {
        let result = audit_result(None, None, "");
        assert!(render_audit_section(&result).is_empty());
    }

    fn cost_result(metrics: Vec<StageMetric>) -> PipelineResult {
        PipelineResult {
            task_id: Uuid::nil(),
            state: PipelineState::new(Uuid::nil()),
            final_diff: String::new(),
            verdict: Verdict::Approved,
            revision_rounds: 1,
            artifacts: vec![],
            metrics,
            safety_proof: None,
            isolation: vec![],
            topology: TopologyMode::Auto,
        }
    }

    #[test]
    fn renders_cost_vs_single_agent_with_tax_and_tester_mix() {
        // Tester mixes a cheaper model ("haiku") than the Coder ("sonnet").
        let metrics = vec![
            StageMetric { role: AgentRole::Planner, provider: "a".into(), model: "sonnet".into(), input_tokens: 1000, output_tokens: 200, latency_ms: 1, cost_usd: 0.0100 },
            StageMetric { role: AgentRole::Coder, provider: "a".into(), model: "sonnet".into(), input_tokens: 2000, output_tokens: 500, latency_ms: 1, cost_usd: 0.0200 },
            StageMetric { role: AgentRole::Tester, provider: "a".into(), model: "haiku".into(), input_tokens: 3000, output_tokens: 100, latency_ms: 1, cost_usd: 0.0050 },
            StageMetric { role: AgentRole::Reviewer, provider: "a".into(), model: "sonnet".into(), input_tokens: 1500, output_tokens: 300, latency_ms: 1, cost_usd: 0.0150 },
        ];
        let result = cost_result(metrics);
        let section = render_cost_section(&result);
        // NIKI vs single-agent framing.
        assert!(section.contains("vs a single autonomous agent"));
        assert!(section.contains("Token multiple"));
        assert!(section.contains("2.10"));
        assert!(section.contains("Redundant context re-ingested"));
        assert!(section.contains("Single-agent estimate"));
        // Cost is measured, so the $ comparison must appear.
        assert!(section.contains("NIKI cost:"));
        assert!(section.contains("Extra paid for multi-agent independence"));
        // Cheap-model-for-Tester control is surfaced.
        assert!(section.contains("Token-tax control"));
        assert!(section.contains("haiku"));
        assert!(section.contains("sonnet"));
    }

    #[test]
    fn renders_cost_transparency_without_cost_shows_token_multiple() {
        // No pricing in metrics — must still show the price-independent multiple.
        let metrics = vec![
            StageMetric { role: AgentRole::Planner, provider: "a".into(), model: "claude".into(), input_tokens: 100, output_tokens: 50, latency_ms: 1, cost_usd: 0.0 },
            StageMetric { role: AgentRole::Coder, provider: "a".into(), model: "claude".into(), input_tokens: 200, output_tokens: 100, latency_ms: 1, cost_usd: 0.0 },
        ];
        let result = cost_result(metrics);
        let section = render_cost_section(&result);
        assert!(section.contains("vs a single autonomous agent"));
        assert!(section.contains("Token multiple"));
        assert!(section.contains("not measured"));
        // No Tester stage and same model, so no mixing note is required.
        assert!(!section.contains("Token-tax control"));
    }

    #[test]
    fn renders_empty_cost_section_when_no_metrics() {
        let result = cost_result(vec![]);
        assert!(render_cost_section(&result).is_empty());
    }
}
