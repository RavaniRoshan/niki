use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use crate::artifacts::types::RunOutcome;
use crate::artifacts::types::{
    AgentRole, CodeDiff, CriticDisposition, Critique, IsolationRecord, IssueSeverity, RedChallenge,
    ReviewFeedback, ReviewIssue, ReviewVerdict, SecurityVerdict, Synthesis, TaskSpec, TestReport,
    Verdict,
};
pub use crate::config::types::{PipelineStageConfig, TopologyMode};
use crate::config::{NikiConfig, SecurityPolicyConfig};
use crate::cost::compute_cost;
use crate::display::agent_stream::AgenticDisplay;
use crate::knowledge::indexer::index_project;
use crate::llm::provider::{LlmProvider, create_provider};
use crate::orchestrator::state::{StageMetric, TaskRecord, TaskStatus};
use crate::safety::SafetyProof;
use crate::sandbox::{ActiveContainers, SandboxBackend, create_sandbox};

use crate::agents::run_agent;
use crate::agents::tester::{self, TestExecution};
use minijinja::context;

/// Serialize a CodeDiff's structured edits into the SEARCH/REPLACE text the
/// sandbox applies, binding each block to the file it belongs to.
pub fn code_diff_to_edit_text(diff: &CodeDiff) -> String {
    // Which file each block belongs to.
    //
    // `CodeDiff.edits` is a flat list with no per-edit path — the applier finds
    // each block by searching every file. That works while every block is a
    // modification, and cannot work at all for a creation: a block with an
    // empty `search` has nothing to search for, so it would be applied to
    // whichever file happened to be first, or refused as unmatched.
    //
    // So: when the artifact names exactly one file, every block is bound to
    // it. Otherwise a creation is bound to the file it creates, and a
    // modification is left unbound exactly as before — a search across all
    // files is what the format has always done, and guessing a binding would
    // silently misapply it.
    let single = (diff.files_changed.len() == 1).then(|| diff.files_changed[0].path.clone());
    let created: Vec<&str> = diff
        .files_changed
        .iter()
        .filter(|f| f.action == crate::artifacts::types::FileAction::Create)
        .map(|f| f.path.as_str())
        .collect();

    let mut out = String::new();
    for e in &diff.edits {
        // An empty `search` is either a creation or an append, and the two
        // need different targets. A creation names the file it is creating; an
        // append has to name an existing one, so with a single changed file
        // that is unambiguous and it binds there. Falling through to the
        // cross-file search would append to whichever file happened to be
        // enumerated first, which is the silent misapplication the binding
        // exists to prevent.
        let target = if e.search.trim().is_empty() {
            created
                .first()
                .map(|p| (*p).to_string())
                .or_else(|| single.clone())
        } else {
            single.clone()
        };
        if let Some(path) = target {
            out.push_str(&format!("FILE: {path}\n"));
        }
        out.push_str("<<<<<<< SEARCH\n");
        out.push_str(&e.search);
        out.push('\n');
        out.push_str("=======\n");
        out.push_str(&e.replace);
        out.push('\n');
        out.push_str(">>>>>>> REPLACE\n\n");
    }
    out
}

pub struct Task {
    pub id: Uuid,
    pub description: String,
    pub project_path: PathBuf,
}

#[derive(Debug)]
pub struct PipelineResult {
    pub task_id: Uuid,
    pub state: super::state::PipelineState,
    pub final_diff: String,
    /// The decision, and whether anything actually made it.
    ///
    /// `verdict` is derived from `outcome`; it can no longer be a value
    /// nothing assigned. `verdict_source` is the human-readable form of the
    /// same fact, kept for display and for the report.
    pub outcome: RunOutcome,
    pub verdict: Verdict,
    pub verdict_source: Option<String>,
    pub revision_rounds: u32,
    /// Raw JSON artifacts produced by each agent, in execution order.
    pub artifacts: Vec<(AgentRole, String)>,
    /// Per-agent cost & latency, in execution order.
    pub metrics: Vec<StageMetric>,
    /// Hermetic safety proof (BUILD_PLAN 1.1): proves the committed repo state
    /// was never mutated. Populated by the CLI after the branch is committed.
    pub safety_proof: Option<SafetyProof>,
    /// Per-agent context-isolation records (BUILD_PLAN 2.1): proves each agent
    /// ran as an independent LLM session that saw only published artifacts.
    pub isolation: Vec<IsolationRecord>,
    /// The agent topology NIKI selected for this run (BUILD_PLAN 3.2, P2.2):
    /// `SingleAgent` (fast-path) or `MultiAgent` (full chain). Visible in the
    /// report so the auto-selection is self-describing, not asserted.
    pub topology: TopologyMode,
    /// Why `topology` was selected (auto-rule outcome or explicit config).
    /// Rendered in the report so a fast-path collapse is never silent.
    pub topology_reason: String,
    /// Risk tier the spec classified into (`low`/`normal`/`high`/`security`).
    pub risk_level: String,
    /// Why that tier was assigned (classifier rationale or explicit mode).
    pub risk_rationale: String,
    /// Real test-suite execution result from inside the sandbox, recorded as
    /// verification evidence before the branch is created. `None` when no test
    /// command could be resolved or execution was skipped.
    pub test_execution: Option<TestExecution>,
    /// Diff-size guardrail notice (BUILD_PLAN 1.1 control): when `general.max_diff_lines`
    /// is set and the produced diff exceeds it, this holds a plain-English warning
    /// surfaced in `report.md`. `None` = on or under the limit (or the limit is unset).
    pub diff_guardwarn: Option<String>,
    pub context_budget: crate::memory::compression::ContextBudget,
}

/// Typed output of a single pipeline stage, used for role-specific handling.
pub enum RoleOutput {
    Planner(TaskSpec),
    Coder(CodeDiff),
    Tester(TestReport),
    Reviewer(ReviewVerdict),
    Synthesizer(Synthesis),
    SecurityAuditor(SecurityVerdict),
    Red(RedChallenge),
    Critic(Critique),
}

/// The ordered stages to run, honoring a user-defined `[pipeline]` topology when
/// present, otherwise the classic Planner → Coder → Tester → Reviewer wiring.
/// When `[security] enabled = true`, an independent `SecurityAuditor` stage is
/// injected ahead of the Reviewer (#4). Ahead, not after: an auditor whose
/// findings arrive once the reviewing agent has already finished are a check
/// that cannot influence the thing it checked.
pub fn resolve_stages(config: &NikiConfig) -> Vec<PipelineStageConfig> {
    let mut stages = if !config.pipeline.stages.is_empty() {
        config.pipeline.stages.clone()
    } else {
        // Build default stages from [agents] config, carrying max_tokens/temperature/fallbacks.
        let mut s = Vec::new();
        for (role, agent) in [
            (AgentRole::Planner, &config.agents.planner),
            (AgentRole::Coder, &config.agents.coder),
            (AgentRole::Tester, &config.agents.tester),
            (AgentRole::Reviewer, &config.agents.reviewer),
        ] {
            s.push(PipelineStageConfig {
                role,
                provider: agent.provider.clone(),
                model: agent.model.clone(),
                skip: false,
                max_tokens: agent.effective_max_tokens(),
                temperature: agent.effective_temperature(),
                reasoning_effort: agent.reasoning_effort.clone(),
                fallbacks: agent.fallbacks.clone(),
            });
        }
        s
    };

    if config.security.enabled {
        let (provider, model) = security_stage_target(config);
        if !stages.iter().any(|s| s.role == AgentRole::SecurityAuditor) {
            let agent = &config.agents.security_auditor;
            let stage = PipelineStageConfig {
                role: AgentRole::SecurityAuditor,
                provider: if provider.is_empty() {
                    agent.provider.clone()
                } else {
                    provider
                },
                model: if model.is_empty() {
                    agent.model.clone()
                } else {
                    model
                },
                skip: false,
                max_tokens: agent.effective_max_tokens(),
                temperature: agent.effective_temperature(),
                reasoning_effort: agent.reasoning_effort.clone(),
                fallbacks: agent.fallbacks.clone(),
            };
            // Ahead of the Reviewer, not after it. An auditor whose findings
            // arrive after the reviewing agent has finished are a footnote the
            // Reviewer never reads — the check happens, and the checking
            // cannot influence the thing it checked.
            match stages.iter().position(|s| s.role == AgentRole::Reviewer) {
                Some(pos) => stages.insert(pos, stage),
                None => stages.push(stage),
            }
        }
    }

    // Parallel-coder mode (#3) always reconciles its coders through a
    // Synthesizer; inject the stage when enabled so the pipeline can find it.
    if config.parallel.enabled
        && config.parallel.coder_count > 1
        && !stages.iter().any(|s| s.role == AgentRole::Synthesizer)
    {
        let agent = &config.agents.synthesizer;
        stages.push(PipelineStageConfig {
            role: AgentRole::Synthesizer,
            provider: agent.provider.clone(),
            model: agent.model.clone(),
            skip: false,
            max_tokens: agent.effective_max_tokens(),
            temperature: agent.effective_temperature(),
            fallbacks: agent.fallbacks.clone(),
            reasoning_effort: None,
        });
    }

    // Adversarial Red/Blue verification (#1.2): inject a `Red` stage immediately
    // BEFORE the Reviewer so the Reviewer is forced to reconcile the Red agent's
    // independent critique. This is the structural guard against the Reviewer
    // silently rubber-stamping the Coder (sycophantic convergence).
    if config.red_blue.enabled
        && let Some(pos) = stages.iter().position(|s| s.role == AgentRole::Reviewer)
        && !stages.iter().any(|s| s.role == AgentRole::Red)
    {
        let (provider, model) = red_blue_stage_target(config);
        let agent = &config.agents.red;
        stages.insert(
            pos,
            PipelineStageConfig {
                role: AgentRole::Red,
                provider: if provider.is_empty() {
                    agent.provider.clone()
                } else {
                    provider
                },
                model: if model.is_empty() {
                    agent.model.clone()
                } else {
                    model
                },
                skip: false,
                max_tokens: agent.effective_max_tokens(),
                temperature: agent.effective_temperature(),
                reasoning_effort: agent.reasoning_effort.clone(),
                fallbacks: agent.fallbacks.clone(),
            },
        );
    }

    stages
}

/// Resolve the provider/model for the injected Red stage: explicit `[red_blue]`
/// provider/model overrides win, otherwise fall back to the `[agents] red` binding.
fn red_blue_stage_target(config: &NikiConfig) -> (String, String) {
    let agent = &config.agents.red;
    (
        config
            .red_blue
            .provider
            .clone()
            .unwrap_or_else(|| agent.provider.clone()),
        config
            .red_blue
            .model
            .clone()
            .unwrap_or_else(|| agent.model.clone()),
    )
}

/// Resolve the provider/model for the injected SecurityAuditor stage: explicit
/// `[security] provider/model` overrides win, otherwise fall back to the
/// `[agents] security_auditor` binding.
fn security_stage_target(config: &NikiConfig) -> (String, String) {
    let agent = &config.agents.security_auditor;
    (
        config
            .security
            .provider
            .clone()
            .unwrap_or_else(|| agent.provider.clone()),
        config
            .security
            .model
            .clone()
            .unwrap_or_else(|| agent.model.clone()),
    )
}

/// Adjust the resolved stage list by risk tier (deterministic gating):
///
/// - `Low`: unchanged — today's default path is untouched.
/// - `Normal`: + Critic after the Reviewer (unless `[critic] enabled = false`).
/// - `High`/`Security`: + Critic, and a SecurityAuditor is forced even when
///   `[security]` is off (the tier means the run needs the audit).
///
/// An explicit `[pipeline].stages` topology is never rewritten: user intent
/// wins over risk injection.
pub fn apply_risk_stages(
    stages: Vec<PipelineStageConfig>,
    risk: &crate::risk::TaskRisk,
    config: &NikiConfig,
) -> Vec<PipelineStageConfig> {
    use crate::risk::RiskLevel;
    // Phase 5.7 documented interaction: Low-risk runs pass through unchanged
    // even with `[critic] enabled = true` — the Critic reviews a Reviewer
    // verdict, and Low-risk runs have none worth re-judging. To force a
    // Critic on a Low-risk task, set `[risk] mode` to a higher tier.
    if matches!(risk.level, RiskLevel::Low) {
        return stages;
    }
    if !config.pipeline.stages.is_empty() {
        tracing::info!(
            target: "niki::pipeline",
            "risk is {} but [pipeline].stages is explicit — leaving the topology alone",
            risk.level.as_str()
        );
        return stages;
    }
    let mut out = stages;
    if matches!(risk.level, RiskLevel::High | RiskLevel::Security)
        && !out.iter().any(|s| s.role == AgentRole::SecurityAuditor)
    {
        let (provider, model) = security_stage_target(config);
        let agent = &config.agents.security_auditor;
        let stage = PipelineStageConfig {
            role: AgentRole::SecurityAuditor,
            provider,
            model,
            skip: false,
            max_tokens: agent.effective_max_tokens(),
            temperature: agent.effective_temperature(),
            fallbacks: agent.fallbacks.clone(),
            reasoning_effort: None,
        };
        // Ahead of the Reviewer, for the same reason as in `resolve_stages`:
        // a risk-injected auditor that runs last is a check nothing reads.
        match out.iter().position(|s| s.role == AgentRole::Reviewer) {
            Some(pos) => out.insert(pos, stage),
            None => out.push(stage),
        }
    }
    if config.critic.enabled && !out.iter().any(|s| s.role == AgentRole::Critic) {
        let (provider, model) = critic_stage_target(config, &out);
        let stage = PipelineStageConfig {
            role: AgentRole::Critic,
            provider,
            model,
            skip: false,
            // The Critic is deliberately cheap: grounding checks need
            // determinism, not a large completion.
            max_tokens: config.critic.effective_max_tokens(),
            temperature: config.critic.temperature,
            fallbacks: Vec::new(),
            reasoning_effort: None,
        };
        match out.iter().position(|s| s.role == AgentRole::Reviewer) {
            Some(pos) => out.insert(pos + 1, stage),
            None => out.push(stage),
        }
    }
    out
}

/// Resolve the provider/model for the injected Critic stage: explicit
/// `[critic]` overrides win, otherwise the resolved Reviewer's binding (so
/// the Critic reasons at the same level as the verdict it checks).
fn critic_stage_target(config: &NikiConfig, stages: &[PipelineStageConfig]) -> (String, String) {
    let reviewer = stages.iter().find(|s| s.role == AgentRole::Reviewer);
    let fallback_provider = reviewer
        .map(|s| s.provider.clone())
        .unwrap_or_else(|| config.agents.reviewer.provider.clone());
    let fallback_model = reviewer
        .map(|s| s.model.clone())
        .unwrap_or_else(|| config.agents.reviewer.model.clone());
    (
        config.critic.provider.clone().unwrap_or(fallback_provider),
        config.critic.model.clone().unwrap_or(fallback_model),
    )
}

/// Cache key for the per-provider client map: primary + failover chain, so
/// different failover chains never collide.
fn provider_cache_key(stage: &PipelineStageConfig) -> String {
    if stage.fallbacks.is_empty() {
        stage.provider.clone()
    } else {
        let mut parts = vec![stage.provider.clone()];
        parts.extend(stage.fallbacks.iter().cloned());
        parts.join(":")
    }
}

/// Evidence-only view of the Coder's diff for the Red agent.
///
/// The Red agent must probe the change adversarially, which requires the
/// *evidence* (edit blocks, files changed) but not the Coder's
/// self-justification (`implementation_notes`, `spec_adherence`,
/// `uncertainties`). Those rationale fields are kept in the audit trail but
/// withheld from Red's prompt, so Red cannot be talked out of a finding by
/// the Coder's own framing. Falls back to the full JSON when it does not
/// parse as a `CodeDiff` (e.g. synthesis-replaced payloads of another shape).
fn red_evidence_json(coder_json: &str) -> String {
    match serde_json::from_str::<CodeDiff>(coder_json) {
        Ok(diff) => serde_json::json!({
            "edits": diff.edits,
            "files_changed": diff.files_changed,
        })
        .to_string(),
        Err(_) => {
            tracing::debug!(
                target: "niki::pipeline",
                "coder JSON did not parse as CodeDiff; Red sees the full payload"
            );
            coder_json.to_string()
        }
    }
}

/// The published-artifact roles an agent receives as context, mirroring the
/// `input_artifacts` each prompt is rendered with. This is the *complete* set of
/// prior agents a role could have seen.
/// `with_red` is true when the Red/Blue pass ran (the Reviewer then also sees Red).
///
/// Scope note (goal-a3f9c2, Phase 3): sources are *roles*, and each role's full
/// typed artifact is shared — including its free-text rationale fields — with
/// one exception: Red receives an evidence-only projection of the Coder diff
/// (see [`red_evidence_json`]). Withholding rationale everywhere remains a
/// follow-up; the record below describes wiring truthfully, not aspiration.
fn isolation_sources_for(role: AgentRole, with_red: bool, with_security: bool) -> Vec<AgentRole> {
    use AgentRole::*;
    match role {
        Planner => vec![],
        Coder => vec![Planner],
        Tester => vec![Planner, Coder],
        Red => vec![Planner, Coder, Tester],
        Reviewer => {
            let mut v = vec![Planner, Coder, Tester];
            if with_red {
                v.push(Red);
            }
            // The auditor runs first and its verdict is a Reviewer input, so
            // the record has to name it. Understating the wiring here would
            // make the isolation table a worse description of the run than the
            // run itself.
            if with_security {
                v.push(SecurityAuditor);
            }
            v
        }
        Synthesizer => vec![Planner, Coder],
        // The auditor's prompt is rendered with spec + coder diff only — it
        // never receives Tester/Reviewer/Red artifacts, so the record says so
        // even though that narrowness is itself a follow-up decision. It
        // runs *before* the Reviewer, so it never sees one.
        SecurityAuditor => vec![Planner, Coder],
        // The critic checks the Reviewer's verdict against the same evidence
        // the Reviewer saw (plus Red, when that pass ran).
        Critic => {
            let mut v = vec![Planner, Coder, Tester, Reviewer];
            if with_red {
                v.push(Red);
            }
            if with_security {
                v.push(SecurityAuditor);
            }
            v
        }
    }
}

/// Fold a SecurityAuditor verdict into the run's verdict.
///
/// `Rejected` holds the run open: the finding is not a matter of taste, so it
/// forces a revision and names the auditor as the source, and no Reviewer
/// approval can overturn it (see `apply_reviewer_verdict`).
///
/// `Approved` *clears* that hold. The auditor re-reads the revised code each
/// round, so an explicit approval means the finding it raised is addressed. The
/// hold used to be a latch — set once, never cleared — which meant a run that
/// fixed its security finding still burned every remaining round and reported
/// `RevisionNeeded` forever. A harness that cannot be talked out of a stale
/// alarm is one nobody trusts: the correct fix stops being reported as
/// outstanding.
///
/// `RevisionNeeded` from the auditor is advisory and does not move the verdict
/// — the Reviewer owns the ordinary quality gate.
///
/// It used to apply only when there was *no* reviewer, on the theory that the
/// Reviewer owns the gate. In the normal configuration a Reviewer is always
/// present, so a SecurityAuditor `Rejected` was computed, recorded as an
/// artifact, displayed in the report — and had no effect on the run.
fn apply_security_verdict(
    security_verdict: Verdict,
    verdict: &mut Verdict,
    verdict_source: &mut Option<String>,
    security_hold: &mut bool,
) {
    match security_verdict {
        Verdict::Rejected => {
            *security_hold = true;
            *verdict = Verdict::RevisionNeeded;
            *verdict_source = Some("security-auditor".to_string());
        }
        Verdict::Approved if *security_hold => {
            *security_hold = false;
        }
        _ => {}
    }
}

/// Record a Reviewer's verdict without letting it clear a security hold.
///
/// The SecurityAuditor runs first so the Reviewer can reconcile its findings,
/// which means a well-behaved Reviewer withholds approval on its own. This is
/// the backstop for the case where it does not: a plain `Approved` must not
/// quietly overturn a security rejection that the auditor already raised.
fn apply_reviewer_verdict(
    reviewer_verdict: Verdict,
    verdict: &mut Verdict,
    verdict_source: &mut Option<String>,
    security_hold: bool,
) {
    if security_hold {
        // Leave the verdict and the source where security put them.
        return;
    }
    *verdict = reviewer_verdict;
    *verdict_source = Some("reviewer".to_string());
}

/// Name the files a change touches that the specification never asked for, so
/// the Reviewer can judge scope it would otherwise never see.
///
/// Returns `None` when there is nothing to say: no spec, no Coder artifact, no
/// planned files to compare against, or no drift. The text names both sides on
/// purpose — "this file is unplanned" is only useful next to "these were the
/// planned ones".
///
/// Public so the wording is testable: the Reviewer's prompt is not reachable
/// from a unit test, and a source check would prove nothing about what is
/// actually said.
pub fn scope_drift_note(spec: &TaskSpec, coder_json: &str) -> Option<String> {
    let planned: Vec<&str> = spec
        .files_to_modify
        .iter()
        .map(|f| f.path.as_str())
        .collect();
    if planned.is_empty() || coder_json.is_empty() {
        return None;
    }
    let diff = serde_json::from_str::<CodeDiff>(coder_json).ok()?;
    let unplanned: Vec<&str> = diff
        .files_changed
        .iter()
        .map(|f| f.path.as_str())
        .filter(|p| !planned.contains(p))
        .collect();
    if unplanned.is_empty() {
        return None;
    }
    Some(format!(
        "SCOPE NOTE — the Coder changed file(s) the specification did not list in \
         files_to_modify: {}. The planned files were: {}. Judge whether each of these is \
         genuinely required by the task; one that is not is unrequested scope, and a \
         reviewer approving a diff it was never shown the scope of is not a review.",
        unplanned.join(", "),
        planned.join(", ")
    ))
}

/// Build the Coder's revision brief from the Reviewer's `issues`.
///
/// Only critical and major issues are carried over — a nit is not worth a round
/// on its own, and including them is how a critique becomes noise the model
/// cannot act on. The guidance is the Reviewer's own assessment, so the tone
/// and the reason stay its own rather than the harness paraphrasing them.
fn bridge_feedback_from_issues(issues: &[ReviewIssue], round: u32) -> Option<ReviewFeedback> {
    let actionable: Vec<ReviewIssue> = issues
        .iter()
        .filter(|i| matches!(i.severity, IssueSeverity::Critical | IssueSeverity::Major))
        .cloned()
        .collect();
    if actionable.is_empty() {
        return None;
    }
    Some(ReviewFeedback {
        critical_issues: actionable,
        guidance: "Address each issue above, in the files it names.".to_string(),
        keep_unchanged: Vec::new(),
        revision_round: round,
    })
}

/// What to do about a Coder patch that validated but would not apply.
///
/// Split out because the distinction matters and is invisible from a full
/// pipeline run: a *mechanical* error — a `search` that is not in the file —
/// is not a judgment about quality, so charging it to the Reviewer's revision
/// budget spends a round the Reviewer asked for on something it never asked
/// about. Measured: a run whose Reviewer was still finding real issues burned
/// its last round this way and failed with the change it had already made
/// twice discarded.
///
/// Separate and smaller, because unlike a quality round this one is not
/// One allowance for "tell the model what went wrong and let it try again",
/// shared across the causes the pipeline decides.
///
/// This replaces a per-cause counter. `MAX_PATCH_REPAIR_ATTEMPTS` was patched
/// in on its own, and the point of a shared one is the invariant Aider keeps
/// its single `max_reflections` for: *no single cause can spend it twice*, and
/// the number a user is told is a total rather than whichever constant
/// happened to run out.
///
/// It does not yet cover everything, and saying so is part of the design. The
/// Coder tool loop bounds its own truncation retry (`MAX_TRUNCATED_ANSWER_
/// RETRIES`, inside `run_tool_loop_with`) and the agent bounds malformed-JSON
/// repair inside a single call (`MAX_REPAIR_RETRIES`). Uniting all three means
/// carrying a turn count out of the loop and through `run_role`, which changes
/// a hot signature; a budget wired halfway through a hidden channel is worse
/// than two named ones. Two budgets, both visible, is the honest interim.
///
/// Aider keeps one counter for the whole apply → lint → test → re-apply cycle
/// and the invariant is the point: *no single cause can spend it twice*. That
/// is the shape here. The cause is recorded so the reason a run stopped names
/// what actually went wrong rather than whichever budget ran out first.
///
/// It is not the Reviewer's revision budget and deliberately does not consume
/// it. A patch that would not apply is a mechanical error, not a judgment about
/// quality, and charging it to `max_revision_rounds` spends a round the
/// Reviewer asked for on something it never asked about — measured, and it is
/// how a run failed with a change it had already made twice discarded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageFeedback {
    /// Send the reason to the model and give it another turn.
    Send {
        reason: String,
        cause: FeedbackCause,
    },
    /// Out of allowance. The reason names the last cause so the message is
    /// about this run rather than about a constant.
    Stop {
        reason: String,
        cause: FeedbackCause,
    },
}

/// What went wrong, for the message the user eventually sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackCause {
    /// The Coder's artifact was well-formed but would not apply to the tree.
    UnappliablePatch,
    /// The model stopped at the token limit partway through an answer.
    Truncated,
    /// The model's output was not a usable artifact.
    MalformedArtifact,
}

impl FeedbackCause {
    fn describe(self) -> &'static str {
        match self {
            FeedbackCause::UnappliablePatch => "the patch did not apply",
            FeedbackCause::Truncated => "a response was cut off at the token limit",
            FeedbackCause::MalformedArtifact => "the model's output was not a usable artifact",
        }
    }
}

/// How many "try again with what went wrong" turns a stage gets, in total,
/// across every cause.
///
/// Two, and the number is the sum of what recovery actually needs rather than
/// a per-cause guess: the unappliable-patch case is the one that happens and
/// the one that can be fixed, and it was measured at two attempts before it
/// stopped being worth spending. A third turn is not free — each is a full
/// Coder call, tens of seconds and real tokens — and the evidence is that a
/// model whose anchor does not match does not start matching on the third try.
pub const MAX_STAGE_FEEDBACK: u32 = 2;

// The Reviewer's own budget is 3 by default. Sharing the whole of it would let
// mechanical failures consume the quality loop's allowance, which is the failure
// `MAX_PATCH_REPAIR_ATTEMPTS` existed to prevent and the reason it had to be
// strictly smaller than 3. Holding them equal reintroduces that.
const _: () = assert!(
    MAX_STAGE_FEEDBACK < 3,
    "the mechanical allowance must stay below the default revision budget of 3"
);

/// The running total of feedback turns a stage has had.
#[derive(Debug, Default, Clone, Copy)]
pub struct FeedbackBudget {
    used: u32,
}

impl FeedbackBudget {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold in recovery turns another component already spent.
    ///
    /// Raises the running total, never lowers it, so calling it each round with
    /// the loop's current count cannot hand the stage fresh turns. Starting a
    /// *new* budget from that count instead is the bug this exists to prevent.
    pub fn note_prior(&mut self, spent: u32) {
        self.used = self.used.max(spent);
    }

    /// Record one more attempt and say whether it is allowed.
    ///
    /// Counts the attempt, so the caller gets the same answer whether it is
    /// about to send feedback or to give up.
    pub fn take(&mut self, reason: &str, cause: FeedbackCause) -> StageFeedback {
        self.used += 1;
        if self.used <= MAX_STAGE_FEEDBACK {
            StageFeedback::Send {
                reason: reason.to_string(),
                cause,
            }
        } else {
            StageFeedback::Stop {
                reason: reason.to_string(),
                cause,
            }
        }
    }

    pub fn used(&self) -> u32 {
        self.used
    }
}

impl StageFeedback {
    /// The sentence a user reads when a run stops for this reason.
    pub fn stop_message(&self) -> Option<String> {
        match self {
            StageFeedback::Send { .. } => None,
            StageFeedback::Stop { reason, cause } => Some(format!(
                "stopped after {MAX_STAGE_FEEDBACK} attempts: {} — the last one said: {}",
                cause.describe(),
                first_sentence(reason)
            )),
        }
    }
}

/// The first sentence of a reason, which is usually the whole of it.
fn first_sentence(reason: &str) -> String {
    let trimmed = reason.trim();
    let end = trimmed.find(". ").map(|i| i + 1).unwrap_or(trimmed.len());
    trimmed[..end].to_string()
}

/// Why the revision loop must stop, even though a stage asked for a revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RevisionHold {
    /// There is something new to act on — run the next round.
    Continue,
    /// A revision was requested with nothing to act on.
    NoActionableIssues,
    /// A full round ran and came back with the same findings.
    UnchangedIssues,
}

/// A stable identity for a set of findings, used to recognise "the same
/// critique, again" — and to tell *having* findings from *having none*.
///
/// `None` means empty. Serialized rather than hand-built so a new field on an
/// issue type cannot silently stop participating in the comparison.
fn issue_signature<T: serde::Serialize>(issues: &[T]) -> Option<String> {
    if issues.is_empty() {
        return None;
    }
    Some(serde_json::to_string(issues).unwrap_or_default())
}

/// Decide whether another revision round is worth running.
///
/// A revision round costs a Coder, a Tester and a Reviewer — minutes and real
/// money — and it can only do what the critique tells it to. Two cases make that
/// spend pointless, and both were observed in live runs:
///
/// * **Nothing actionable.** The Reviewer returned `revision_needed` with an
///   empty `critical_issues` list (a 3B model does this readily: it declines to
///   approve without naming a blocker). The Coder is handed that, cannot act on
///   it, and re-emits a near-identical patch — which in one live run then failed
///   to parse and took down a task that had already passed its tests 5/5.
///
/// * **The same critique.** The round ran, and the next Reviewer returned a
///   byte-identical issue set. Nothing changed, so nothing would.
///
/// Stopping is not the same as approving. The verdict stays `RevisionNeeded`,
/// the run still reports `RevisionRequested`, and the branch is still produced
/// — the only thing that changes is that the harness stops paying for rounds
/// that cannot help.
fn revision_hold(previous: Option<&str>, current: Option<&str>) -> RevisionHold {
    let Some(current) = current.filter(|c| !c.is_empty()) else {
        return RevisionHold::NoActionableIssues;
    };
    if previous == Some(current) {
        return RevisionHold::UnchangedIssues;
    }
    RevisionHold::Continue
}

/// Fire one lifecycle hook, failing the run closed on Block.
/// No-ops (including unknown results) allow — hooks observe by default and
/// only an explicit block stops the pipeline.
fn fire_hook(
    bus: &crate::audit::HookBus,
    event: crate::audit::HookEvent,
    payload: serde_json::Value,
) -> Result<()> {
    if !bus.has_hooks(event) {
        return Ok(());
    }
    match bus.run(event, &payload.to_string()) {
        crate::audit::HookOutcome::Allow | crate::audit::HookOutcome::Noop => Ok(()),
        crate::audit::HookOutcome::Block(reason) => {
            anyhow::bail!("hook blocked {}: {}", event.as_str(), reason)
        }
    }
}

/// Payload for per-agent hook events.
fn agent_hook_payload(role: AgentRole, task_id: &Uuid, round: u32) -> serde_json::Value {
    serde_json::json!({
        "role": format!("{:?}", role),
        "task_id": task_id.to_string(),
        "round": round,
    })
}

/// Pick the agent topology for this run (BUILD_PLAN 3.2, P2.2).
///
/// `Auto` (the default) decides by task shape: a bounded/sequential task — one
/// whose `estimated_complexity` is at or below `single_agent_max_complexity` —
/// collapses to the single-agent fast-path, while anything bigger runs the full
/// multi-agent chain. The full chain is forced whenever the task needs an
/// independent security audit or parallel coders, since the solo fast-path
/// can't provide those.
pub fn select_topology(spec: &TaskSpec, config: &NikiConfig) -> TopologyMode {
    match config.pipeline.topology {
        TopologyMode::MultiAgent => TopologyMode::MultiAgent,
        TopologyMode::SingleAgent => TopologyMode::SingleAgent,
        TopologyMode::Auto => {
            if config.security.enabled || config.parallel.enabled {
                return TopologyMode::MultiAgent;
            }
            // Model capability is now an input, not an assumption.
            //
            // This used to look only at the task, so a low-complexity task
            // collapsed to a single agent regardless of what was running it. The
            // compute-matched ablation says that is backwards for a weak model:
            // below a 45% single-agent baseline a multi-agent pipeline is worth
            // about +22 points, and above 50% it costs about 5. NIKI's own
            // zero-setup path is a small local model, which is exactly the case
            // the old heuristic hurt most.
            //
            // `benefits_from_structure()` is deliberately asymmetric: an
            // unmeasured model is treated as weak, because the expensive
            // mistake is running without structure and watching the run die at
            // the Coder, not running with it and paying a few points.
            let capability = crate::config::capability::load(&config.project_dir_hint());
            if capability.benefits_from_structure() {
                return TopologyMode::MultiAgent;
            }
            let task_level = spec.estimated_complexity as u8;
            let threshold = config.pipeline.single_agent_max_complexity as u8;
            if task_level <= threshold {
                TopologyMode::SingleAgent
            } else {
                TopologyMode::MultiAgent
            }
        }
    }
}

/// Phase 5.7: Auto High/Security tiers force the full multi-agent chain so
/// risk-added stages (SecurityAuditor) survive the SingleAgent collapse.
/// Explicit `[pipeline].topology` is never overridden. Returns whether the
/// override fired (the caller upgrades the topology + reason).
pub fn force_multiagent_for_high_risk(
    topology: TopologyMode,
    configured: TopologyMode,
    level: crate::risk::RiskLevel,
) -> bool {
    matches!(configured, TopologyMode::Auto)
        && matches!(
            level,
            crate::risk::RiskLevel::High | crate::risk::RiskLevel::Security
        )
        && matches!(topology, TopologyMode::SingleAgent)
}

/// Human-readable reason for the topology decision, recorded in the task
/// record and report so an `Auto` collapse is self-describing, never silent
/// (goal-a3f9c2, Phase 3: the fast-path drops independent review, and the
/// user deserves to know why it was chosen).
pub fn topology_reason(spec: &TaskSpec, config: &NikiConfig) -> String {
    match config.pipeline.topology {
        TopologyMode::MultiAgent => "explicit [pipeline].topology = multiagent".to_string(),
        TopologyMode::SingleAgent => "explicit [pipeline].topology = singleagent: fast-path requested (Planner + solo Coder; no independent Tester/Reviewer/Red)".to_string(),
        TopologyMode::Auto => {
            if config.security.enabled || config.parallel.enabled {
                return "auto: security/parallel stages require the full multi-agent chain".to_string();
            }
            let capability = crate::config::capability::load(&config.project_dir_hint());
            if capability.benefits_from_structure() {
                return format!(
                    "auto: full multi-agent chain — model capability {}. A multi-agent pipeline \
                     is worth about +22 points to a weak model and costs about 5 to a strong \
                     one, so an unmeasured or weak model gets the structure.",
                    capability.explain()
                );
            }
            if (spec.estimated_complexity as u8)
                <= (config.pipeline.single_agent_max_complexity as u8)
            {
                format!(
                    "auto: estimated complexity {:?} <= max {:?} and the model measured strong — \
                     collapsed to fast-path (Planner + solo Coder; no independent \
                     Tester/Reviewer/Red)",
                    spec.estimated_complexity, config.pipeline.single_agent_max_complexity
                )
            } else {
                format!(
                    "auto: estimated complexity {:?} > max {:?}: full multi-agent chain",
                    spec.estimated_complexity, config.pipeline.single_agent_max_complexity
                )
            }
        }
    }
}
///
/// In `SingleAgent` mode only the `Coder` runs — the Tester, Reviewer, Red and
/// (if present) SecurityAuditor/Synthesizer stages are collapsed into the one
/// solo Coder session, which is the whole point of the fast-path: it avoids the
/// multi-agent token tax of re-ingesting shared context in every session.
/// The body stages (everything after the Planner) to run for a given topology.
///
/// In `SingleAgent` mode only the `Coder` runs — the Tester, Reviewer, Red and
/// (if present) SecurityAuditor/Synthesizer stages are collapsed into the one
/// solo Coder session, which is the whole point of the fast-path: it avoids the
/// multi-agent token tax of re-ingesting shared context in every session.
pub fn body_stages_for<'a>(
    topology: TopologyMode,
    stages: &[&'a PipelineStageConfig],
) -> Vec<&'a PipelineStageConfig> {
    match topology {
        TopologyMode::SingleAgent => stages
            .iter()
            .filter(|s| s.role == AgentRole::Coder)
            .copied()
            .collect(),
        TopologyMode::MultiAgent | TopologyMode::Auto => stages.to_vec(),
    }
}

/// The toolchain the sandbox image/process is expected to pre-bake. Verified up
/// front so a misconfigured environment fails fast instead of hanging on a runtime
/// install. The configured `extra_packages` are always included.
fn required_tools(config: &NikiConfig) -> Vec<String> {
    let mut required = config.docker.extra_packages.clone();
    for tool in ["git", "node", "npm", "python3"] {
        if !required.iter().any(|p| p == tool) {
            required.push(tool.to_string());
        }
    }
    required
}

/// A pipeline always needs a Planner to produce the spec; inject one if the
/// user's topology omitted it.
fn ensure_planner(
    stages: Vec<PipelineStageConfig>,
    config: &NikiConfig,
) -> Vec<PipelineStageConfig> {
    if !stages.iter().any(|s| s.role == AgentRole::Planner) {
        let agent = &config.agents.planner;
        let mut out = vec![PipelineStageConfig {
            role: AgentRole::Planner,
            provider: agent.provider.clone(),
            model: agent.model.clone(),
            skip: false,
            max_tokens: agent.effective_max_tokens(),
            temperature: agent.effective_temperature(),
            fallbacks: agent.fallbacks.clone(),
            reasoning_effort: None,
        }];
        out.extend(stages);
        out
    } else {
        stages
    }
}

fn provider_for(
    provider: &str,
    fallbacks: &[String],
    config: &NikiConfig,
) -> Result<Box<dyn LlmProvider>> {
    if fallbacks.is_empty() {
        // No fallbacks — plain provider.
        let cfg = config.providers.get(provider).ok_or_else(|| {
            crate::NikiError::Config(format!("Provider '{}' not configured", provider))
        })?;
        create_provider(provider, cfg)
    } else {
        // Build a failover chain: primary + fallbacks.
        crate::llm::failover::FailoverProvider::new(provider, fallbacks, &config.providers)
            .map(|p| Box::new(p) as Box<dyn LlmProvider>)
    }
}

/// Read the current on-disk contents of every file the spec wants to modify, so the
/// Coder can produce a diff that edits the existing code instead of recreating it.
fn build_current_files(spec: &TaskSpec, project_path: &Path) -> String {
    let mut out = String::new();
    for fc in &spec.files_to_modify {
        let p = project_path.join(&fc.path);
        match std::fs::read_to_string(&p) {
            Ok(content) => {
                out.push_str(&format!(
                    "### File: {} (action: {:?})\n```\n{}\n```\n\n",
                    fc.path, fc.action, content
                ));
            }
            Err(_) => {
                out.push_str(&format!(
                    "### File: {} (does not exist yet — will be created)\n\n",
                    fc.path
                ));
            }
        }
    }
    if out.is_empty() {
        out.push_str("(no files listed to modify)");
    }
    out
}

/// Resolve the security policy for a given agent role, falling back to the
/// default (deny-list only) policy when no role-specific policy is configured.
fn role_policy(role: AgentRole, config: &NikiConfig) -> SecurityPolicyConfig {
    let key = format!("{:?}", role).to_lowercase();
    config
        .security
        .policies
        .get(&key)
        .cloned()
        .unwrap_or_else(SecurityPolicyConfig::default)
}

/// Run `count` Coder agents concurrently (#3). Each coder is isolated in its
/// OWN git worktree sandbox so its changes can never collide with the others
/// (docker bind-mounts would share the host dir and conflict). Each coder's
/// patch is applied to its own worktree and its produced `CodeDiff` is
/// returned; the caller reconciles them through the Synthesizer stage.
///
/// Coders run as independent tokio tasks, each owning a forked `AgenticDisplay`
/// that forwards events to the single visible TUI. They share the provider and
/// the task spec, but no mutable pipeline state, so there is no contention.
#[allow(clippy::too_many_arguments)]
async fn run_parallel_coders(
    count: u32,
    coder_llm: Arc<dyn LlmProvider>,
    model: &str,
    provider: &str,
    task_spec: &TaskSpec,
    knowledge_str: &str,
    project_path: &Path,
    config: &NikiConfig,
    containers: ActiveContainers,
    _task_id: &Uuid,
    base_display: &AgenticDisplay,
    metrics: &mut Vec<StageMetric>,
    mcp_tools: &str,
    // Bare mode: skip project-memory injection in spawned coders.
    bare_memory: bool,
    // Lifecycle hooks bus (cloned per spawned coder: shell-outs are brief).
    hook_bus: crate::audit::HookBus,
    hook_task_id: Uuid,
) -> Result<Vec<CodeDiff>> {
    let event_tx = base_display
        .tui_tx()
        .unwrap_or_else(|| std::sync::mpsc::channel().0);
    let mut tasks = Vec::new();
    for _ in 0..count.max(1) {
        let llm = coder_llm.clone();
        let model = model.to_string();
        let provider = provider.to_string();
        let task_spec = task_spec.clone();
        let knowledge = knowledge_str.to_string();
        let project_path = project_path.to_path_buf();
        let config = config.clone();
        let containers = containers.clone();
        let coder_task_id = Uuid::new_v4();
        let mut disp = base_display.fork();
        let mcp_tools = mcp_tools.to_string();
        let event_tx = event_tx.clone();
        let hook_bus = hook_bus.clone();

        tasks.push(tokio::spawn(async move {
            // Own worktree sandbox per coder → isolated changes.
            let sandbox = create_sandbox(
                SandboxBackend::Worktree,
                None,
                AgentRole::Coder,
                &project_path,
                &coder_task_id,
                &config.docker,
                &config,
                role_policy(AgentRole::Coder, &config),
                containers,
                event_tx.clone(),
            )
            .await?;
            sandbox.ensure_tools(&required_tools(&config)).await?;

            let mut local_metrics: Vec<StageMetric> = Vec::new();
            let solo = run_role(
                AgentRole::Coder,
                &*llm,
                &model,
                &provider,
                &task_spec,
                "",
                "",
                "",
                "",
                "",
                0,
                &knowledge,
                &project_path,
                None,
                &mut disp,
                &mut local_metrics,
                0,   // max_tokens: use agent default
                0.0, // temperature: use agent default
                None,
                &mcp_tools,
                config_max_diff_lines(&config),
                bare_memory,
                &hook_bus,
                &hook_task_id,
                None,
            )
            .await?;
            let output = solo.output;

            let diff = match output {
                RoleOutput::Coder(d) => d,
                _ => unreachable!("coder stage yields a CodeDiff"),
            };
            // Apply to this coder's own worktree so `get_diff` reflects only its
            // change. A failed apply leaves the worktree holding nothing this
            // coder produced, so the diff read below would return the tree
            // unchanged and the synthesiser would merge an empty contribution
            // as if it were real work.
            sandbox
                .apply_patch(&code_diff_to_edit_text(&diff), &project_path)
                .await
                .with_context(|| {
                    format!(
                        "a parallel Coder's patch did not apply to its own worktree \
                         ({project_path:?}); its change was never written, so the run \
                         is stopped rather than merging a diff that does not exist"
                    )
                })?;
            let _wt_diff = sandbox
                .get_diff(
                    &diff
                        .files_changed
                        .iter()
                        .map(|f| f.path.clone())
                        .collect::<Vec<_>>(),
                )
                .await?;
            sandbox.destroy().await?;
            Ok::<_, anyhow::Error>((diff, local_metrics))
        }));
    }

    let mut out = Vec::new();
    for t in tasks {
        let (diff, local_metrics) = t
            .await
            .map_err(|e| anyhow::anyhow!("parallel coder task failed: {}", e))??;
        metrics.extend(local_metrics);
        out.push(diff);
    }
    Ok(out)
}

/// Experimental bounded research step (Phase 3.3, Layers 4+5).
///
/// Runs one `run_tool_loop` with the baseline registry before the Planner and
/// returns a context appendix plus a usage metric. Returns `None` when the
/// flag is off so the default pipeline stays byte-identical.
async fn run_experimental_research(
    llm: &dyn LlmProvider,
    model: &str,
    provider: &str,
    task: &Task,
    config: &NikiConfig,
    display: &mut AgenticDisplay,
    budget: Option<&mut super::budget::RunBudget>,
) -> Result<Option<(String, StageMetric)>> {
    if !config.tools.experimental_tool_loop {
        return Ok(None);
    }
    let start = Instant::now();
    let mut registry = crate::runtime::build_baseline_registry();
    // Phase 5.4: bound tool-loop hooks by the same `[hooks] timeout_seconds`.
    registry.set_hook_timeout_secs(config.hooks.timeout_seconds);
    let ctx = crate::runtime::ToolContext {
        agent_id: crate::mission::AgentId(format!("research-{}", task.id)),
        mission_id: crate::mission::MissionId(task.id.to_string()),
        role: "planner".into(),
        project_path: task.project_path.clone(),
        permissions: HashMap::new(),
        // Inherit the configured posture (manual fails closed headless).
        permission_mode: crate::runtime::ToolContext::parse_permission_mode(
            &config.permissions.mode,
        ),
        task_store: None,
    };
    let messages = vec![
        crate::runtime::LoopMessage::System(
            "You are a research assistant. Use the available tools to gather facts about the task, then summarize briefly.".into(),
        ),
        crate::runtime::LoopMessage::User(task.description.clone()),
    ];
    let out = crate::runtime::run_tool_loop(
        llm,
        model,
        &registry,
        &ctx,
        messages,
        None,
        config.tools.max_steps.max(1),
        display.tui_tx(),
        budget,
    )
    .await?;
    tracing::info!(
        target: "niki::pipeline",
        steps = out.steps,
        tool_calls = out.tool_calls.len(),
        input_tokens = out.usage.input_tokens,
        output_tokens = out.usage.output_tokens,
        "experimental research loop finished"
    );
    let cost_usd = compute_cost(provider, model, &out.usage);
    // Phase 3.5: loop usage lands in the existing StageDone/StageTotals
    // accounting via agent_done (muted-safe, TTY-safe).
    display.agent_done(
        AgentRole::Planner,
        vec![format!(
            "research loop: {} steps, {} tool calls",
            out.steps,
            out.tool_calls.len()
        )],
        out.usage,
        cost_usd,
    );
    let latency_ms = start.elapsed().as_millis() as u64;
    let appendix = format!(
        "## Tool Research (experimental loop, {} steps)\n{}",
        out.steps, out.content
    );
    let metric = StageMetric {
        role: AgentRole::Planner,
        provider: provider.to_string(),
        model: model.to_string(),
        input_tokens: out.usage.input_tokens,
        output_tokens: out.usage.output_tokens,
        cached_input_tokens: out.usage.cached_input_tokens,
        reasoning_tokens: out.usage.reasoning_tokens,
        latency_ms,
        cost_usd,
        retry_count: 0,
        ttft_ms: 0,
    };
    Ok(Some((appendix, metric)))
}

/// Run one agent: stream its output, measure latency, compute cost, record a
/// metric, and return the raw JSON artifact.
async fn run_stage(
    role: AgentRole,
    llm: &dyn LlmProvider,
    model: &str,
    provider: &str,
    template_name: &str,
    ctx: minijinja::Value,
    schema_path: &str,
    display: &mut AgenticDisplay,
    metrics: &mut Vec<StageMetric>,
    max_tokens: u32,
    temperature: f32,
    // Sent to the provider, not applied locally — see `AgentConfig::reasoning_effort`.
    reasoning_effort: Option<&str>,
    steer_rx: Option<&std::sync::Arc<std::sync::Mutex<Option<String>>>>,
) -> Result<String> {
    let start = Instant::now();
    let (json, usage, retry_count, ttft_ms) = run_agent(
        role,
        llm,
        model,
        template_name,
        ctx,
        schema_path,
        display,
        max_tokens,
        temperature,
        reasoning_effort,
        steer_rx,
    )
    .await?;
    let latency_ms = start.elapsed().as_millis() as u64;
    // Price against the provider that actually served. When the request fell
    // through to a fallback, `provider`/`model` are still the primary's, so
    // pricing with them charged fallback usage at the primary's rate — which
    // feeds the spend cap, the Cost page, the report and the JSON envelope.
    let served = llm.served_by();
    let served_provider: &str = served.as_deref().unwrap_or(provider);
    let cost_usd = compute_cost(served_provider, model, &usage);
    metrics.push(StageMetric {
        role,
        provider: served_provider.to_string(),
        model: model.to_string(),
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        cached_input_tokens: usage.cached_input_tokens,
        reasoning_tokens: usage.reasoning_tokens,
        latency_ms,
        cost_usd,
        retry_count,
        ttft_ms,
    });
    Ok(json)
}

/// Run the Coder on a tool loop instead of a single call.
///
/// Returns `None` when the model answered without submitting an artifact, which
/// is the signal to fall back to the one-shot path. That is not a failure mode
/// to paper over: a model that ignores the tool call and replies in prose is
/// exactly the case the old path handles, and the loop must never be worse than
/// the thing it replaced.
async fn run_coder_tool_loop(
    role: AgentRole,
    llm: &dyn LlmProvider,
    model: &str,
    provider: &str,
    template_name: &str,
    ctx: minijinja::Value,
    schema_path: &str,
    project_path: &Path,
    // The loop's own step budget bounds the spend, so the stage's
    // per-response  does not apply to it.
    _max_tokens: u32,
    display: &mut AgenticDisplay,
    metrics: &mut Vec<StageMetric>,
) -> Option<(String, u32)> {
    let schema_text = crate::load_asset(schema_path).ok()?;
    let schema_json: serde_json::Value = serde_json::from_str(&schema_text).ok()?;
    // `prompts/` matters, and getting it wrong is silent.
    //
    // `load_asset` resolves a *path*: it splits on the first `/` to decide
    // which embedded directory to look in, and a bare `coder.md` has no
    // directory, so the embedded lookup misses and the filesystem fallback
    // looks for `$CARGO_MANIFEST_DIR/coder.md`, which does not exist. The
    // `?` turned that into `None`, the caller read it as "the loop produced
    // no artifact", and the Coder silently fell through to the one-shot path.
    //
    // So the tool loop returned `None` on its second line, on every run,
    // since it was written — and nothing said so, because "no artifact" and
    // "the loop never started" are the same value. The tests claimed to cover
    // this were source-text greps, which cannot see a function that runs and
    // immediately gives up.
    let template = crate::load_asset(&format!("prompts/{template_name}")).ok()?;
    let mut env = minijinja::Environment::new();
    env.add_template("loop", &template).ok()?;
    let system_prompt = env.get_template("loop").ok()?.render(ctx).ok()?;

    let registry = crate::runtime::build_baseline_registry();
    let tool_ctx = crate::runtime::ToolContext {
        agent_id: crate::mission::AgentId(format!("coder-{}", project_path.display())),
        mission_id: crate::mission::MissionId(project_path.display().to_string()),
        role: "coder".into(),
        project_path: project_path.to_path_buf(),
        permissions: HashMap::new(),
        permission_mode: crate::runtime::ToolContext::parse_permission_mode("manual"),
        task_store: None,
    };

    let start = Instant::now();
    // Reject a bad submission *inside* the loop so the model can fix it with
    // its exploration still in context, rather than throwing the whole
    // exploration away and re-reading nothing in a fresh one-shot call.
    let schema_for_validation = schema_path.to_string();
    let validator: crate::runtime::ArtifactValidator =
        std::sync::Arc::new(move |value: &serde_json::Value| {
            let json = serde_json::to_string(value).map_err(|e| e.to_string())?;
            crate::artifacts::validate::validate_artifact(&json, &schema_for_validation)
                .map_err(|e| e.to_string())
        });
    let out = crate::runtime::run_tool_loop_with(
        crate::runtime::LoopOptions {
            submit_artifact: Some(crate::runtime::submit_artifact_spec(schema_json)),
            validate_artifact: Some(validator),
        },
        llm,
        model,
        &registry,
        &tool_ctx,
        vec![crate::runtime::LoopMessage::System(system_prompt)],
        None,
        // Enough to explore and then submit, and no more: a loop with no exit
        // is a spend cap with extra steps.
        12,
        display.tui_tx(),
        None,
    )
    .await
    .ok()?;

    // Every way the loop can come back empty has to say so.
    //
    // The notice used to fire only when the loop *submitted* something
    // invalid. A model that never called `submit_artifact` at all — which is
    // what a model told "respond with only the raw JSON artifact" does, while
    // being offered a tool loop — returned None with no message anywhere, and
    // the run quietly became the one-shot path. From the outside that is
    // indistinguishable from the loop not existing, which is how the tool
    // loop shipped with a prompt that contradicted it and nobody noticed.
    let Some(artifact) = out.artifact else {
        let msg = coder_loop_fallback_notice(&out);
        tracing::warn!(target: "niki::pipeline", role = "coder", steps = out.steps, "{}", msg);
        eprintln!("niki: {msg}");
        return None;
    };
    let json = serde_json::to_string_pretty(&artifact).ok()?;

    // Validate before accepting: a loop that produced something the stage cannot
    // parse is no better than the one-shot call failing — but the two are
    // *different* failures, and only the second one was visible. A live run
    // showed the loop submitting an artifact with an empty `search`, silently
    // falling back, and reporting the fallback's "model is too small" error —
    // which named neither the loop nor the fact that it had run.
    if let Err(e) = crate::artifacts::validate::validate_artifact(&json, schema_path) {
        // `tracing::warn` alone is not a diagnostic anyone will read: main
        // installs `EnvFilter::from_default_env()`, which is ERROR-only unless
        // RUST_LOG is set. So this also goes to stderr, where a person running
        // a CLI from a terminal is already looking, and to the TUI notice line
        // when there is one.
        let msg = format!(
            "the Coder explored with tools and submitted a CodeDiff that did not validate \
             ({e}); falling back to a single-shot call. The error that follows describes \
             the fallback, not this."
        );
        tracing::warn!(target: "niki::pipeline", role = "coder", error = %e, "{}", msg);
        eprintln!("niki: {msg}");
        return None;
    }

    let latency_ms = start.elapsed().as_millis() as u64;
    // How many "try again" turns the loop spent on its own recovery. Handed
    // back so the pipeline's shared allowance starts where this one left off.
    let feedback_turns = out.feedback_turns;
    let served = llm.served_by();
    let served_provider: &str = served.as_deref().unwrap_or(provider);
    metrics.push(StageMetric {
        role,
        provider: served_provider.to_string(),
        model: model.to_string(),
        input_tokens: out.usage.input_tokens,
        output_tokens: out.usage.output_tokens,
        cached_input_tokens: out.usage.cached_input_tokens,
        reasoning_tokens: out.usage.reasoning_tokens,
        latency_ms,
        cost_usd: compute_cost(served_provider, model, &out.usage),
        // The loop is the retry mechanism now: the model was allowed to correct
        // itself before it had to produce something final.
        retry_count: 0,
        ttft_ms: 0,
    });
    Some((json, feedback_turns))
}

/// What to tell the user when the Coder's tool loop produced no artifact.
///
/// Public so the wording is testable. That it reaches *stderr* rather than
/// only `tracing` is not unit-observable — `tracing` is invisible without
/// RUST_LOG — so `tests/agent_tool_loop.rs` checks the `eprintln!` directly,
/// and this checks that the message says something a user can act on.
pub fn coder_loop_fallback_notice(out: &crate::runtime::tools::LoopOutput) -> String {
    let used: Vec<&str> = out
        .tool_calls
        .iter()
        .filter(|(name, ok)| *ok && name != "submit_artifact")
        .map(|(name, _)| name.as_str())
        .collect();
    // What the model actually said.
    //
    // Without this the user is told the model is too small, which is a guess,
    // when the real answer is sitting in the response the harness already
    // holds: a small model asked to call a tool frequently answers in prose,
    // and prose is a completely different problem with a different fix. Five
    // breadth runs failed identically and identically uninformative.
    let said = out.content.trim();
    let excerpt = if said.is_empty() {
        "(it returned no text either)".to_string()
    } else {
        let flat: String = said.split_whitespace().collect::<Vec<_>>().join(" ");
        if flat.chars().count() > 240 {
            format!("{}…", flat.chars().take(240).collect::<String>())
        } else {
            flat
        }
    };
    format!(
        "the Coder ran the tool loop for {} step(s) and never called submit_artifact{}. \
         It {}{excerpt} \
         Falling back to a single-shot call. The error that follows describes the \
         fallback, not this.",
        out.steps,
        if used.is_empty() {
            " (it made no tool calls at all)".to_string()
        } else {
            format!(" (it did use: {})", used.join(", "))
        },
        // A truncated answer is a different problem with a different fix, and
        // the two used to be reported identically. "The model is too small"
        // sends a user to buy a different model; "the response was cut off at
        // the token limit" sends them to raise it.
        if out.truncated {
            "was CUT OFF at the token limit before it finished. It said: "
        } else {
            "said: "
        }
    )
}

/// Prompt template + JSON schema for a given role.
///
/// Public so `tests/embedded_assets.rs` can assert that every role's prompt and
/// schema actually resolve through `load_asset`. A typo in either path is
/// invisible until an agent silently receives an empty prompt in production.
pub fn role_prompt(role: AgentRole) -> (&'static str, &'static str) {
    match role {
        AgentRole::Planner => ("planner.md", "schemas/task_spec.schema.json"),
        AgentRole::Coder => ("coder.md", "schemas/code_diff.schema.json"),
        AgentRole::Tester => ("tester.md", "schemas/test_report.schema.json"),
        AgentRole::Reviewer => ("reviewer.md", "schemas/review_verdict.schema.json"),
        AgentRole::Synthesizer => ("synthesizer.md", "schemas/synthesis.schema.json"),
        AgentRole::SecurityAuditor => ("security_auditor.md", "schemas/security_audit.schema.json"),
        AgentRole::Red => ("red.md", "schemas/red_challenge.schema.json"),
        AgentRole::Critic => ("critic.md", "schemas/critique.schema.json"),
    }
}

/// Run a role-aware stage: build the role-specific prompt context, execute the
/// agent, parse the artifact, and render a summary for display.
///
/// What one stage produced.
///
/// A struct rather than the `(json, summary, output)` tuple this used to
/// return, so the Coder's tool loop can report how many recovery turns it
/// spent without becoming a fourth tuple element. It is the link between the
/// loop's own budget and the pipeline's shared one: both are "tell the model
/// what went wrong and try again", and two counters meant a stage could get
/// twice the allowance depending on which of them it hit.
pub struct RoleRun {
    pub json: String,
    pub summary: Vec<String>,
    pub output: RoleOutput,
    /// Recovery turns the stage spent on its own reasons, before the pipeline
    /// sees it. Zero for every role but the Coder, and zero for the Coder when
    /// it took the one-shot fallback.
    pub feedback_turns: u32,
}

/// Only body stages (Coder/Tester/Reviewer) flow through here; the Planner is
/// handled as the pipeline entry point in `execute_pipeline`.
#[allow(clippy::too_many_arguments)]
async fn run_role(
    role: AgentRole,
    llm: &dyn LlmProvider,
    model: &str,
    provider: &str,
    task_spec: &TaskSpec,
    coder_json: &str,
    tester_json: &str,
    red_json: &str,
    reviewer_json: &str,
    security_json: &str,
    round: u32,
    knowledge_str: &str,
    project_path: &Path,
    review_feedback: Option<&String>,
    display: &mut AgenticDisplay,
    metrics: &mut Vec<StageMetric>,
    max_tokens: u32,
    temperature: f32,
    // The provider's reasoning-effort control, forwarded untouched.
    reasoning_effort: Option<&str>,
    mcp_tools: &str,
    max_diff_lines: Option<usize>,
    // Bare mode: skip project-memory injection (ambient history off).
    bare_memory: bool,
    // Lifecycle hooks + owning task (PreAgentStart/PostAgentStop fire here,
    // so every body stage — including parallel coders — is covered).
    hooks: &crate::audit::HookBus,
    hook_task_id: &Uuid,
    steer_rx: Option<&std::sync::Arc<std::sync::Mutex<Option<String>>>>,
) -> Result<RoleRun> {
    // Recovery turns the Coder's tool loop spends on its own reasons, handed on
    // so the pipeline's shared allowance starts where the loop's left off.
    let mut feedback_turns: u32 = 0;
    let task_spec_json = serde_json::to_string_pretty(task_spec)?;
    let (template, schema) = role_prompt(role);

    // Load role-specific memory for prompt injection (absent when bare).
    // Phase 4.6: hierarchical memory (user > team > project) completes the
    // user/team → prompt loop; empty by default so default prompts are unchanged.
    let memory_str = if bare_memory {
        String::new()
    } else {
        crate::memory::render_hierarchical_memory(project_path, role, 10)
    };

    let ctx = match role {
        AgentRole::Coder => context! {
            input_artifacts => vec![task_spec_json.clone()],
            revision_context => review_feedback.cloned(),
            revision_round => round,
            project_knowledge => knowledge_str.to_string(),
            project_memory => memory_str,
            current_files => build_current_files(task_spec, project_path),
            mcp_tools => mcp_tools.to_string(),
            // The Coder may run as a tool loop instead of one call, and the
            // protocol is different: the answer is a `submit_artifact` call,
            // not prose. The prompt says "respond with only the raw JSON
            // artifact", so without this the model obeys the prompt, the loop
            // is offered a tool it was told not to use, and it returns prose
            // the loop cannot accept — silently, because nothing was
            // submitted, so the fallback had no reason to announce itself.
            // Measured: five breadth runs, zero tool calls, every one
            // falling back to the one-shot path without saying so.
            tool_loop => true,
        },
        AgentRole::Tester => context! {
            input_artifacts => vec![task_spec_json.clone(), coder_json.to_string()],
            project_knowledge => knowledge_str.to_string(),
            project_memory => memory_str,
            mcp_tools => mcp_tools.to_string(),
        },
        AgentRole::Reviewer => {
            // When the Red/Blue pass ran, the Reviewer must reconcile each Red
            // challenge. We append the Red artifact as a 4th input so the
            // Reviewer is forced to engage with the adversarial critique instead
            // of ratifying the Coder (guards sycophantic convergence, #1.2).
            //
            // The SecurityAuditor's findings go in the same way, and for the
            // same reason: an independent check that a reviewing agent cannot
            // see is not a check. The auditor runs first precisely so this is
            // populated by the time the Reviewer is called.
            let mut artifacts = vec![
                task_spec_json.clone(),
                coder_json.to_string(),
                tester_json.to_string(),
            ];
            if !red_json.is_empty() {
                artifacts.push(red_json.to_string());
            }
            // Files the change touches that the specification never named.
            //
            // Measured: a task about `src/broken.rs` produced a Coder artifact
            // that created `src/search.rs` — a file the task never mentioned and
            // the Coder invented — and the run carried on, because the artifact
            // schema has no opinion on scope. The Reviewer is the stage that
            // judges whether a change is in scope, and it cannot judge what it
            // is never shown.
            //
            // Not an error: touching an adjacent file is often right, and
            // failing the run for it would be worse than saying so. It is
            // named here so the judgement is actually available to the stage
            // that makes it.
            if let Some(note) = scope_drift_note(task_spec, coder_json) {
                artifacts.push(note);
            }
            let diff_guardrail_hint = max_diff_lines.and_then(|m| {
                if m > 0 {
                    Some(format!(
                        "Diff-size guardrail is active (`general.max_diff_lines = {}`): lean toward tighter, \
                         more reviewable deltas and flag oversized changes as a review concern.",
                        m
                    ))
                } else {
                    None
                }
            });
            context! {
                input_artifacts => artifacts,
                // Red and Security ride as named optional artifacts, not as
                // positional entries. Appending to `input_artifacts` made the
                // prompt's `input_artifacts[3]` mean "Red" only when Red ran
                // and nothing else did — so a fourth artifact silently
                // re-pointed the template at the wrong JSON.
                red_artifact => red_json.to_string(),
                security_artifact => security_json.to_string(),
                project_knowledge => knowledge_str.to_string(),
                project_memory => memory_str,
                diff_guardrail_hint => diff_guardrail_hint.clone(),
                mcp_tools => mcp_tools.to_string(),
            }
        }
        AgentRole::Red => context! {
            // The Red agent sees the same inputs as the Reviewer (spec + diff +
            // tests) but only the *evidence* of the Coder's diff — rationale
            // fields (implementation_notes, spec_adherence, uncertainties) are
            // withheld (see `red_evidence_json`), so Red probes adversarially
            // instead of being framed by the Coder's self-justification.
            input_artifacts => vec![task_spec_json.clone(), red_evidence_json(coder_json), tester_json.to_string()],
            project_knowledge => knowledge_str.to_string(),
            project_memory => memory_str,
            mcp_tools => mcp_tools.to_string(),
        },
        AgentRole::Synthesizer => context! {
            // In the parallel-coder flow (#3) `coder_json` carries every coder
            // diff concatenated; the Synthesizer reconciles them into one change.
            input_artifacts => vec![task_spec_json.clone(), coder_json.to_string()],
            project_knowledge => knowledge_str.to_string(),
            project_memory => memory_str,
            mcp_tools => mcp_tools.to_string(),
        },
        AgentRole::SecurityAuditor => context! {
            input_artifacts => vec![task_spec_json.clone(), coder_json.to_string()],
            project_knowledge => knowledge_str.to_string(),
            project_memory => memory_str,
            mcp_tools => mcp_tools.to_string(),
        },
        AgentRole::Critic => {
            // Narrow meta-verifier: the spec, the Coder's evidence (never its
            // rationale), the Tester report, and the Reviewer verdict under
            // test — plus Red when that pass ran.
            let mut artifacts = vec![
                task_spec_json.clone(),
                red_evidence_json(coder_json),
                tester_json.to_string(),
                reviewer_json.to_string(),
            ];
            if !red_json.is_empty() {
                artifacts.push(red_json.to_string());
            }
            context! {
                input_artifacts => artifacts,
                red_artifact => red_json.to_string(),
                security_artifact => security_json.to_string(),
                project_knowledge => knowledge_str.to_string(),
                project_memory => memory_str,
                mcp_tools => mcp_tools.to_string(),
            }
        }
        AgentRole::Planner => {
            // Should never happen — the Planner is run separately. Keep the
            // match exhaustive and surface a clear error if it does.
            return Err(
                crate::NikiError::Config("Planner must not run as a body stage".into()).into(),
            );
        }
    };

    fire_hook(
        hooks,
        crate::audit::HookEvent::PreAgentStart,
        agent_hook_payload(role, hook_task_id, round),
    )?;

    // The Coder can be a loop instead of a single call.
    //
    // Every role here used to be one streaming call that had to emit a whole
    // validated artifact blind. The Coder is the stage that suffers most: it is
    // the one that needs to *read* the file it is editing, and it was being
    // handed a copy of it. The loop lets it read, grep, run and edit, and then
    // call `submit_artifact` with the typed artifact — the same schema, the
    // same audit trail, reached after exploring rather than guessed blind.
    //
    // Falling back to the one-shot path when the loop yields no artifact. A
    // model that ignores the tool and answers in prose must not be worse off
    // than it was before, so the fallback is the old behaviour verbatim rather
    // than an error — the loop can only add capability, never take it away.
    let json = if role == AgentRole::Coder {
        match run_coder_tool_loop(
            role,
            llm,
            model,
            provider,
            template,
            ctx.clone(),
            schema,
            project_path,
            max_tokens,
            display,
            metrics,
        )
        .await
        {
            Some((json, turns)) => {
                feedback_turns = turns;
                json
            }
            None => {
                run_stage(
                    role,
                    llm,
                    model,
                    provider,
                    template,
                    ctx,
                    schema,
                    display,
                    metrics,
                    max_tokens,
                    temperature,
                    reasoning_effort,
                    steer_rx,
                )
                .await?
            }
        }
    } else {
        run_stage(
            role,
            llm,
            model,
            provider,
            template,
            ctx,
            schema,
            display,
            metrics,
            max_tokens,
            temperature,
            reasoning_effort,
            steer_rx,
        )
        .await?
    };
    let output = parse_role(role, &json)?;
    let summary = match &output {
        RoleOutput::Planner(s) => crate::display::artifact_render::render_task_spec_summary(s),
        RoleOutput::Coder(d) => crate::display::artifact_render::render_code_diff_summary(d),
        RoleOutput::Tester(t) => crate::display::artifact_render::render_test_report_summary(t),
        RoleOutput::Reviewer(v) => {
            crate::display::artifact_render::render_review_verdict_summary(v)
        }
        RoleOutput::Synthesizer(s) => crate::display::artifact_render::render_synthesis_summary(s),
        RoleOutput::SecurityAuditor(v) => {
            crate::display::artifact_render::render_security_verdict_summary(v)
        }
        RoleOutput::Red(v) => crate::display::artifact_render::render_red_challenge_summary(v),
        RoleOutput::Critic(v) => crate::display::artifact_render::render_critique_summary(v),
    };
    fire_hook(
        hooks,
        crate::audit::HookEvent::PostAgentStop,
        agent_hook_payload(role, hook_task_id, round),
    )?;
    Ok(RoleRun {
        json,
        summary,
        output,
        feedback_turns,
    })
}

pub fn parse_role(role: AgentRole, json: &str) -> Result<RoleOutput> {
    Ok(match role {
        AgentRole::Planner => RoleOutput::Planner(serde_json::from_str(json)?),
        AgentRole::Coder => RoleOutput::Coder(serde_json::from_str(json)?),
        AgentRole::Tester => RoleOutput::Tester(serde_json::from_str(json)?),
        AgentRole::Reviewer => RoleOutput::Reviewer(serde_json::from_str(json)?),
        AgentRole::Synthesizer => RoleOutput::Synthesizer(serde_json::from_str(json)?),
        AgentRole::SecurityAuditor => RoleOutput::SecurityAuditor(serde_json::from_str(json)?),
        AgentRole::Red => RoleOutput::Red(serde_json::from_str(json)?),
        AgentRole::Critic => RoleOutput::Critic(serde_json::from_str(json)?),
    })
}

/// Run one post-loop stage (the Critic, or the single Critic-forced Reviewer
/// retry) with the same bookkeeping as loop stages: metrics, artifacts,
/// isolation record, display, spend cap, and incremental task record.
#[allow(clippy::too_many_arguments)]
async fn run_bookkept_stage(
    stage: &PipelineStageConfig,
    llm: &dyn LlmProvider,
    task_spec: &TaskSpec,
    coder_json: &str,
    tester_json: &str,
    red_json: &str,
    reviewer_json: &str,
    security_json: &str,
    round: u32,
    knowledge_str: &str,
    project_path: &Path,
    review_feedback: Option<&String>,
    display: &mut AgenticDisplay,
    metrics: &mut Vec<StageMetric>,
    artifacts: &mut Vec<(AgentRole, String)>,
    isolation: &mut Vec<IsolationRecord>,
    mcp_tools: &str,
    config: &NikiConfig,
    hook_bus: &crate::audit::HookBus,
    hook_task_id: &Uuid,
    task: &Task,
    task_dir: &Path,
    state: &mut super::state::PipelineState,
    bare: bool,
    // Whether this run actually has a SecurityAuditor stage. Taken from the
    // resolved stage list rather than `config.security.enabled` because a
    // High/Security-risk task gets one injected whether or not the config
    // asked for it — the isolation record has to reflect the run, not the
    // request.
    security_enabled: bool,
    steer_rx: Option<&std::sync::Arc<std::sync::Mutex<Option<String>>>>,
) -> Result<(String, RoleOutput)> {
    let stage_run = run_role(
        stage.role,
        llm,
        &stage.model,
        &stage.provider,
        task_spec,
        coder_json,
        tester_json,
        red_json,
        reviewer_json,
        security_json,
        round,
        knowledge_str,
        project_path,
        review_feedback,
        display,
        metrics,
        stage.max_tokens,
        stage.temperature,
        stage.reasoning_effort.as_deref(),
        mcp_tools,
        config_max_diff_lines(config),
        bare,
        hook_bus,
        hook_task_id,
        steer_rx,
    )
    .await?;
    let RoleRun {
        json,
        summary,
        output: role_output,
        ..
    } = stage_run;
    artifacts.push((stage.role, json.clone()));
    isolation.push(IsolationRecord {
        role: stage.role,
        backend: config.docker.backend,
        context_sources: isolation_sources_for(
            stage.role,
            config.red_blue.enabled,
            security_enabled,
        ),
        saw_other_reasoning: false,
    });
    // Copy the values out: `metrics` is about to be borrowed mutably below,
    // and the old code held a reference to its last element across that call.
    // An empty metrics list was an `unreachable!` panic; it is now a zero-cost
    // stage, which is the honest reading.
    let (usage, cost) = metrics
        .last()
        .map(|m| (m.usage(), m.cost_usd))
        .unwrap_or((crate::llm::provider::TokenUsage::default(), 0.0));
    display.agent_done(stage.role, summary, usage, cost);
    finish_stage(
        display,
        metrics,
        state,
        config,
        task,
        task_dir,
        project_path,
        round,
    )?;
    Ok((json, role_output))
}

/// Hard-enforce the per-run spend cap. Returns an error that aborts the pipeline
/// before any further stages run (and therefore before a branch is produced) once
/// the cumulative estimated cost of completed stages exceeds `[general] spend_cap_usd`.
/// This turns the previously warn-only cap into a real stop so autonomous runs can't
/// run away on cost (launch-plan B2).
fn enforce_spend_cap(spend_cap: f64, metrics: &[StageMetric]) -> Result<()> {
    if spend_cap <= 0.0 {
        return Ok(());
    }
    let total: f64 = metrics.iter().map(|m| m.cost_usd).sum();
    if total > spend_cap {
        anyhow::bail!(
            "spend cap exceeded — estimated ${:.4} > cap ${:.2}. \
             The run was stopped before any branch was created. \
             Lower the task scope or raise [general] spend_cap_usd.",
            total,
            spend_cap
        );
    }
    Ok(())
}

/// Translate `[general] max_diff_lines` into the `Option<usize>` the body stages expect.
/// `0` means "off" (None); any positive value passes through, enabling the Reviewer
/// diff-size nudge and the post-run guardrail rendered in report.md.
fn config_max_diff_lines(config: &NikiConfig) -> Option<usize> {
    let lines = config.general.max_diff_lines;
    if lines > 0 {
        Some(lines as usize)
    } else {
        None
    }
}

/// T7: Update the context budget from accumulated metrics, write context.json,
/// and auto-compact when the session-switch threshold is crossed.
/// Lowest-priority sections compress first (history, external sources, older
/// memory); what was dropped is recorded in `context.json`. When compaction is
/// off, an explicit warning is emitted and the run continues.
fn update_context_budget(
    metrics: &[StageMetric],
    state: &mut super::state::PipelineState,
    project_path: &Path,
    task_dir: &Path,
    config: &NikiConfig,
) -> Result<()> {
    let total: u32 = metrics.iter().map(|m| m.total_tokens()).sum();
    state.context_budget.used = total;
    let past_threshold = state.context_budget.needs_session_switch();

    if past_threshold {
        if config.compaction.enabled && config.compaction.auto_compact {
            let dropped = vec!["history", "external_sources", "older_memory"];
            let _ = crate::memory::compression::compress_context(
                project_path,
                AgentRole::Planner,
                crate::memory::compression::CompressionStrategy::KnowledgeBlock,
                format!(
                    "Pipeline context at {:.1}% budget ({}k/{}k tokens used).",
                    state.context_budget.fill_ratio() * 100.0,
                    state.context_budget.used / 1000,
                    state.context_budget.capacity / 1000,
                ),
                vec![format!(
                    "Total tokens consumed: {}",
                    state.context_budget.used
                )],
                vec!["Pipeline auto-compaction triggered by context budget threshold.".to_string()],
                vec![],
                Some(state.context_budget.used),
            );
            let ctx_json = serde_json::json!({
                "used": state.context_budget.used,
                "capacity": state.context_budget.capacity,
                "fill_ratio": state.context_budget.fill_ratio(),
                "should_compress": state.context_budget.should_compress(),
                "needs_session_switch": true,
                "compacted": true,
                "dropped_sections": dropped,
            });
            return crate::knowledge::kb::write_atomic(
                &task_dir.join("context.json"),
                serde_json::to_string_pretty(&ctx_json)?.as_bytes(),
            )
            .with_context(|| {
                format!(
                    "could not write the context snapshot to {}",
                    task_dir.join("context.json").display()
                )
            });
        }
        eprintln!(
            "Warning: context budget past threshold ({:.1}% of {} tokens) with compaction off — continuing without compression.",
            state.context_budget.fill_ratio() * 100.0,
            state.context_budget.capacity
        );
        tracing::warn!(
            target: "niki::pipeline",
            fill = state.context_budget.fill_ratio(),
            "context budget past threshold, compaction off"
        );
    }

    let ctx_json = serde_json::json!({
        "used": state.context_budget.used,
        "capacity": state.context_budget.capacity,
        "fill_ratio": state.context_budget.fill_ratio(),
        "should_compress": state.context_budget.should_compress(),
        "needs_session_switch": past_threshold,
        "compacted": false,
    });
    // Was two discarded `let _ =`s. `context.json` is what a run's context
    // pressure story is reconstructed from, and a silent write failure left a
    // stale or absent file that reads as "this run never used much context".
    // Atomic, and propagated.
    crate::knowledge::kb::write_atomic(
        &task_dir.join("context.json"),
        serde_json::to_string_pretty(&ctx_json)?.as_bytes(),
    )
    .with_context(|| {
        format!(
            "could not write the context snapshot to {}",
            task_dir.join("context.json").display()
        )
    })
}

/// The bookkeeping every finished stage owes the run, in one place.
///
/// Seven call sites spelled this out separately, in two different orders, and
/// two of them — the Planner's and the Synthesizer's — had dropped
/// `enforce_spend_cap` entirely. Those paths did not bypass the cap; they
/// enforced it one stage late, so a run could overshoot by the cost of a whole
/// stage before anything noticed. "What counts as complete for a stage" also
/// had as many answers as there were call sites, which is how cost and usage
/// drift out of agreement with what actually ran.
///
/// `round` is the revision round to record; stages that run outside a loop
/// pass 0.
#[allow(clippy::too_many_arguments)]
fn finish_stage(
    display: &mut crate::display::agent_stream::AgenticDisplay,
    metrics: &mut Vec<StageMetric>,
    state: &mut super::state::PipelineState,
    config: &NikiConfig,
    task: &Task,
    task_dir: &Path,
    project_path: &Path,
    round: u32,
) -> Result<()> {
    display.update_pipeline_status();
    enforce_spend_cap(config.general.spend_cap_usd, metrics)?;
    state.accrue_budget(metrics)?;
    update_context_budget(metrics, state, project_path, task_dir, config)?;
    save_task_record(task, metrics, TaskStatus::Running, task_dir, round)?;
    Ok(())
}

/// Record one stage's isolation provenance.
///
/// Part of the same duplication: every stage site rebuilt the identical
/// record, so the table in the report was assembled by copy-paste and a
/// stage that forgot it would silently drop out of the isolation proof.
fn record_isolation(
    isolation: &mut Vec<IsolationRecord>,
    role: AgentRole,
    config: &NikiConfig,
    security_enabled: bool,
) {
    isolation.push(IsolationRecord {
        role,
        backend: config.docker.backend,
        context_sources: isolation_sources_for(role, config.red_blue.enabled, security_enabled),
        saw_other_reasoning: false,
    });
}

/// T8: Save an incremental TaskRecord snapshot to disk.
///
/// Fail-closed. This used to discard the write error (`let _ = ...`), so a run
/// whose state could not be persisted carried on to completion and reported
/// success — leaving the user a branch with no record of what produced it, no
/// cost accounting, and nothing to resume from. Every caller now propagates.
fn save_task_record(
    task: &Task,
    metrics: &[StageMetric],
    status: TaskStatus,
    task_dir: &Path,
    round: u32,
) -> Result<()> {
    let mut rec = TaskRecord::new(task.id, &task.description);
    rec.status = status;
    rec.revision_rounds = round;
    rec.add_metrics(metrics);
    rec.save_to_disk(task_dir).with_context(|| {
        format!(
            "could not persist the run record to {} — stopping rather than \
             completing a run whose state cannot be read back",
            task_dir.join("task.json").display()
        )
    })
}

/// Project memory for prompt injection: ambient history, present by default,
/// absent when bare (`--bare` = no ambient inputs). Phase 4.6: budget-aware
/// rendering wires the compaction readers — full entries by default, trimmed
/// to compressed knowledge under context pressure.
fn memory_for_role(
    project_path: &Path,
    role: AgentRole,
    bare: bool,
    budget: &crate::memory::ContextBudget,
) -> String {
    if bare {
        String::new()
    } else {
        crate::memory::render_memory_with_budget(project_path, role, budget)
    }
}

pub async fn execute_pipeline(
    task: &Task,
    config: &NikiConfig,
    docker: Option<&bollard::Docker>,
    display: &mut AgenticDisplay,
    containers: ActiveContainers,
    dry_run: bool,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    task_dir: &Path,
    // Pre-approved plan: validated `planner.json` from `niki plan`, reviewed
    // by the user. When `Some`, the Planner LLM call is skipped and this spec
    // drives the run. `None` runs the Planner normally.
    plan_override_json: Option<String>,
    // Bare mode (`niki run --bare`): deterministic-inputs mode for CI.
    // Skips project memory injection, MCP tool discovery, and external
    // knowledge-URL fetching, so runs depend only on the repo + config.
    // Model sampling nondeterminism and failover retries still apply — bare
    // means "no ambient inputs", not "bit-identical output".
    bare: bool,
) -> Result<PipelineResult> {
    // In bare mode, strip the ambient-input config up front so every
    // downstream consumer (indexer, memory, MCP) sees the same story.
    let bare_config: Option<NikiConfig>;
    let config: &NikiConfig = if bare {
        let mut stripped = config.clone();
        stripped.knowledge.urls.clear();
        bare_config = Some(stripped);
        bare_config.as_ref().unwrap()
    } else {
        config
    };
    // 1. Index Project — fail-soft by contract (Mantis "never fail, produce
    // an empty index"): indexing informs the Planner but must never abort the
    // run. Opt out via `[repo_intel] on_failure = "fail"`.
    let knowledge = match index_project(&task.project_path, config).await {
        Ok(k) => k,
        Err(e) => {
            if config.repo_intel.on_failure == "fail" {
                return Err(e);
            }
            eprintln!("Warning: project indexing failed ({e}); continuing with an empty index");
            crate::knowledge::ProjectKnowledge {
                file_tree: String::new(),
                detected_languages: Vec::new(),
                package_info: Vec::new(),
                git_recent_commits: Vec::new(),
                skills_files: Vec::new(),
                project_size: crate::knowledge::ProjectSize::Small,
                external_sources: Vec::new(),
                standing_rules: crate::knowledge::indexer::load_standing_rules(&task.project_path),
                agents_md: String::new(),
            }
        }
    };
    let knowledge_str = knowledge.render();

    // Shell-hook bus from `[hooks]` config. Wired subset: PreTaskStart,
    // PostTaskStop, PreAgentStart, PostAgentStop. A Block outcome aborts the
    // run fail-closed — hooks are policy, and policy violations must not be
    // advisory. (PreToolUse/PostToolUse fire inside the runtime tool loop.)
    let mut hook_bus = crate::audit::HookBus::from_map(&config.hooks.commands);
    // Phase 5.4: bound every hook by `[hooks] timeout_seconds`.
    hook_bus.set_timeout_secs(config.hooks.timeout_seconds);
    fire_hook(
        &hook_bus,
        crate::audit::HookEvent::PreTaskStart,
        serde_json::json!({"task_id": task.id.to_string(), "description": task.description}),
    )?;

    let mut state = super::state::PipelineState::new(task.id);
    state
        .context_budget
        .apply_compaction_config(&config.compaction);
    // Phase 5.5: resolve the unified run budget (steps/cost/wallclock) once;
    // every stage boundary accrues into it via `accrue_budget`.
    state.run_budget = super::budget::RunBudget::resolve(config);
    let mut metrics: Vec<StageMetric> = Vec::new();

    // Initialize AgentRuntime and start the persistent AgentSession
    let agent_runtime = crate::runtime::AgentRuntime::new(config.clone());
    let journal_sink = Arc::new(crate::runtime::JournalEventSink::new(
        task_dir.join("events.jsonl"),
    ));
    let mut runtime_session = match agent_runtime
        .start_session(
            task.project_path.clone(),
            task.id,
            task.description.clone(),
            Some(journal_sink),
            Some(crate::runtime::CancellationToken::from_atomic(
                cancel.clone(),
            )),
        )
        .await
    {
        Ok(s) => Some(s),
        Err(e) => {
            tracing::warn!(target: "niki::pipeline", "Failed to start agent session: {e}");
            None
        }
    };

    if let Some(ref s) = runtime_session {
        let prewarm = s.prewarm().await;
        tracing::info!(
            target: "niki::pipeline",
            prewarm_ms = prewarm.prewarm_ms,
            "AgentRuntime prewarmed resources"
        );
    }

    // T8: Save an early TaskRecord (Running) so a crash mid-pipeline still
    // leaves a status file on disk.
    save_task_record(task, &metrics, TaskStatus::Running, task_dir, 0)?;

    let mut artifacts: Vec<(AgentRole, String)> = Vec::new();
    // Per-agent context-isolation records (BUILD_PLAN 2.1). Populated as each
    // stage runs so the report can prove every agent was an independent session.
    let mut isolation: Vec<IsolationRecord> = Vec::new();

    // --- MCP tool discovery (optional, launch-plan C1) ---
    // When `[mcp] enabled = true`, connect configured servers now and surface their
    // tools to every agent via the prompt context. Skipped entirely when bare:
    // external servers are ambient inputs. The manager is wired into the
    // runtime here; the agent→server tool-call execution loop remains a follow-up.
    let mcp_tools: String = if bare {
        String::new()
    } else if config.mcp.enabled {
        let mut mgr = crate::mcp::McpManager::new();
        if let Err(e) = mgr.connect_all().await {
            eprintln!("Warning: MCP connect failed: {}", e);
        }
        mgr.tools_for_prompt()
    } else {
        String::new()
    };

    let event_tx = display.tui_tx().unwrap_or_else(|| {
        let (tx, _) = std::sync::mpsc::channel();
        tx
    });

    // T12: Create the /steer correction channel using a shared Arc<Mutex<Option<String>>>.
    // The Arc goes to the TUI (via DisplayEvent) so the chat page can write user
    // corrections; a clone is polled inside run_agent's streaming loop.
    let steer_state: std::sync::Arc<std::sync::Mutex<Option<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let _ = event_tx.send(crate::display::tui::DisplayEvent::SteerChannel(
        steer_state.clone(),
    ));
    let steer_rx = Some(&steer_state);

    // Resolve the ordered, data-driven stage list.
    let stages = ensure_planner(resolve_stages(config), config);

    // Provenance anchor: snapshot the repo/config state this run reasons
    // about, before the Planner executes. Best-effort by design — a manifest
    // write failure warns and never fails the run.
    let mut run_manifest =
        super::provenance::capture(config, &task.project_path, &task.id, &stages);
    if config.snapshot.enabled {
        if let Err(e) = super::provenance::write_manifest(task_dir, &run_manifest) {
            eprintln!("Warning: could not write run manifest: {e}");
        }
        if let Some(tasks_dir) = task_dir.parent() {
            super::provenance::prune_stale_snapshots(tasks_dir, config.snapshot.retention_days);
        }
    }

    // History mining is deterministic and cached (known commits are skipped),
    // so it runs every time without an LLM. Failures warn and never fail the
    // run; outside a git repo the miner records `unsupported` and stops.
    if config.repo_intel.enabled && config.repo_intel.history {
        let outcome = crate::knowledge::history::mine_history(
            &task.project_path,
            config,
            &run_manifest.active_snapshot.snapshot_id,
        );
        if outcome.invalidated {
            eprintln!("Note: git history was rewritten — history cache invalidated and rebuilt");
        }
    }

    // --- Planner (entry point) ---
    // An approved plan (`niki run --plan <id>`) skips the Planner LLM call:
    // the user-reviewed spec drives the run directly. The JSON is re-validated
    // here so stale or hand-edited plans fail fast instead of confusing stages.
    // Metrics stay empty for the skipped stage so reported costs remain honest.
    let planner_json: String = if let Some(approved) = plan_override_json.as_ref() {
        let _: TaskSpec = serde_json::from_str(approved).map_err(|e| {
            crate::NikiError::Config(format!("--plan artifact is not a valid TaskSpec: {e}"))
        })?;
        tracing::info!(target: "niki::pipeline", "using approved plan, Planner LLM call skipped");
        approved.clone()
    } else {
        fire_hook(
            &hook_bus,
            crate::audit::HookEvent::PreAgentStart,
            agent_hook_payload(AgentRole::Planner, &task.id, 0),
        )?;
        let planner_stage = stages
            .iter()
            .find(|s| s.role == AgentRole::Planner && !s.skip)
            .ok_or_else(|| crate::NikiError::Config("No Planner stage configured".to_string()))?;
        let planner_llm = provider_for(&planner_stage.provider, &planner_stage.fallbacks, config)?;

        // Planner context: the bounded pack (repo manifest + KB + symbol
        // excerpts + learnings) when repo intelligence is on; the legacy full
        // index render otherwise. Role memory injection is unchanged.
        let mut planner_context = if config.repo_intel.enabled {
            let repo_manifest = crate::repo_intel::build_manifest(&task.project_path, config);
            crate::knowledge::context_pack::build_context_pack(
                &task.project_path,
                config,
                &task.description,
                &repo_manifest,
            )
        } else {
            knowledge_str.clone()
        };

        // Phase 3.3: experimental bounded research loop (default off → the
        // context below is byte-identical to a flag-off run).
        let mut research_metric: Option<StageMetric> = None;
        if config.tools.experimental_tool_loop {
            if let Some((appendix, metric)) = run_experimental_research(
                planner_llm.as_ref(),
                &planner_stage.model,
                &planner_stage.provider,
                task,
                config,
                display,
                Some(&mut state.run_budget),
            )
            .await?
            {
                planner_context = format!("{planner_context}\n{appendix}");
                research_metric = Some(metric);
            }
        }

        let planned =
run_stage(
            AgentRole::Planner,
            planner_llm.as_ref(),
            &planner_stage.model,
            &planner_stage.provider,
            "planner.md",
            context! {
                task_description => task.description.clone(),
                project_knowledge => planner_context,
                project_memory => memory_for_role(&task.project_path, AgentRole::Planner, bare, &state.context_budget),
                mcp_tools => mcp_tools.clone(),
            },
            "schemas/task_spec.schema.json",
            display,
            &mut metrics,
            planner_stage.max_tokens,
            planner_stage.temperature,
            planner_stage.reasoning_effort.as_deref(),
            steer_rx,
        )

        .await?;
        // Phase 5.5: accrue the Planner stage (and any earlier unaccounted
        // metric) at this boundary — unconditional, not just when the
        // research loop ran.
        state.accrue_budget(&metrics)?;
        if let Some(metric) = research_metric {
            // Order the research step before the Planner call it informed.
            // The inserted research metric must not accrue again: mark
            // everything accounted (its loop already accrued per-iteration).
            if let Some(pos) = metrics.len().checked_sub(1) {
                metrics.insert(pos, metric);
                state.run_budget.account_metrics_up_to(metrics.len());
            }
        }
        planned
    };
    let task_spec: TaskSpec = serde_json::from_str(&planner_json)?;
    artifacts.push((AgentRole::Planner, planner_json.clone()));
    isolation.push(IsolationRecord {
        role: AgentRole::Planner,
        backend: config.docker.backend,
        // The Planner is the entry point and sees nothing, so its isolation
        // record is empty regardless; `security_enabled` is not yet computed
        // this early, and passing a value the arm ignores would only invite
        // someone to start believing it.
        context_sources: isolation_sources_for(AgentRole::Planner, config.red_blue.enabled, false),
        saw_other_reasoning: false,
    });
    // Approved-plan runs skip the Planner LLM call, so no metric exists for
    // this stage — report zero usage rather than crashing on the assumption
    // that the Planner always ran.
    let pm = metrics.last().cloned().unwrap_or_else(|| StageMetric {
        role: AgentRole::Planner,
        provider: String::new(),
        model: String::new(),
        input_tokens: 0,
        output_tokens: 0,
        cached_input_tokens: 0,
        reasoning_tokens: 0,
        latency_ms: 0,
        cost_usd: 0.0,
        retry_count: 0,
        ttft_ms: 0,
    });
    display.agent_done(
        AgentRole::Planner,
        crate::display::artifact_render::render_task_spec_summary(&task_spec),
        pm.usage(),
        pm.cost_usd,
    );
    // No PostAgentStop when the Planner was skipped by --plan (no agent ran).
    if plan_override_json.is_none() {
        fire_hook(
            &hook_bus,
            crate::audit::HookEvent::PostAgentStop,
            agent_hook_payload(AgentRole::Planner, &task.id, 0),
        )?;
    }
    finish_stage(
        display,
        &mut metrics,
        &mut state,
        config,
        task,
        task_dir,
        &task.project_path,
        0,
    )?;

    // Decide the agent topology from the task shape (BUILD_PLAN 3.2, P2.2).
    // The Planner has already derived `estimated_complexity`, so we can pick
    // the fast-path (single solo Coder) or the full multi-agent chain now.
    // Phase 5.7: risk classifies FIRST and topology resolves AFTER it — an
    // Auto High/Security tier forces MultiAgent so the risk-added
    // SecurityAuditor survives (the SingleAgent fast-path would collapse it
    // away). Explicit `[pipeline].stages` topologies are never rewritten.
    let task_risk = crate::risk::classify(&task_spec, config);
    let stages = apply_risk_stages(stages, &task_risk, config);
    // Whether a SecurityAuditor is actually part of *this* run. A High/Security
    // risk tier injects one even when `[security] enabled` is false, so this
    // is read off the resolved stage list rather than the config flag.
    let security_enabled = stages.iter().any(|s| s.role == AgentRole::SecurityAuditor);
    let mut topology = select_topology(&task_spec, config);
    let mut topology_reason = topology_reason(&task_spec, config);
    if force_multiagent_for_high_risk(topology, config.pipeline.topology, task_risk.level)
        && matches!(topology, TopologyMode::SingleAgent)
    {
        topology = TopologyMode::MultiAgent;
        topology_reason = format!(
            "{}; risk override: {} tier forces the full multi-agent chain",
            topology_reason,
            task_risk.level.as_str()
        );
    }
    let topology_reason = format!("{}; risk: {}", topology_reason, task_risk.rationale);

    if let Some(ref mut sess) = runtime_session {
        let _ = sess
            .emit_event(
                "planner",
                "done",
                crate::runtime::AgentEventKind::ArtifactProduced,
                serde_json::json!({
                    "role": "planner",
                    "schema": "schemas/task_spec.schema.json",
                }),
            )
            .await;

        if let Ok(mut store) = sess.context_store.try_write() {
            store.upsert(crate::runtime::ContextFragment::new(
                "planner_artifact",
                crate::runtime::FragmentKind::PlanContext,
                &planner_json,
                2000,
            ));
        }

        let _ = agent_runtime
            .checkpoint(
                sess,
                AgentRole::Planner,
                None,
                Some(task_risk.level.as_str().to_string()),
                artifacts.clone(),
            )
            .await;
    }

    // Dry-run: stop after the Planner and surface the spec without executing.

    if dry_run {
        fire_hook(
            &hook_bus,
            crate::audit::HookEvent::PostTaskStop,
            serde_json::json!({"task_id": task.id.to_string(), "dry_run": true}),
        )?;
        // The manifest still records what the dry run reasoned about (no
        // branch by design). Best-effort like every provenance write.
        if config.snapshot.enabled {
            run_manifest.dry_run = true;
            if let Err(e) = super::provenance::write_manifest(task_dir, &run_manifest) {
                eprintln!("Warning: could not update run manifest: {e}");
            }
        }
        return Ok(PipelineResult {
            task_id: task.id,
            context_budget: state.context_budget.clone(),
            state,
            final_diff: String::new(),
            diff_guardwarn: None,
            outcome: RunOutcome::NotEvaluated {
                reason: "plan-only early return: the pipeline did not run".into(),
            },
            verdict: Verdict::Approved,
            // Nothing was reviewed on this early-return path either.
            verdict_source: Some("planner-only (no review performed)".to_string()),
            revision_rounds: 0,
            artifacts,
            metrics,
            safety_proof: None,
            isolation,
            topology,
            topology_reason: topology_reason.clone(),
            risk_level: task_risk.level.as_str().to_string(),
            risk_rationale: task_risk.rationale.clone(),
            test_execution: None,
        });
    }

    // T10: Create a checkpoint after the Planner stage so /undo and /rewind can restore.
    // Both of these used to be discarded. A session directory that cannot be
    // created, or a checkpoint that cannot be written, means `/undo` and
    // `/rewind` silently do not work for the rest of the run — the user
    // discovers it only when they reach for the undo they were promised.
    let session_mgr = crate::session::SessionManager::new(&task.project_path);
    session_mgr.init().with_context(|| {
        format!(
            "could not initialise sessions under {}",
            task.project_path.display()
        )
    })?;
    session_mgr
        .create_checkpoint(
            "after_planner",
            crate::session::current_git_commit(&task.project_path),
        )
        .context("could not write the after_planner checkpoint; /undo and /rewind would not work for this run")?;

    // 2. Initialize Sandbox (backend chosen by config: docker / worktree)
    // `containers` is an Arc and is cloned here so the parallel-coder path below
    // can hand its own clone to each per-coder worktree sandbox.
    let planner_policy = role_policy(AgentRole::Planner, config);
    let sandbox = create_sandbox(
        config.docker.backend,
        docker,
        AgentRole::Planner,
        &task.project_path,
        &task.id,
        &config.docker,
        config,
        planner_policy,
        containers.clone(),
        event_tx.clone(),
    )
    .await?;

    // The sandbox image is expected to be pre-baked with the toolchain the pipeline
    // needs (git/node/npm/python3). We verify presence up front rather than installing
    // at runtime, so a misconfigured image fails fast instead of hanging on apt.
    let required = required_tools(config);
    sandbox.ensure_tools(&required).await?;

    // --- Body stages (everything after the Planner), in configured order ---
    // The topology collapse goes through `body_stages_for` so the rule has one
    // mechanism. It used to be open-coded as "everything but the Planner",
    // which meant the SingleAgent collapse was satisfied only because that arm
    // happens to look up the Coder by hand — change the arm to iterate the list
    // and the full chain would run while the unit test still passed.
    let all_body: Vec<&PipelineStageConfig> = stages
        .iter()
        .filter(|s| s.role != AgentRole::Planner && !s.skip)
        .collect();
    let body_stages: Vec<&PipelineStageConfig> = body_stages_for(topology, &all_body);

    // Build one provider client per distinct provider+fallbacks combination.
    // Stored as `Arc` so the parallel-coder path can move a clone into a
    // spawned task without fighting the borrow checker.
    let mut provider_cache: HashMap<String, Arc<dyn LlmProvider>> = HashMap::new();
    for s in &body_stages {
        // Cache key includes fallbacks so different failover chains don't collide.
        let cache_key = provider_cache_key(s);
        if let std::collections::hash_map::Entry::Vacant(e) = provider_cache.entry(cache_key) {
            let llm = provider_for(&s.provider, &s.fallbacks, config)?;
            e.insert(Arc::from(llm));
        }
    }

    let max_rounds = config
        .pipeline
        .max_revision_rounds
        .unwrap_or(config.general.max_revision_rounds);
    let has_reviewer = body_stages.iter().any(|s| s.role == AgentRole::Reviewer);

    let mut coder_json = String::new();
    let mut tester_json = String::new();
    let mut red_json = String::new();
    // Latest Reviewer verdict JSON, fed to the post-loop Critic pass.
    let mut reviewer_json = String::new();
    // Latest SecurityAuditor verdict JSON. The auditor runs *before* the
    // Reviewer so its findings are something the Reviewer can reconcile
    // rather than a footnote it never sees.
    let mut security_json = String::new();
    // Set by a SecurityAuditor `Rejected`. While it is set, no Reviewer
    // approval can end the run — the auditor's rejection stands until the work
    // is actually revised.
    let mut security_hold = false;
    // Revision feedback is intentionally latest-round-only: each Reviewer
    // verdict OVERWRITES (never appends), so a retrying Coder sees the
    // current critique, not an accumulation of stale guidance. Full history
    // stays in the artifacts trail.
    let mut review_feedback: Option<String> = None;
    // The findings a next revision round would actually be handed: the
    // Reviewer's critical issues, or — while a security rejection holds the run
    // open — the auditor's findings. `None`/`""` means "asked to revise, told
    // nothing", which is what `revision_hold` refuses to spend a round on.
    let mut actionable_findings: Option<String> = None;
    // Set when the Coder's patch was well-formed but would not apply. The
    // rest of the round is skipped and the Coder is asked again with the
    // reason, rather than the run ending.
    let mut patch_failure: Option<String> = None;
    // One allowance for every "tell the model what went wrong" turn, across
    // every cause the pipeline itself decides, so no single failure mode can
    // spend it twice.
    //
    // Seeded from the Coder's tool loop, which spends turns of its own on a
    // truncated answer and reports the count as `RoleRun::feedback_turns`. One
    // allowance covers both, so a stage cannot get twice the turns depending on
    // which of the two it happened to hit.
    //
    // What is *not* covered is the agent's malformed-JSON repair, which happens
    // inside a single `run_agent` call and is bounded there. Uniting that means
    // a budget threaded through the agent's signature, which is a deeper change
    // than a stage-level one; two named budgets beat one that overstates.
    let mut feedback_budget = FeedbackBudget::new();
    // The signature of the findings the *previous* round was run against, so a
    // repeat of the same critique is recognisable.
    let mut last_findings: Option<String> = None;
    let mut verdict = Verdict::Approved;
    // Set whenever a Reviewer actually produces the verdict. Overwritten by the
    // Solo fast path below, which has no Reviewer.
    let mut verdict_source: Option<String> = None;
    let mut round = 0;

    match topology {
        TopologyMode::MultiAgent => {
            if config.parallel.enabled && config.parallel.coder_count > 1 {
                // ── Parallel-coder mode (#3) ────────────────────────────────────────
                // 1) Run N coders concurrently, each isolated in its own git worktree.
                let coder_stage = body_stages
                    .iter()
                    .find(|s| s.role == AgentRole::Coder)
                    .expect("parallel mode requires a Coder stage");
                let coder_cache_key = provider_cache_key(coder_stage);
                let per_coder = run_parallel_coders(
                    config.parallel.coder_count,
                    provider_cache
                        .get(&coder_cache_key)
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "Provider '{}' not found in cache",
                                coder_stage.provider
                            )
                        })?
                        .clone(),
                    &coder_stage.model,
                    &coder_stage.provider,
                    &task_spec,
                    &knowledge_str,
                    &task.project_path,
                    config,
                    containers.clone(),
                    &task.id,
                    display,
                    &mut metrics,
                    &mcp_tools,
                    bare,
                    hook_bus.clone(),
                    task.id,
                )
                .await?;
                // Phase 5.5: close the parallel-coder spend hole — N coders
                // accrue N costs before the Synthesizer runs, so enforce the
                // hard ceiling and the unified budget here, not at the next
                // stage boundary.
                enforce_spend_cap(config.general.spend_cap_usd, &metrics)?;
                state.accrue_budget(&metrics)?;

                // Each parallel coder ran in its own git worktree — record the isolation
                // pattern (one record represents the N independent coder sessions).
                isolation.push(IsolationRecord {
                    role: AgentRole::Coder,
                    backend: SandboxBackend::Worktree,
                    context_sources: isolation_sources_for(
                        AgentRole::Coder,
                        config.red_blue.enabled,
                        security_enabled,
                    ),
                    saw_other_reasoning: false,
                });

                // 2) Reconcile the per-coder diffs through the Synthesizer stage
                //    (injected by `resolve_stages` when parallel mode is on).
                let synth_stage = body_stages
                    .iter()
                    .find(|s| s.role == AgentRole::Synthesizer)
                    .expect("parallel mode requires a Synthesizer stage");
                let synth_cache_key = provider_cache_key(synth_stage);
                let synth_llm = provider_cache.get(&synth_cache_key).ok_or_else(|| {
                    anyhow::anyhow!("Provider '{}' not found in cache", synth_stage.provider)
                })?;
                let coder_json_in = serde_json::to_string(&per_coder)?;
                let stage_run = run_role(
                    AgentRole::Synthesizer,
                    &**synth_llm,
                    &synth_stage.model,
                    &synth_stage.provider,
                    &task_spec,
                    &coder_json_in,
                    "",
                    "",
                    "",
                    "",
                    0,
                    &knowledge_str,
                    &task.project_path,
                    None,
                    display,
                    &mut metrics,
                    synth_stage.max_tokens,
                    synth_stage.temperature,
                    synth_stage.reasoning_effort.as_deref(),
                    &mcp_tools,
                    config_max_diff_lines(config),
                    bare,
                    &hook_bus,
                    &task.id,
                    steer_rx,
                )
                .await?;
                let (json, summary, role_output) =
                    (stage_run.json, stage_run.summary, stage_run.output);
                artifacts.push((AgentRole::Synthesizer, json.clone()));
                record_isolation(
                    &mut isolation,
                    AgentRole::Synthesizer,
                    config,
                    security_enabled,
                );
                let m = metrics.last().unwrap_or_else(|| {
                    unreachable!("metrics always has at least one entry after push")
                });
                display.agent_done(AgentRole::Synthesizer, summary, m.usage(), m.cost_usd);
                finish_stage(
                    display,
                    &mut metrics,
                    &mut state,
                    config,
                    task,
                    task_dir,
                    &task.project_path,
                    0,
                )?;

                let merged = match role_output {
                    RoleOutput::Synthesizer(s) => s.merged,
                    _ => unreachable!("synthesizer stage yields a Synthesis"),
                };
                coder_json = serde_json::to_string_pretty(&merged)?;
                // The Tester runs against this tree. If the merged patch never
                // lands, the Tester verifies a tree that does not contain the
                // change, and a Reviewer then judges a verdict about code that
                // was never written.
                sandbox
                    .apply_patch(&code_diff_to_edit_text(&merged), &task.project_path)
                    .await
                    .context("the Synthesizer's merged patch did not apply")?;

                // 3) Run the remaining stages (Tester / Red / Reviewer / SecurityAuditor)
                //    exactly once. In parallel mode the coders don't re-run on revision
                //    feedback, so there is no inner revision loop. The Critic is
                //    excluded here: it runs once post-loop with the final verdict.
                for stage in body_stages.iter().filter(|s| {
                    s.role != AgentRole::Coder
                        && s.role != AgentRole::Synthesizer
                        && s.role != AgentRole::Critic
                }) {
                    // Same per-stage check as the sequential loop: this branch
                    // has no outer revision loop to fall back on, so without it
                    // the flag is never read at all.
                    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                        save_task_record(task, &metrics, TaskStatus::Cancelled, task_dir, 0)?;
                        return Err(crate::NikiError::Cancelled.into());
                    }
                    let cache_key = provider_cache_key(stage);
                    let llm = provider_cache.get(&cache_key).ok_or_else(|| {
                        anyhow::anyhow!("Provider '{}' not found in cache", stage.provider)
                    })?;
                    let stage_run = run_role(
                        stage.role,
                        &**llm,
                        &stage.model,
                        &stage.provider,
                        &task_spec,
                        &coder_json,
                        &tester_json,
                        &red_json,
                        &reviewer_json,
                        &security_json,
                        0,
                        &knowledge_str,
                        &task.project_path,
                        None,
                        display,
                        &mut metrics,
                        stage.max_tokens,
                        stage.temperature,
                        stage.reasoning_effort.as_deref(),
                        &mcp_tools,
                        config_max_diff_lines(config),
                        bare,
                        &hook_bus,
                        &task.id,
                        steer_rx,
                    )
                    .await?;
                    let (json, summary, role_output) =
                        (stage_run.json, stage_run.summary, stage_run.output);
                    artifacts.push((stage.role, json.clone()));
                    isolation.push(IsolationRecord {
                        role: stage.role,
                        backend: config.docker.backend,
                        context_sources: isolation_sources_for(
                            stage.role,
                            config.red_blue.enabled,
                            security_enabled,
                        ),
                        saw_other_reasoning: false,
                    });
                    let m = metrics.last().unwrap_or_else(|| {
                        unreachable!("metrics always has at least one entry after push")
                    });
                    display.agent_done(stage.role, summary, m.usage(), m.cost_usd);
                    finish_stage(
                        display,
                        &mut metrics,
                        &mut state,
                        config,
                        task,
                        task_dir,
                        &task.project_path,
                        0,
                    )?;

                    match role_output {
                        RoleOutput::Tester(_) => {
                            tester_json = json;
                        }
                        RoleOutput::Red(_) => {
                            // Capture the Red critique so the downstream Reviewer (which
                            // runs after it in this loop) must reconcile it (#1.2).
                            red_json = json;
                        }
                        RoleOutput::Reviewer(v) => {
                            apply_reviewer_verdict(
                                v.verdict,
                                &mut verdict,
                                &mut verdict_source,
                                security_hold,
                            );
                            reviewer_json = json;
                        }
                        RoleOutput::SecurityAuditor(v) => {
                            apply_security_verdict(
                                v.verdict,
                                &mut verdict,
                                &mut verdict_source,
                                &mut security_hold,
                            );
                            security_json = json;
                        }
                        _ => unreachable!("only Tester/Red/Reviewer/SecurityAuditor remain"),
                    }
                }
            } else {
                while round < max_rounds {
                    // Cooperative cancellation: the TUI (or any holder of the
                    // flag) can abort the run between revision rounds.
                    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                        save_task_record(task, &metrics, TaskStatus::Cancelled, task_dir, round)?;
                        return Err(crate::NikiError::Cancelled.into());
                    }
                    for stage in body_stages.iter().filter(|s| s.role != AgentRole::Critic) {
                        // Checked per stage, not just per round. A round is
                        // Tester → Red → SecurityAuditor → Reviewer, each a
                        // sequential LLM call; on a slow model that is minutes
                        // in which the cancel flag was never read. The user
                        // pressed Esc and nothing happened until the round
                        // happened to end.
                        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                            save_task_record(
                                task,
                                &metrics,
                                TaskStatus::Cancelled,
                                task_dir,
                                round,
                            )?;
                            return Err(crate::NikiError::Cancelled.into());
                        }
                        let cache_key = provider_cache_key(stage);
                        let llm = provider_cache.get(&cache_key).ok_or_else(|| {
                            anyhow::anyhow!("Provider '{}' not found in cache", stage.provider)
                        })?;
                        // Re-resolved every round: the sandbox root is fixed
                        // for a run, but the *files* in it change as patches
                        // land, and `build_current_files` reads from disk.
                        let stage_root: Option<std::path::PathBuf> =
                            sandbox.work_root().map(|p| p.to_path_buf());
                        let stage_run = run_role(
                            stage.role,
                            &**llm,
                            &stage.model,
                            &stage.provider,
                            &task_spec,
                            &coder_json,
                            &tester_json,
                            &red_json,
                            &reviewer_json,
                            &security_json,
                            round,
                            &knowledge_str,
                            // The tree the Coder's edits land in, not the
                            // project it was asked about. On the worktree
                            // backend these differ from round 1 onwards, and a
                            // Coder shown the pre-run file cannot produce an
                            // edit that matches what is already there.
                            stage_root.as_deref().unwrap_or(&task.project_path),
                            review_feedback.as_ref(),
                            display,
                            &mut metrics,
                            stage.max_tokens,
                            stage.temperature,
                            stage.reasoning_effort.as_deref(),
                            &mcp_tools,
                            config_max_diff_lines(config),
                            bare,
                            &hook_bus,
                            &task.id,
                            steer_rx,
                        )
                        .await?;
                        if stage.role == AgentRole::Coder {
                            // Fold the loop's own recovery turns into the
                            // shared allowance.
                            //
                            // `note_prior`, not a re-seed: re-creating the
                            // budget here restarted it at zero on every round,
                            // so a stage whose patch never applied looped
                            // forever printing "attempt 1 of 2". Found by
                            // running the suite and finding a test binary
                            // still going after ten minutes.
                            feedback_budget.note_prior(stage_run.feedback_turns);
                        }
                        let (json, summary, role_output) =
                            (stage_run.json, stage_run.summary, stage_run.output);
                        artifacts.push((stage.role, json.clone()));
                        if let Some(ref mut sess) = runtime_session {
                            let fragment_kind = match stage.role {
                                AgentRole::Planner => crate::runtime::FragmentKind::PlanContext,
                                AgentRole::Coder => crate::runtime::FragmentKind::ArtifactSummary,
                                AgentRole::Tester => crate::runtime::FragmentKind::TestFailure,
                                AgentRole::Reviewer
                                | AgentRole::Critic
                                | AgentRole::SecurityAuditor => {
                                    crate::runtime::FragmentKind::ReviewFinding
                                }
                                _ => crate::runtime::FragmentKind::ArtifactSummary,
                            };
                            if let Ok(mut store) = sess.context_store.try_write() {
                                store.upsert(crate::runtime::ContextFragment::new(
                                    format!("{}_{}", stage.role.as_str(), round),
                                    fragment_kind,
                                    &json,
                                    1500,
                                ));
                            }
                            let _ = sess
                                .emit_event(
                                    stage.role.as_str(),
                                    &format!("round_{round}"),
                                    crate::runtime::AgentEventKind::ArtifactProduced,
                                    serde_json::json!({
                                        "role": stage.role.as_str(),
                                        "round": round,
                                        "summary": summary,
                                    }),
                                )
                                .await;

                            let _ = agent_runtime
                                .checkpoint(
                                    sess,
                                    stage.role,
                                    Some(format!("round_{round}")),
                                    Some(task_risk.level.as_str().to_string()),
                                    artifacts.clone(),
                                )
                                .await;
                        }
                        isolation.push(IsolationRecord {
                            role: stage.role,
                            backend: config.docker.backend,
                            context_sources: isolation_sources_for(
                                stage.role,
                                config.red_blue.enabled,
                                security_enabled,
                            ),
                            saw_other_reasoning: false,
                        });
                        let m = metrics.last().unwrap_or_else(|| {
                            unreachable!("metrics always has at least one entry after push")
                        });
                        display.agent_done(stage.role, summary, m.usage(), m.cost_usd);
                        finish_stage(
                            display,
                            &mut metrics,
                            &mut state,
                            config,
                            task,
                            task_dir,
                            &task.project_path,
                            round,
                        )?;

                        match role_output {
                            RoleOutput::Coder(diff) => {
                                coder_json = json;
                                // A patch that validates but does not apply is
                                // the Coder's `search` not being text that is
                                // in the file. That is a fixable mistake, not a
                                // dead end: the model is told, and gets another
                                // round with the reason.
                                //
                                // It used to end the run. Measured: a refactor
                                // produced a schema-valid artifact whose anchor
                                // matched nothing, and the whole task died at
                                // round 1 — after the plan, the code, and a
                                // Tester pass had already been paid for. The
                                // guard itself was right (a Tester must never
                                // verify a tree without the change); returning
                                // an error was not.
                                match sandbox
                                    .apply_patch(&code_diff_to_edit_text(&diff), &task.project_path)
                                    .await
                                {
                                    Ok(()) => {}
                                    Err(e) => {
                                        patch_failure = Some(format!(
                                            "The harness could not apply your patch, so none of \
                                             it was written and nothing you produced this round \
                                             is in the tree.\n\nThe usual cause is a `search` \
                                             that is not literally present in the file — it \
                                             must be copied verbatim, including indentation, \
                                             with enough surrounding lines to be unique. Re-read \
                                             the file and rebuild each edit against what it \
                                             actually contains now.\n\nThe harness said: {e}"
                                        ));
                                    }
                                }
                            }
                            RoleOutput::Tester(_) => {
                                tester_json = json;
                            }
                            RoleOutput::Red(_) => {
                                // Capture the Red critique so the Reviewer (which runs
                                // after it in this round) must reconcile it (#1.2).
                                red_json = json;
                            }
                            RoleOutput::Reviewer(v) => {
                                apply_reviewer_verdict(
                                    v.verdict,
                                    &mut verdict,
                                    &mut verdict_source,
                                    security_hold,
                                );
                                reviewer_json = json.clone();
                                // A Reviewer that asks for a revision while
                                // leaving `feedback` null has still said what
                                // is wrong — in `issues`. The Coder only ever
                                // reads `feedback`, so the critique was being
                                // discarded and the next round, when it ran,
                                // got nothing.
                                //
                                // Bridging it is better than refusing to loop:
                                // the information exists, and the alternative
                                // is a run that stops at "revise" with a named
                                // problem it will not act on. The gate below
                                // still covers the case where there is genuinely
                                // nothing to act on — a revision request with
                                // no issues *and* no feedback, which is what a
                                // small model does when it declines to approve
                                // without naming a blocker.
                                let feedback = v
                                    .feedback
                                    .or_else(|| bridge_feedback_from_issues(&v.issues, round));
                                actionable_findings = feedback
                                    .as_ref()
                                    .and_then(|f| issue_signature(&f.critical_issues));
                                review_feedback = match feedback {
                                    Some(f) => Some(serde_json::to_string_pretty(&f)?),
                                    None => None,
                                };
                            }
                            RoleOutput::Synthesizer(s) => {
                                // The reconciled change replaces the per-coder diffs for the
                                // downstream Tester/Reviewer stages.
                                coder_json = serde_json::to_string_pretty(&s.merged)?;
                                sandbox
                                    .apply_patch(
                                        &code_diff_to_edit_text(&s.merged),
                                        &task.project_path,
                                    )
                                    .await
                                    .context("the Synthesizer's merged patch did not apply")?;
                            }
                            RoleOutput::SecurityAuditor(v) => {
                                // A security rejection gates the run. It used to
                                // apply only when there was *no* reviewer, on
                                // the theory that the Reviewer owns the gate —
                                // which meant that in the normal configuration,
                                // with a Reviewer present, a SecurityAuditor
                                // verdict of Rejected was computed, recorded,
                                // and then had no effect on the run at all.
                                apply_security_verdict(
                                    v.verdict,
                                    &mut verdict,
                                    &mut verdict_source,
                                    &mut security_hold,
                                );
                                // While the hold stands, these are what the next
                                // round would be asked to fix — the reviewer's
                                // approval cannot clear it.
                                if security_hold {
                                    actionable_findings = issue_signature(&v.findings);
                                }
                                security_json = json.clone();
                            }
                            RoleOutput::Planner(_) => unreachable!("planner is handled separately"),
                            // The Critic is filtered from loop iteration and
                            // runs once post-loop with the final verdict.
                            RoleOutput::Critic(_) => {
                                unreachable!("critic runs post-loop, never in the loop")
                            }
                        }
                        // The round is abandoned here, not after it: the Tester
                        // must not run against a tree the change never reached,
                        // and neither must the Reviewer judge it.
                        if patch_failure.is_some() {
                            break;
                        }
                    }

                    // A patch that will not apply gets its own allowance.
                    //
                    // It is a *mechanical* error — a `search` that is not in
                    // the file — not a judgment about quality, so charging it
                    // against the Reviewer's revision budget spends a round the
                    // Reviewer asked for on something it never asked about.
                    // Measured: a run whose Reviewer was still finding real
                    // issues burned its last round on a patch that would not
                    // apply, and failed with the change it had already made
                    // twice thrown away.
                    //
                    // Separate and smaller, because unlike a quality round this
                    // one is not converging by definition: the same anchor will
                    // not match next time either.
                    if let Some(reason) = patch_failure.take() {
                        let feedback =
                            feedback_budget.take(&reason, FeedbackCause::UnappliablePatch);
                        match feedback {
                            StageFeedback::Stop { .. } => {
                                // Phrased through the shared budget so the reason
                                // names the cause and the model's own words. The
                                // previous wording said "could not produce a
                                // patch that applies", which lost the phrase the
                                // existing tests (rightly) assert on, and told a
                                // user less.
                                let detail =
                                    feedback.stop_message().expect("a stop carries a message");
                                return Err(anyhow::anyhow!(
                                    "the Coder's patch did not apply, and the shared recovery \
                                     allowance is used up — {detail}"
                                ));
                            }
                            StageFeedback::Send { reason, .. } => {
                                review_feedback = Some(reason);
                                display.agent_warning(
                                    AgentRole::Coder,
                                    &format!(
                                        "the patch did not apply — asking the Coder to rebuild \
                                         it (attempt {} of {})",
                                        feedback_budget.used(),
                                        MAX_STAGE_FEEDBACK
                                    ),
                                );
                                // Deliberately does not advance `round`: the
                                // Reviewer has not been consulted about
                                // anything yet, so its budget is untouched.
                                continue;
                            }
                        }
                    }

                    // A round is only worth running if it can act on something.
                    // Both holds below keep the verdict at `RevisionNeeded`, so
                    // stopping early reports the same honest outcome as running
                    // out of rounds — it just stops paying for rounds that
                    // cannot help, and stops a doomed Coder call from taking
                    // down a task whose tests already passed.
                    let mut stop_now = false;
                    if matches!(verdict, Verdict::RevisionNeeded) {
                        match revision_hold(
                            last_findings.as_deref(),
                            actionable_findings.as_deref(),
                        ) {
                            RevisionHold::Continue => {
                                last_findings = actionable_findings.clone();
                                display.revision_requested(round, max_rounds, &[]);
                            }
                            RevisionHold::NoActionableIssues => {
                                display.agent_warning(
                                    AgentRole::Reviewer,
                                    &format!(
                                        "revision requested with nothing to act on \
                                         (round {round}/{max_rounds}) — stopping instead of \
                                         repeating the round"
                                    ),
                                );
                                stop_now = true;
                            }
                            RevisionHold::UnchangedIssues => {
                                display.agent_warning(
                                    AgentRole::Reviewer,
                                    &format!(
                                        "round {round} returned the same findings unchanged \
                                         — stopping instead of revising again"
                                    ),
                                );
                                stop_now = true;
                            }
                        }
                    }
                    if stop_now {
                        break;
                    }

                    if has_reviewer {
                        // A security rejection keeps the loop going even if a
                        // Reviewer went on to approve: the run is not done
                        // until the finding is actually addressed.
                        if !security_hold
                            && matches!(verdict, Verdict::Approved | Verdict::Rejected)
                        {
                            break;
                        }
                    } else {
                        // No reviewer to gate the loop on; one pass is enough.
                        break;
                    }
                    round += 1;
                }
            }
        } // close MultiAgent arm
        TopologyMode::Auto => unreachable!(
            "select_topology resolves Auto into MultiAgent/SingleAgent before dispatch"
        ),
        TopologyMode::SingleAgent => {
            // Single-agent fast-path (BUILD_PLAN 3.2, P2.2): the Planner already
            // derived the task shape. For bounded/sequential work we collapse
            // Coder/Tester/Reviewer/Red into one solo Coder session, removing the
            // 3-4 large-context re-ingestion sessions that make up the multi-agent
            // token tax (slice 2.3). Trade-off (named in the report): there is no
            // independent Red/Blue adversarial review on this path.
            // This arm has no revision loop, so nothing downstream would ever
            // read the cancel flag. Check it before spending a full Coder
            // session on work the user has already asked to stop.
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                // `&metrics`, not `&[]`: the Planner already ran and was
                // billed, and a cancelled record that shows zero stages makes
                // the user pay for a run that appears to have done nothing.
                save_task_record(task, &metrics, TaskStatus::Cancelled, task_dir, 0)?;
                return Err(crate::NikiError::Cancelled.into());
            }
            let coder_stage = body_stages
                .iter()
                .find(|s| s.role == AgentRole::Coder)
                .expect("single-agent mode requires a Coder stage");
            let coder_cache_key = provider_cache_key(coder_stage);
            let coder_llm = provider_cache.get(&coder_cache_key).ok_or_else(|| {
                anyhow::anyhow!("Provider '{}' not found in cache", coder_stage.provider)
            })?;
            let current_files = build_current_files(&task_spec, &task.project_path);
            fire_hook(
                &hook_bus,
                crate::audit::HookEvent::PreAgentStart,
                agent_hook_payload(AgentRole::Coder, &task.id, 0),
            )?;
            let solo_ctx = context! {
                task_description => task.description.clone(),
                project_knowledge => knowledge_str.clone(),
                project_memory => memory_for_role(&task.project_path, AgentRole::Coder, bare, &state.context_budget),
                current_files => current_files.clone(),
                // The fast path gets the tool loop too.
                //
                // It did not, and that was backwards. The loop — read, grep,
                // run, edit, then submit a typed artifact — only existed on
                // the multi-agent path, so the *cheaper* topology was the one
                // that could not explore, and the expensive one could. The
                // compute-matched ablation says a single agent with a tool loop
                // is the better default, and a single agent *without* one is
                // the case that fails on a weak model.
                //
                // `solo.md` had no tool protocol either, so it would have
                // answered in prose; the prompt now describes the protocol
                // when `tool_loop` is set, and the one-shot fallback below is
                // unchanged for a model that ignores it.
                tool_loop => true,
            };
            // Loop first, one-shot only as the fallback — the same arrangement
            // the multi-agent Coder uses, and for the same reason: the loop can
            // only add capability, never take it away.
            let solo_json = match run_coder_tool_loop(
                AgentRole::Coder,
                &**coder_llm,
                &coder_stage.model,
                &coder_stage.provider,
                "solo.md",
                solo_ctx.clone(),
                "schemas/code_diff.schema.json",
                &task.project_path,
                coder_stage.max_tokens,
                display,
                &mut metrics,
            )
            .await
            {
                Some((json, _turns)) => json,
                None => {
                    run_stage(
                        AgentRole::Coder,
                        &**coder_llm,
                        &coder_stage.model,
                        &coder_stage.provider,
                        "solo.md",
                        solo_ctx,
                        "schemas/code_diff.schema.json",
                        display,
                        &mut metrics,
                        coder_stage.max_tokens,
                        coder_stage.temperature,
                        coder_stage.reasoning_effort.as_deref(),
                        steer_rx,
                    )
                    .await?
                }
            };
            artifacts.push((AgentRole::Coder, solo_json.clone()));
            fire_hook(
                &hook_bus,
                crate::audit::HookEvent::PostAgentStop,
                agent_hook_payload(AgentRole::Coder, &task.id, 0),
            )?;
            record_isolation(&mut isolation, AgentRole::Coder, config, security_enabled);
            // Copy the values out: `metrics` is about to be borrowed mutably
            // below, and the old code held a reference to its last element
            // across that call. An empty metrics list was an `unreachable!`
            // panic; it is now a zero-cost stage, which is the honest reading.
            let (usage, cost) = metrics
                .last()
                .map(|m| (m.usage(), m.cost_usd))
                .unwrap_or((crate::llm::provider::TokenUsage::default(), 0.0));
            display.agent_done(
                AgentRole::Coder,
                vec!["solo code diff produced".to_string()],
                usage,
                cost,
            );
            finish_stage(
                display,
                &mut metrics,
                &mut state,
                config,
                task,
                task_dir,
                &task.project_path,
                0,
            )?;

            // The solo Coder returns a CodeDiff; apply it so the downstream diff
            // read picks up the change.
            coder_json = solo_json;
            // An unparseable artifact is treated as a failed apply, so it
            // reaches the same bounded repair attempt rather than falling
            // through to a self-approval of a change that was never written.
            let first_apply: Result<()> = match serde_json::from_str::<CodeDiff>(&coder_json) {
                Ok(parsed) => {
                    sandbox
                        .apply_patch(&code_diff_to_edit_text(&parsed), &task.project_path)
                        .await
                }
                Err(parse_err) => Err(anyhow::anyhow!(
                    "the solo Coder's artifact is not a valid code diff: {parse_err}"
                )),
            };
            if let Err(apply_err) = first_apply {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    save_task_record(task, &metrics, TaskStatus::Cancelled, task_dir, 0)?;
                    return Err(crate::NikiError::Cancelled.into());
                }
                // One bounded repair attempt: show the coder its exact apply
                // error and ask for corrected SEARCH blocks. Weak/local models
                // often fix themselves when shown the failure (e.g. regex
                // anchors instead of verbatim text). Same spend-cap and audit
                // accounting as the first attempt — never silent, never unbounded.
                let (usage, cost) = metrics
                    .last()
                    .map(|m| (m.usage(), m.cost_usd))
                    .unwrap_or((crate::llm::provider::TokenUsage::default(), 0.0));
                display.agent_done(
                    AgentRole::Coder,
                    vec![format!("patch did not apply ({apply_err}) — repairing")],
                    usage,
                    cost,
                );
                fire_hook(
                    &hook_bus,
                    crate::audit::HookEvent::PreAgentStart,
                    agent_hook_payload(AgentRole::Coder, &task.id, 1),
                )?;
                let repair_json =
run_stage(
                    AgentRole::Coder,
                    &**coder_llm,
                    &coder_stage.model,
                    &coder_stage.provider,
                    "solo.md",
                    context! {
                        task_description => format!(
                            "{}\n\n---\nYour previous output FAILED to apply. Error: {apply_err}\nFix rules: SEARCH blocks must be copied VERBATIM from \"Current File Contents\" — no regex, no `^`/`$` anchors, no line numbers, no paraphrasing. To insert at the top of a file, include its first 3-5 actual lines in SEARCH and put your new lines before them in REPLACE. Output ONLY the corrected JSON artifact.",
                            task.description,
                        ),
                        project_knowledge => knowledge_str.clone(),
                        project_memory => memory_for_role(&task.project_path, AgentRole::Coder, bare, &state.context_budget),
                        current_files => current_files.clone(),
                    },
                    "schemas/code_diff.schema.json",
                    display,
                    &mut metrics,
                    coder_stage.max_tokens,
                    coder_stage.temperature,
                    coder_stage.reasoning_effort.as_deref(),
                    steer_rx,
                )

                .await?;
                artifacts.push((AgentRole::Coder, repair_json.clone()));
                fire_hook(
                    &hook_bus,
                    crate::audit::HookEvent::PostAgentStop,
                    agent_hook_payload(AgentRole::Coder, &task.id, 1),
                )?;
                record_isolation(&mut isolation, AgentRole::Coder, config, security_enabled);
                let rm = metrics.last().unwrap_or_else(|| {
                    unreachable!("metrics always has at least one entry after push")
                });
                display.agent_done(
                    AgentRole::Coder,
                    vec!["repaired code diff produced".to_string()],
                    rm.usage(),
                    rm.cost_usd,
                );
                finish_stage(
                    display,
                    &mut metrics,
                    &mut state,
                    config,
                    task,
                    task_dir,
                    &task.project_path,
                    0,
                )?;

                coder_json = repair_json;
                let repaired: CodeDiff = serde_json::from_str(&coder_json).with_context(|| {
                    format!(
                        "the repair attempt did not return a valid code diff either: \
                         {coder_json}"
                    )
                })?;
                // The repair was the last chance. Falling through here used to
                // set `verdict = Approved` and complete the run, handing back
                // a branch containing no change whatsoever and reporting it as
                // a success.
                sandbox
                    .apply_patch(&code_diff_to_edit_text(&repaired), &task.project_path)
                    .await
                    .with_context(|| {
                        format!(
                            "the repaired patch still did not apply ({:?}); the working \
                             tree contains none of this run's work, so the run is \
                             stopped instead of reporting an approval for it",
                            task.project_path
                        )
                    })?;
            }
            verdict = Verdict::Approved;
            // The Solo fast path never runs a Reviewer: this is the Coder
            // approving its own patch. Record that provenance so "Approved"
            // cannot be read as an independent check.
            verdict_source = Some("solo-coder (no independent review)".to_string());
            round = 0;
        }
    }

    // Critic pass (risk-gated, max-once by construction): checks that the
    // Reviewer verdict is grounded in the diff/test evidence. A Reject forces
    // exactly one Reviewer retry with the unsupported claims attached,
    // followed by a closing Critic run. The Critic never loops and never
    // gates on its own — the verdict follows the (possibly retried)
    // Reviewer; both Critiques stay in the artifact trail. Skipped when no
    // Reviewer ran (e.g. the single-agent fast-path).
    if topology == TopologyMode::MultiAgent && !reviewer_json.is_empty() {
        let critic_stage = stages
            .iter()
            .find(|s| s.role == AgentRole::Critic && !s.skip)
            .cloned();
        let reviewer_stage = stages
            .iter()
            .find(|s| s.role == AgentRole::Reviewer && !s.skip)
            .cloned();
        if let (Some(critic_stage), Some(reviewer_stage)) = (critic_stage, reviewer_stage) {
            let critic_llm = provider_cache
                .get(&provider_cache_key(&critic_stage))
                .ok_or_else(|| {
                    anyhow::anyhow!("Provider '{}' not found in cache", critic_stage.provider)
                })?
                .clone();
            let (_critic_json, critic_output) = run_bookkept_stage(
                &critic_stage,
                &*critic_llm,
                &task_spec,
                &coder_json,
                &tester_json,
                &red_json,
                &reviewer_json,
                &security_json,
                round,
                &knowledge_str,
                &task.project_path,
                None,
                display,
                &mut metrics,
                &mut artifacts,
                &mut isolation,
                &mcp_tools,
                config,
                &hook_bus,
                &task.id,
                task,
                task_dir,
                &mut state,
                bare,
                security_enabled,
                steer_rx,
            )
            .await?;
            let rejected = matches!(
                critic_output,
                RoleOutput::Critic(ref c) if matches!(c.disposition, CriticDisposition::Reject)
            );
            if rejected && let RoleOutput::Critic(critique) = critic_output {
                // One exact Reviewer retry with the critique as guidance.
                let retry_guidance = format!(
                    "The Critic rejected the previous verdict as ungrounded: {}\nUnsupported claims:\n- {}\nRe-verify each claim against the diff and test evidence, then render a fresh verdict.",
                    critique.summary,
                    critique.unsupported_claims.join("\n- ")
                );
                review_feedback = Some(retry_guidance);
                let reviewer_llm = provider_cache
                    .get(&provider_cache_key(&reviewer_stage))
                    .ok_or_else(|| {
                        anyhow::anyhow!("Provider '{}' not found in cache", reviewer_stage.provider)
                    })?
                    .clone();
                let (retry_json, retry_output) = run_bookkept_stage(
                    &reviewer_stage,
                    &*reviewer_llm,
                    &task_spec,
                    &coder_json,
                    &tester_json,
                    &red_json,
                    &reviewer_json,
                    &security_json,
                    round,
                    &knowledge_str,
                    &task.project_path,
                    review_feedback.as_ref(),
                    display,
                    &mut metrics,
                    &mut artifacts,
                    &mut isolation,
                    &mcp_tools,
                    config,
                    &hook_bus,
                    &task.id,
                    task,
                    task_dir,
                    &mut state,
                    bare,
                    security_enabled,
                    steer_rx,
                )
                .await?;
                if let RoleOutput::Reviewer(v) = retry_output {
                    apply_reviewer_verdict(
                        v.verdict,
                        &mut verdict,
                        &mut verdict_source,
                        security_hold,
                    );
                    reviewer_json = retry_json;
                    // No further rounds exist post-loop, so the retried
                    // verdict's feedback has nowhere to go — the verdict
                    // itself is what the closing Critic judges.
                }
                round += 1;
                // Closing Critic run: records the final grounding judgment.
                // Its disposition is recorded, not enforced.
                run_bookkept_stage(
                    &critic_stage,
                    &*critic_llm,
                    &task_spec,
                    &coder_json,
                    &tester_json,
                    &red_json,
                    &reviewer_json,
                    &security_json,
                    round,
                    &knowledge_str,
                    &task.project_path,
                    None,
                    display,
                    &mut metrics,
                    &mut artifacts,
                    &mut isolation,
                    &mcp_tools,
                    config,
                    &hook_bus,
                    &task.id,
                    task,
                    task_dir,
                    &mut state,
                    bare,
                    security_enabled,
                    steer_rx,
                )
                .await?;
            }
        }
    }

    // Read the resulting diff, scoped to agent-produced files (Phase 5.1).
    // For the Docker backend the patch was applied to the bind-mounted host
    // project, so we read the host working tree directly. For worktree the
    // change lives only in the sandbox copy, so we read it from there (the
    // run step applies it back to the host before committing).
    let agent_files: Vec<String> = artifacts
        .iter()
        .filter(|(r, _)| *r == AgentRole::Coder)
        .flat_map(|(_, j)| crate::output::git::agent_files_from_coder_json(Some(j)))
        .collect();
    let final_diff = match config.docker.backend {
        SandboxBackend::Docker => {
            crate::output::git::working_tree_diff_scoped(&task.project_path, &agent_files)
        }
        _ => sandbox.get_diff(&agent_files).await?,
    };

    // Diff-size guardrail (optional). Warns when a single run's diff grows past
    // the configured ceiling — the "smaller incremental changes" control from the
    // agentic-engineering checklist (arXiv 2603.27249 §4). Zero = unset.
    let diff_guardwarn = (config.general.max_diff_lines > 0)
        .then(|| {
            let changed_lines = final_diff
                .lines()
                .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
                .count();
            let limit = config.general.max_diff_lines as usize;
            (changed_lines > limit).then(|| {
                format!(
                    "Diff adds {} changed lines, exceeding guardrail general.max_diff_lines ({}).\
                 Prefer smaller incremental PRs; the Reviewer was nudged toward a tighter delta.",
                    changed_lines, limit
                )
            })
        })
        .flatten();

    // Verification in the loop: actually execute the project's test suite inside
    // the sandbox and record the real result as part of the audit trail, *before*
    // the branch is created. This is the "verified before you see it" guarantee.
    let mut test_execution = tester::run_tests(&*sandbox, config, &task.project_path).await;
    // Mutation gate (opt-in): when configured, surviving mutants fail the run
    // exactly like a failing suite. The result nests inside test_execution so
    // the audit trail keeps one verification record per run.
    if let Some(te) = test_execution.as_mut() {
        te.mutation = tester::run_mutation(&*sandbox, config, &task.project_path)
            .await
            .map(Box::new);
    }

    sandbox.destroy().await?;

    update_context_budget(&metrics, &mut state, &task.project_path, task_dir, config)?;
    save_task_record(task, &metrics, TaskStatus::Running, task_dir, round)?;

    // Extract learnings from this run and save to memory
    extract_memory_from_artifacts(
        &task.project_path,
        &task.description,
        &artifacts,
        &verdict,
        &state,
    );

    // Phase 4.4 distillation trigger: an Approved run with a green executed
    // suite stages a skill candidate (never auto-activates; promotion is an
    // explicit `niki skills promote` step). Best-effort: never fails the run.
    maybe_stage_skill_candidate(
        &task.project_path,
        config,
        &task.description,
        &artifacts,
        &verdict,
        test_execution.as_ref(),
        &metrics,
        &task.id.to_string(),
    );

    // Phase 4.6: the audit trail writers are live — one entry per completed
    // run, appended (never overwritten) under the project dir. Best-effort.
    crate::audit::append_audit_entry(
        &task.project_path,
        &task.id.to_string(),
        &crate::audit::AuditEntry::new(
            "pipeline_completed",
            serde_json::json!({
                "verdict": format!("{:?}", verdict),
                "stages": metrics.len(),
                "cost_usd": metrics.iter().map(|m| m.cost_usd).sum::<f64>(),
            }),
        ),
    );

    if let Some(ref mut sess) = runtime_session {
        let _ = sess
            .emit_event(
                "pipeline",
                "completed",
                crate::runtime::AgentEventKind::SessionCompleted,
                serde_json::json!({
                    "verdict": format!("{:?}", verdict),
                    "revision_rounds": round,
                    "stages": metrics.len(),
                    "cost_usd": metrics.iter().map(|m| m.cost_usd).sum::<f64>(),
                }),
            )
            .await;

        let _ = agent_runtime
            .checkpoint(
                sess,
                AgentRole::Reviewer,
                Some(format!("round_{round}")),
                Some(task_risk.level.as_str().to_string()),
                artifacts.clone(),
            )
            .await;
    }

    // The one place a run's outcome is decided.
    //
    // `verdict` alone cannot express "nobody reviewed this", so the outcome is
    // derived from what actually ran: a reviewer that produced a verdict, the
    // Solo fast path that approved its own work, or neither.
    let reviewer_ran = !reviewer_json.is_empty();
    let outcome = if reviewer_ran && verdict_source.is_some() {
        match verdict {
            Verdict::Approved => RunOutcome::Reviewed {
                verdict,
                by: verdict_source.clone().unwrap_or_else(|| "reviewer".into()),
            },
            _ => RunOutcome::RevisionRequested {
                by: verdict_source.clone().unwrap_or_else(|| "reviewer".into()),
            },
        }
    } else if topology == TopologyMode::SingleAgent {
        RunOutcome::SelfVerified {
            note: "the SingleAgent fast path approves its own patch; no independent review \
                   was performed"
                .into(),
        }
    } else {
        RunOutcome::NotEvaluated {
            reason: "no review stage produced a verdict for this topology".into(),
        }
    };
    // A SelfVerified or NotEvaluated run must not report a bare `Approved`
    // that a consumer could mistake for a passed review.
    let verdict = outcome.verdict().unwrap_or(Verdict::RevisionNeeded);

    // Fired after the derivation so the payload carries the verdict a consumer
    // will actually see, not the pre-derivation one.
    fire_hook(
        &hook_bus,
        crate::audit::HookEvent::PostTaskStop,
        serde_json::json!({
            "task_id": task.id.to_string(),
            "verdict": format!("{:?}", verdict),
            "outcome": &outcome,
            "independently_reviewed": outcome.is_independently_reviewed(),
        }),
    )?;

    Ok(PipelineResult {
        task_id: task.id,
        context_budget: state.context_budget.clone(),
        state,
        final_diff,
        diff_guardwarn,
        outcome,
        verdict,
        verdict_source,
        revision_rounds: round,
        artifacts,
        metrics,
        safety_proof: None,
        isolation,
        topology,
        topology_reason: topology_reason.clone(),
        risk_level: task_risk.level.as_str().to_string(),
        risk_rationale: task_risk.rationale.clone(),
        test_execution,
    })
}

/// Extract learnings from completed pipeline artifacts and save to role-specific memory.
fn extract_memory_from_artifacts(
    project_dir: &Path,
    task: &str,
    artifacts: &[(AgentRole, String)],
    verdict: &Verdict,
    _state: &super::state::PipelineState,
) {
    use crate::memory::append_memory;

    // 1. Planner memory: record successful decomposition patterns
    if let Some((_, planner_json)) = artifacts.iter().find(|(r, _)| *r == AgentRole::Planner)
        && let Ok(spec) = serde_json::from_str::<crate::artifacts::types::TaskSpec>(planner_json)
    {
        let tags = vec![
            "task-decomposition".into(),
            format!("complexity:{:?}", spec.estimated_complexity).to_lowercase(),
        ];
        let content = format!(
            "Task: {} → {} files to modify",
            task.chars().take(80).collect::<String>(),
            spec.files_to_modify.len(),
        );
        let _ = append_memory(project_dir, AgentRole::Planner, task, tags, content, None);
    }

    // 2. Coder memory: record revision needed patterns
    if matches!(verdict, Verdict::RevisionNeeded) {
        let content = format!(
            "Revision was needed on this task — reviewer found issues: {}",
            task.chars().take(200).collect::<String>(),
        );
        let tags = vec!["revision-needed".into(), "error-pattern".into()];
        let _ = append_memory(project_dir, AgentRole::Coder, task, tags, content, None);
    }

    // 3. If verdict is Approved, record success pattern for the Coder.
    // Phase 4.3: the content includes the task so dedupe keys are per-task
    // rather than a constant string that would otherwise accumulate.
    if matches!(verdict, Verdict::Approved) {
        let tags = vec!["success".into()];
        let content = format!(
            "Task completed successfully with Approved verdict: {}",
            task.chars().take(200).collect::<String>(),
        );
        let _ = append_memory(project_dir, AgentRole::Coder, task, tags, content, None);
    }

    // 4. Red agent: if it found adversarial issues, record them
    if let Some((_, red_json)) = artifacts.iter().find(|(r, _)| *r == AgentRole::Red)
        && let Ok(challenge) =
            serde_json::from_str::<crate::artifacts::types::RedChallenge>(red_json)
        && !challenge.challenges.is_empty()
    {
        let content = format!(
            "Adversarial challenges: {}",
            challenge
                .challenges
                .iter()
                .map(|c| c.claim.chars().take(100).collect::<String>())
                .collect::<Vec<_>>()
                .join("; ")
        );
        let tags = vec!["adversarial-finding".into()];
        let _ = append_memory(project_dir, AgentRole::Red, task, tags, content, None);
    }
}

/// Phase 4.4 distillation trigger. Stages a skill candidate only for Approved
/// runs with a green executed suite; everything else is a no-op. Best-effort
/// by contract: all failures are swallowed so distillation never fails a run.
#[allow(clippy::too_many_arguments)]
fn maybe_stage_skill_candidate(
    project_dir: &Path,
    config: &NikiConfig,
    task: &str,
    artifacts: &[(AgentRole, String)],
    verdict: &Verdict,
    test_execution: Option<&crate::agents::tester::TestExecution>,
    metrics: &[StageMetric],
    task_id: &str,
) {
    let suite_green = test_execution.map(|t| t.passed).unwrap_or(false);
    if !matches!(verdict, Verdict::Approved) || !suite_green {
        return;
    }
    let plan_shape = artifacts
        .iter()
        .find(|(r, _)| *r == AgentRole::Planner)
        .and_then(|(_, j)| serde_json::from_str::<crate::artifacts::types::TaskSpec>(j).ok())
        .map(|spec| {
            format!(
                "{} ({} files)",
                spec.summary.chars().take(200).collect::<String>(),
                spec.files_to_modify.len()
            )
        })
        .unwrap_or_default();
    let review_notes = artifacts
        .iter()
        .find(|(r, _)| *r == AgentRole::Reviewer)
        .map(|(_, j)| j.chars().take(300).collect::<String>())
        .unwrap_or_default();
    let model = metrics.first().map(|m| m.model.as_str()).unwrap_or("");
    let test_command = test_execution.map(|t| t.command.as_str()).unwrap_or("");
    let snapshot = crate::skills::head_snapshot_ref(project_dir);
    let _ = crate::skills::stage_candidate(
        project_dir,
        config,
        task,
        &plan_shape,
        test_command,
        &review_notes,
        "Approved",
        model,
        &snapshot,
        task_id,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifacts::types::{
        Complexity, IssueCategory, IssueSeverity, ReviewFeedback, ReviewIssue,
    };
    use crate::config::NikiConfig;

    #[test]
    fn default_pipeline_includes_red_before_reviewer() {
        // Red/Blue is off by default — classic 4-stage pipeline:
        // Planner → Coder → Tester → Reviewer.
        let c = NikiConfig::default();
        let s = resolve_stages(&c);
        assert_eq!(s.len(), 4);
        assert!(s.iter().any(|x| x.role == AgentRole::Planner));
        assert!(s.iter().any(|x| x.role == AgentRole::Coder));
        assert!(!s.iter().any(|x| x.role == AgentRole::Red));
        let roles: Vec<AgentRole> = s.iter().map(|x| x.role).collect();
        assert!(
            roles.iter().position(|r| *r == AgentRole::Tester).unwrap()
                < roles
                    .iter()
                    .position(|r| *r == AgentRole::Reviewer)
                    .unwrap()
        );
    }

    #[test]
    fn security_injects_auditor() {
        let mut c = NikiConfig::default();
        c.security.enabled = true;
        let s = resolve_stages(&c);
        assert!(s.iter().any(|x| x.role == AgentRole::SecurityAuditor));
    }

    fn mock_script_provider(
        dir: &std::path::Path,
        model: &str,
        text: &str,
    ) -> crate::llm::mock::MockProvider {
        let script = serde_json::json!({
            "models": { model: { "responses": [
                {"text": text, "input_tokens": 30, "output_tokens": 12}
            ] } }
        });
        let path = dir.join("mock-script.json");
        std::fs::write(&path, serde_json::to_string(&script).unwrap()).unwrap();
        crate::llm::mock::MockProvider::new(Some(path.to_str().unwrap())).unwrap()
    }

    #[tokio::test]
    async fn experimental_research_off_returns_none() {
        // Phase 3.3: default flag off → no research step (pipeline unchanged).
        let tmp = tempfile::tempdir().unwrap();
        let provider = mock_script_provider(tmp.path(), "m", "unused");
        let config = NikiConfig::default();
        assert!(!config.tools.experimental_tool_loop);
        let task = Task {
            id: uuid::Uuid::new_v4(),
            description: "do thing".into(),
            project_path: tmp.path().to_path_buf(),
        };
        let mut display = AgenticDisplay::new();
        let out =
            run_experimental_research(&provider, "m", "mock", &task, &config, &mut display, None)
                .await
                .unwrap();
        assert!(out.is_none());
    }

    #[tokio::test]
    async fn experimental_research_on_runs_loop_and_reports_usage() {
        // Phase 3.3: flag on → loop runs (mock answers immediately) and the
        // appendix + usage metric come back.
        let tmp = tempfile::tempdir().unwrap();
        let provider = mock_script_provider(tmp.path(), "m", "researched facts here");
        let mut config = NikiConfig::default();
        config.tools.experimental_tool_loop = true;
        let task = Task {
            id: uuid::Uuid::new_v4(),
            description: "do thing".into(),
            project_path: tmp.path().to_path_buf(),
        };
        let mut display = AgenticDisplay::new();
        let (appendix, metric) =
            run_experimental_research(&provider, "m", "mock", &task, &config, &mut display, None)
                .await
                .unwrap()
                .expect("flag on must run the loop");
        assert!(appendix.contains("researched facts here"), "{appendix}");
        assert_eq!(metric.input_tokens, 30);
        assert_eq!(metric.output_tokens, 12);
    }
    #[test]
    fn red_blue_injects_red_before_reviewer() {
        // Red/Blue off by default — enable to get 5-stage pipeline:
        // Planner → Coder → Tester → Red → Reviewer.
        let mut c = NikiConfig::default();
        c.red_blue.enabled = true;
        let s = resolve_stages(&c);
        assert!(c.red_blue.enabled);
        let roles: Vec<AgentRole> = s.iter().map(|x| x.role).collect();
        let red_pos = roles.iter().position(|r| *r == AgentRole::Red).unwrap();
        let reviewer_pos = roles
            .iter()
            .position(|r| *r == AgentRole::Reviewer)
            .unwrap();
        assert!(red_pos < reviewer_pos, "Red must run before the Reviewer");
    }

    #[test]
    fn red_blue_can_be_disabled() {
        let mut c = NikiConfig::default();
        c.red_blue.enabled = false;
        let s = resolve_stages(&c);
        assert!(!s.iter().any(|x| x.role == AgentRole::Red));
    }

    /// Render the reviewer prompt with and without the Red artifact to confirm
    /// the `{% if input_artifacts | length > 3 %}` reconciliation block toggles
    /// correctly (templates are only validated at runtime, not at compile time).
    #[test]
    fn reviewer_template_toggles_red_block() {
        use minijinja::Environment;
        let content = crate::load_asset("prompts/reviewer.md").unwrap();
        let mut env = Environment::new();
        env.add_template("reviewer.md", &content).unwrap();
        let tmpl = env.get_template("reviewer.md").unwrap();

        // Red and Security are named optional artifacts, not positional
        // entries. Keying the template off `input_artifacts | length` meant a
        // fourth artifact silently re-pointed `input_artifacts[3]` at whatever
        // arrived next.
        let ctx3 = minijinja::context! {
            input_artifacts => vec!["spec", "diff", "tests"],
            red_artifact => "",
            security_artifact => "",
            project_knowledge => "",
            artifact_schema => "{}",
        };
        let rendered3 = tmpl.render(ctx3).unwrap();
        assert!(
            !rendered3.contains("RECONCILE THIS"),
            "Red block must be hidden when no Red artifact is present"
        );

        let ctx4 = minijinja::context! {
            input_artifacts => vec!["spec", "diff", "tests"],
            red_artifact => "red-challenge",
            security_artifact => "",
            project_knowledge => "",
            artifact_schema => "{}",
        };
        let rendered4 = tmpl.render(ctx4).unwrap();
        assert!(
            rendered4.contains("RECONCILE THIS"),
            "Red block must appear when the Red artifact is present"
        );
    }

    /// The security block is independent of the Red block. They used to share
    /// one positional slot, so enabling the auditor displaced Red rather than
    /// joining it — and a run with both silently lost one of the two reviews.
    #[test]
    fn reviewer_template_renders_red_and_security_independently() {
        use minijinja::Environment;
        let content = crate::load_asset("prompts/reviewer.md").unwrap();
        let mut env = Environment::new();
        env.add_template("reviewer.md", &content).unwrap();
        let tmpl = env.get_template("reviewer.md").unwrap();

        let ctx = minijinja::context! {
            input_artifacts => vec!["spec", "diff", "tests"],
            red_artifact => "RED-PAYLOAD",
            security_artifact => "SECURITY-PAYLOAD",
            project_knowledge => "",
            artifact_schema => "{}",
        };
        let rendered = tmpl.render(ctx).unwrap();
        assert!(rendered.contains("RED-PAYLOAD"), "red payload missing");
        assert!(
            rendered.contains("SECURITY-PAYLOAD"),
            "security payload missing: a security audit that is not shown to \
             the Reviewer cannot be reconciled by it"
        );
        assert!(rendered.contains("Independent Security Audit"));

        // Security alone, with no Red pass.
        let ctx_sec = minijinja::context! {
            input_artifacts => vec!["spec", "diff", "tests"],
            red_artifact => "",
            security_artifact => "SECURITY-PAYLOAD",
            project_knowledge => "",
            artifact_schema => "{}",
        };
        let rendered_sec = tmpl.render(ctx_sec).unwrap();
        assert!(rendered_sec.contains("SECURITY-PAYLOAD"));
        assert!(!rendered_sec.contains("Adversarial Red Challenge"));
    }

    /// The auditor must be injected *ahead* of the Reviewer. It used to be
    /// appended, so in every run with `[security] enabled` the Reviewer
    /// finished before the auditor produced anything — the audit could never
    /// reach the reviewing agent it exists to inform.
    #[test]
    fn security_auditor_is_ordered_before_the_reviewer() {
        let mut c = NikiConfig::default();
        c.security.enabled = true;
        let s = resolve_stages(&c);
        let sec = s
            .iter()
            .position(|x| x.role == AgentRole::SecurityAuditor)
            .expect("security stage injected");
        let rev = s
            .iter()
            .position(|x| x.role == AgentRole::Reviewer)
            .expect("reviewer present");
        assert!(
            sec < rev,
            "SecurityAuditor must run before Reviewer, got order {s:?}"
        );
    }

    /// Same ordering guarantee when the auditor is injected by a High/Security
    /// risk tier rather than by config.
    #[test]
    fn risk_injected_security_auditor_precedes_the_reviewer() {
        fn risk_spec_for_auth_change() -> TaskSpec {
            let json = serde_json::json!({
                "summary": "Harden auth",
                "approach": "Validate tokens",
                "files_to_modify": [
                    {"path": "src/auth.rs", "action": "modify", "description": "validate tokens"}
                ],
                "acceptance_criteria": ["tokens are validated"],
                "constraints": [],
                "estimated_complexity": "medium",
                "uncertainties": null,
            });
            serde_json::from_value(json).expect("spec parses")
        }
        let mut c = NikiConfig::default();
        c.security.enabled = false;
        // Force the tier rather than trying to coax it out of the classifier's
        // heuristics — the assertion is about *ordering*, not about what makes
        // a task risky.
        c.risk.mode = crate::config::types::RiskMode::High;
        let base = resolve_stages(&c);
        let risk = crate::risk::classify(&risk_spec_for_auth_change(), &c);
        let s = apply_risk_stages(base, &risk, &c);
        let sec = s
            .iter()
            .position(|x| x.role == AgentRole::SecurityAuditor)
            .expect("risk tier injects a security stage");
        let rev = s
            .iter()
            .position(|x| x.role == AgentRole::Reviewer)
            .expect("reviewer present");
        assert!(sec < rev, "risk-injected auditor must precede the reviewer");
    }

    /// A security rejection has to survive a later Reviewer approval.
    #[test]
    fn a_security_rejection_is_not_overturned_by_a_reviewer_approval() {
        let (mut verdict, mut source) = (Verdict::Approved, None);
        let mut hold = false;

        apply_security_verdict(Verdict::Rejected, &mut verdict, &mut source, &mut hold);
        assert!(hold, "a rejection must set the hold");
        assert_eq!(verdict, Verdict::RevisionNeeded);
        assert_eq!(source.as_deref(), Some("security-auditor"));

        // The Reviewer then approves. The security rejection stands.
        apply_reviewer_verdict(Verdict::Approved, &mut verdict, &mut source, hold);
        assert_eq!(
            verdict,
            Verdict::RevisionNeeded,
            "a security rejection must not be overturned by a later approval"
        );
        assert_eq!(
            source.as_deref(),
            Some("security-auditor"),
            "the record must still show security as the reason"
        );
    }

    /// A hold the auditor itself can lift.
    ///
    /// The auditor re-reads the revised code every round, so an explicit
    /// `Approved` means the finding it raised is addressed. The hold used to be
    /// a latch that nothing could clear: a run that fixed its security finding
    /// still burned every remaining round and reported `RevisionNeeded` for
    /// ever, which is a false alarm the user can do nothing about.
    #[test]
    fn an_auditor_approval_clears_its_own_hold() {
        let (mut verdict, mut source) = (Verdict::Approved, None);
        let mut hold = false;

        apply_security_verdict(Verdict::Rejected, &mut verdict, &mut source, &mut hold);
        assert!(hold);

        // Round 2: the auditor reads the revision and finds it clean.
        apply_security_verdict(Verdict::Approved, &mut verdict, &mut source, &mut hold);
        assert!(!hold, "the auditor must be able to clear its own hold");

        // And the Reviewer's verdict is free to take the gate again — the
        // reviewer's independence from the auditor is untouched.
        apply_reviewer_verdict(Verdict::Approved, &mut verdict, &mut source, hold);
        assert_eq!(verdict, Verdict::Approved);
        assert_eq!(source.as_deref(), Some("reviewer"));
    }

    /// An auditor that still wants a revision keeps the hold — only an explicit
    /// approval lifts it.
    /// One allowance, shared: mixing causes does not buy extra turns.
    ///
    /// A per-cause counter is how a run ends up burning six attempts on a stage
    /// that is failing once — three budgets were patched in separately, each
    /// blind to the others. Aider keeps a single `max_reflections` for the
    /// whole apply → lint → test → re-apply cycle for exactly this reason.
    ///
    /// The test rotates through the causes in every order, because a budget
    /// that only behaves when one cause happens to come first is not shared.
    #[test]
    fn one_allowance_is_shared_across_every_cause() {
        let causes = [
            FeedbackCause::UnappliablePatch,
            FeedbackCause::Truncated,
            FeedbackCause::MalformedArtifact,
        ];

        for rotation in 0..causes.len() {
            let mut b = FeedbackBudget::new();
            let mut sends = 0;
            for i in 0..12 {
                let cause = causes[(i + rotation) % causes.len()];
                if matches!(b.take("because", cause), StageFeedback::Send { .. }) {
                    sends += 1;
                }
            }
            assert_eq!(
                sends, MAX_STAGE_FEEDBACK as usize,
                "rotation {rotation}: the total must not depend on which causes are mixed"
            );
            assert_eq!(b.used(), 12, "but every attempt is still counted");
        }
    }

    /// One cause alone gets the same allowance, and no more.
    #[test]
    fn a_single_cause_cannot_exceed_the_shared_allowance() {
        let mut b = FeedbackBudget::new();
        for _ in 0..MAX_STAGE_FEEDBACK {
            assert!(matches!(
                b.take("x", FeedbackCause::UnappliablePatch),
                StageFeedback::Send { .. }
            ));
        }
        assert!(
            matches!(
                b.take("x", FeedbackCause::UnappliablePatch),
                StageFeedback::Stop { .. }
            ),
            "and the next one stops, whatever the cause"
        );
    }

    /// The stop message names what went wrong, not just the count.
    ///
    /// With per-cause budgets, "stopped" named whichever ran out first rather
    /// than what the run was doing. A user reading it has to be able to tell a
    /// token-limit problem from an anchoring problem, because they need a
    /// different model or a different prompt.
    #[test]
    fn the_stop_message_names_the_cause_not_just_the_count() {
        let mut b = FeedbackBudget::new();
        let mut last = None;
        for _ in 0..=MAX_STAGE_FEEDBACK {
            last = Some(b.take(
                "edits[1] did not match anything in the file.",
                FeedbackCause::UnappliablePatch,
            ));
        }
        let msg = last.unwrap().stop_message().expect("a stop has a message");
        assert!(msg.contains("the patch did not apply"), "{msg}");
        assert!(
            msg.contains("did not match"),
            "and repeats the model's own words: {msg}"
        );

        let mut b = FeedbackBudget::new();
        let mut last = None;
        for _ in 0..=MAX_STAGE_FEEDBACK {
            last = Some(b.take("hit the output token limit", FeedbackCause::Truncated));
        }
        let msg = last.unwrap().stop_message().expect("a stop has a message");
        assert!(msg.contains("cut off at the token limit"), "{msg}");
        assert!(
            !msg.contains("the patch did not apply"),
            "a truncation must not be reported as an anchoring problem: {msg}"
        );
    }

    /// The loop's recovery turns come off the same allowance.
    ///
    /// The Coder's tool loop spends turns of its own when a response is cut off
    /// at the token limit. Those and the pipeline's unappliable-patch retries are
    /// the same kind of event — "tell the model what went wrong and try again" —
    /// so a stage that hit both must not get the sum of the two budgets. This
    /// is the wiring that `RoleRun::feedback_turns` exists for.
    #[test]
    fn a_loop_that_already_recovered_leaves_less_for_the_pipeline() {
        // A loop that spent both turns on truncated answers.
        let mut after = FeedbackBudget::new();
        after.note_prior(2);
        assert!(
            matches!(
                after.take("x", FeedbackCause::UnappliablePatch),
                StageFeedback::Stop { .. }
            ),
            "with the allowance already spent, the pipeline must not grant another"
        );

        // A loop that recovered once leaves exactly one.
        let mut one_left = FeedbackBudget::new();
        one_left.note_prior(1);
        assert!(matches!(
            one_left.take("x", FeedbackCause::UnappliablePatch),
            StageFeedback::Send { .. }
        ));
        assert!(
            matches!(
                one_left.take("x", FeedbackCause::UnappliablePatch),
                StageFeedback::Stop { .. }
            ),
            "and then the allowance is gone"
        );

        // A loop that never recovered changes nothing.
        let mut untouched = FeedbackBudget::new();
        assert!(matches!(
            untouched.take("x", FeedbackCause::UnappliablePatch),
            StageFeedback::Send { .. }
        ));
    }

    /// Folding in another component's spend must never hand out fresh turns.
    ///
    /// The bug this guards: the budget was re-created from the loop's turn
    /// count on *every* patch failure, so it restarted at zero each time, a
    /// stage whose patch never applied looped for ever, and the suite showed a
    /// test binary still running after ten minutes printing "attempt 1 of 2".
    /// `note_prior` raises the total instead, so a repeat cannot reset it.
    #[test]
    fn folding_in_another_spend_never_resets_the_allowance() {
        // `max`, not `+`, and the distinction matters.
        //
        // The loop reports a *cumulative* count of the same feedback turns the
        // pipeline is counting. Adding the two would count every turn twice
        // and halve the effective allowance; taking the larger is correct
        // because they are two views of one number. A first version of this
        // test asserted they should add, and was wrong.
        let mut b = FeedbackBudget::new();
        // The pipeline spends one of its own.
        assert!(matches!(
            b.take("mine", FeedbackCause::UnappliablePatch),
            StageFeedback::Send { .. }
        ));
        // The loop now reports one as well — the same turn, seen from the other
        // side. It must not double it, and it must not reset it either.
        b.note_prior(1);
        assert_eq!(b.used(), 1, "one turn, counted once");

        // A genuinely larger report does raise the total, and the allowance
        // shrinks by exactly that much.
        let mut spent = FeedbackBudget::new();
        spent.note_prior(MAX_STAGE_FEEDBACK);
        assert_eq!(spent.used(), MAX_STAGE_FEEDBACK);
        assert!(
            matches!(
                spent.take("x", FeedbackCause::UnappliablePatch),
                StageFeedback::Stop { .. }
            ),
            "a loop that used the whole allowance leaves none"
        );

        // A lower report cannot give turns back.
        let mut c = FeedbackBudget::new();
        c.note_prior(MAX_STAGE_FEEDBACK);
        c.note_prior(0);
        assert_eq!(c.used(), MAX_STAGE_FEEDBACK, "the total never goes down");
    }

    /// A send is not a stop; conflating them would warn on every retry.
    #[test]
    fn a_send_is_not_a_stop() {
        let mut b = FeedbackBudget::new();
        let sent = b.take("anything", FeedbackCause::Truncated);
        assert!(sent.stop_message().is_none());
    }

    #[test]
    fn a_revision_with_no_issues_is_not_worth_a_round() {
        let feedback = ReviewFeedback {
            critical_issues: vec![],
            guidance: "looks a bit off".into(),
            keep_unchanged: vec![],
            revision_round: 0,
        };
        assert_eq!(
            revision_hold(None, issue_signature(&feedback.critical_issues).as_deref()),
            RevisionHold::NoActionableIssues
        );
    }

    /// No feedback at all is the same case: nothing was said, so nothing can
    /// be acted on.
    #[test]
    fn a_revision_with_no_feedback_is_not_worth_a_round() {
        assert_eq!(revision_hold(None, None), RevisionHold::NoActionableIssues);
    }

    /// A round that comes back with the identical critique changed nothing, so
    /// a second one would not either.
    #[test]
    fn an_unchanged_critique_stops_the_loop() {
        let issues = vec![ReviewIssue {
            severity: IssueSeverity::Major,
            category: IssueCategory::Bug,
            file_path: Some("src/lib.rs".into()),
            line_range: None,
            description: "off-by-one".into(),
            suggested_fix: None,
        }];
        let first = issue_signature(&issues).expect("one issue is not empty");
        assert_eq!(
            revision_hold(None, Some(first.as_str())),
            RevisionHold::Continue
        );
        assert_eq!(
            revision_hold(Some(&first), Some(&first)),
            RevisionHold::UnchangedIssues
        );
    }

    /// A genuinely new critique runs the next round.
    #[test]
    fn a_changed_critique_continues_the_loop() {
        let a = issue_signature(&[ReviewIssue {
            severity: IssueSeverity::Major,
            category: IssueCategory::Bug,
            file_path: Some("src/lib.rs".into()),
            line_range: None,
            description: "off-by-one".into(),
            suggested_fix: None,
        }])
        .expect("one issue is not empty");
        let b = issue_signature(&[ReviewIssue {
            severity: IssueSeverity::Critical,
            category: IssueCategory::Security,
            file_path: Some("src/auth.rs".into()),
            line_range: None,
            description: "token compared with ==, not constant-time".into(),
            suggested_fix: None,
        }])
        .expect("one issue is not empty");
        assert_eq!(revision_hold(Some(&a), Some(&b)), RevisionHold::Continue);
    }

    #[test]
    fn an_advisory_security_verdict_does_not_take_the_gate() {
        let (mut verdict, mut source) = (Verdict::RevisionNeeded, None);
        let mut hold = false;
        apply_security_verdict(Verdict::Approved, &mut verdict, &mut source, &mut hold);
        assert!(!hold, "an advisory pass must not hold the run");
        assert_eq!(source, None, "an advisory pass names nobody as the source");

        // And the Reviewer is then free to set the verdict normally.
        apply_reviewer_verdict(Verdict::Approved, &mut verdict, &mut source, hold);
        assert_eq!(verdict, Verdict::Approved);
        assert_eq!(source.as_deref(), Some("reviewer"));
    }

    /// The isolation record is a description of the run. When the Reviewer is
    /// given the security artifact, the table has to say so.
    #[test]
    fn isolation_record_names_the_security_auditor_when_it_ran() {
        assert!(
            !isolation_sources_for(AgentRole::Reviewer, false, false)
                .contains(&AgentRole::SecurityAuditor)
        );
        assert!(
            isolation_sources_for(AgentRole::Reviewer, false, true)
                .contains(&AgentRole::SecurityAuditor),
            "a Reviewer that saw the security audit must record it as a source"
        );
    }

    #[test]
    fn parallel_injects_synthesizer() {
        let mut c = NikiConfig::default();
        c.parallel.enabled = true;
        c.parallel.coder_count = 3;
        let s = resolve_stages(&c);
        assert!(s.iter().any(|x| x.role == AgentRole::Synthesizer));
    }

    #[test]
    fn required_tools_includes_base_set() {
        let c = NikiConfig::default();
        let t = required_tools(&c);
        assert!(t.iter().any(|p| p == "git"));
        assert!(t.iter().any(|p| p == "python3"));
    }

    // ── Adaptive topology (BUILD_PLAN 3.2, P2.2) ───────────────────────────

    fn spec_with(c: Complexity) -> TaskSpec {
        TaskSpec {
            summary: String::new(),
            approach: String::new(),
            files_to_modify: vec![],
            acceptance_criteria: vec![],
            constraints: vec![],
            estimated_complexity: c,
            uncertainties: None,
        }
    }

    #[test]
    fn select_topology_auto_low_with_an_unmeasured_model_uses_multi_agent() {
        // This test used to assert the opposite, and it was not a neutral
        // assertion: it encoded the bug. `Auto` looked only at the task, so a
        // low-complexity task collapsed to a single agent whatever was running
        // it — and the compute-matched ablation (arXiv:2512.08296, 260 configs,
        // SWE-bench Verified included) says a multi-agent pipeline is worth
        // about +22 points below a 45% single-agent baseline and costs about 5
        // above 50%. NIKI's own zero-setup path is a 3B local model, so the
        // heuristic handed the structure-hungry model the opposite of the
        // structure it needed, and a live run against it died at the Coder.
        //
        // The fast path is still correct for a *measured strong* model, and
        // `select_topology_auto_low_with_a_measured_strong_model_uses_single_agent`
        // covers that.
        let mut c = NikiConfig::default();
        c.pipeline.topology = TopologyMode::Auto;
        let spec = spec_with(Complexity::Low);
        assert_eq!(select_topology(&spec, &c), TopologyMode::MultiAgent);
    }

    #[test]
    fn select_topology_auto_low_with_a_measured_strong_model_uses_single_agent() {
        // The other half: the change is not "always multi". A frontier model on
        // a simple task should still take the fast path, because the same study
        // says structure costs it about 5 points.
        let dir = tempfile::tempdir().expect("tempdir");
        crate::config::capability::save(
            dir.path(),
            crate::config::capability::ModelCapability::Measured {
                passed: 9,
                total: 10,
            },
        )
        .expect("save");
        let mut c = NikiConfig {
            project_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        c.pipeline.topology = TopologyMode::Auto;
        let spec = spec_with(Complexity::Low);
        assert_eq!(select_topology(&spec, &c), TopologyMode::SingleAgent);
    }

    #[test]
    fn high_risk_forces_multiagent_under_auto_only() {
        // Phase 5.7: Auto + High/Security upgrades SingleAgent; explicit
        // topologies and lower tiers are untouched.
        use crate::risk::RiskLevel;
        assert!(force_multiagent_for_high_risk(
            TopologyMode::SingleAgent,
            TopologyMode::Auto,
            RiskLevel::High
        ));
        assert!(force_multiagent_for_high_risk(
            TopologyMode::SingleAgent,
            TopologyMode::Auto,
            RiskLevel::Security
        ));
        assert!(!force_multiagent_for_high_risk(
            TopologyMode::SingleAgent,
            TopologyMode::Auto,
            RiskLevel::Normal
        ));
        assert!(!force_multiagent_for_high_risk(
            TopologyMode::SingleAgent,
            TopologyMode::SingleAgent,
            RiskLevel::High
        ));
        assert!(!force_multiagent_for_high_risk(
            TopologyMode::MultiAgent,
            TopologyMode::Auto,
            RiskLevel::High
        ));
    }

    #[test]
    fn topology_reason_names_the_collapse_and_what_it_cost() {
        // A silent fast-path collapse is a vision violation; the reason string
        // must name what was dropped.
        //
        // `Auto` with no measurement now keeps the chain, so the collapse is
        // only reachable with a model measured strong — which is the case where
        // it is the right call, and the one the user needs to be able to see
        // happen.
        let dir = tempfile::tempdir().expect("tempdir");
        crate::config::capability::save(
            dir.path(),
            crate::config::capability::ModelCapability::Measured {
                passed: 9,
                total: 10,
            },
        )
        .expect("save");
        let mut c = NikiConfig {
            project_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        c.pipeline.topology = TopologyMode::Auto;

        let reason = topology_reason(&spec_with(Complexity::Low), &c);
        assert!(
            reason.contains("collapsed to fast-path"),
            "reason: {reason}"
        );
        assert!(
            reason.contains("no independent Tester/Reviewer/Red"),
            "the reason must name what the collapse dropped: {reason}"
        );

        // And the unmeasured case must say *why* it kept the chain, and what
        // would let it collapse.
        let mut unknown = NikiConfig::default();
        unknown.pipeline.topology = TopologyMode::Auto;
        let kept = topology_reason(&spec_with(Complexity::Low), &unknown);
        assert!(kept.contains("not measured"), "reason: {kept}");
        assert!(kept.contains("doctor --measure"), "reason: {kept}");

        let reason_multi = topology_reason(&spec_with(Complexity::High), &c);
        assert!(
            reason_multi.contains("full multi-agent chain"),
            "reason: {reason_multi}"
        );
    }

    #[test]
    fn red_evidence_json_strips_coder_rationale() {
        let coder = serde_json::json!({
            "edits": [{"search": "a", "replace": "b"}],
            "files_changed": [{"path": "x.rs", "action": "modify", "language": "rust"}],
            "implementation_notes": "I chose b because it felt right",
            "spec_adherence": "Trust me",
            "uncertainties": ["not sure about edge cases"]
        })
        .to_string();
        let evidence = red_evidence_json(&coder);
        assert!(evidence.contains("\"edits\""), "evidence keeps edits");
        assert!(evidence.contains("x.rs"), "evidence keeps files");
        assert!(!evidence.contains("felt right"), "rationale withheld");
        assert!(
            !evidence.contains("Trust me"),
            "self-justification withheld"
        );
        assert!(!evidence.contains("not sure"), "uncertainties withheld");
    }

    #[test]
    fn red_evidence_json_falls_back_on_unparseable_input() {
        let raw = "not json at all";
        assert_eq!(red_evidence_json(raw), raw);
    }

    #[test]
    fn isolation_record_matches_wiring() {
        // The record must mirror input_artifacts wiring, not aspiration:
        // Synthesizer reconciles concatenated coder diffs; the auditor sees
        // spec + coder diff only (never Tester/Reviewer/Red).
        assert_eq!(
            isolation_sources_for(AgentRole::Synthesizer, false, false),
            vec![AgentRole::Planner, AgentRole::Coder]
        );
        assert_eq!(
            isolation_sources_for(AgentRole::SecurityAuditor, true, false),
            vec![AgentRole::Planner, AgentRole::Coder]
        );
    }

    #[test]
    fn select_topology_auto_medium_uses_multi_agent() {
        // The default single-agent threshold is Low, so Medium breaching it
        // routes to the full multi-agent chain.
        let c = NikiConfig::default();
        let spec = spec_with(Complexity::Medium);
        assert_eq!(select_topology(&spec, &c), TopologyMode::MultiAgent);
    }

    #[test]
    fn select_topology_auto_high_uses_multi_agent() {
        let c = NikiConfig::default();
        let spec = spec_with(Complexity::High);
        assert_eq!(select_topology(&spec, &c), TopologyMode::MultiAgent);
    }

    #[test]
    fn select_topology_auto_low_with_security_forces_multi() {
        // A solo Coder can't run an independent Security Auditor, so security
        // on forces the multi-agent topology even for low-complexity tasks.
        let mut c = NikiConfig::default();
        c.security.enabled = true;
        let spec = spec_with(Complexity::Low);
        assert_eq!(select_topology(&spec, &c), TopologyMode::MultiAgent);
    }

    #[test]
    fn select_topology_auto_low_with_parallel_forces_multi() {
        // Parallel coders need the multi-agent orchestration path.
        let mut c = NikiConfig::default();
        c.parallel.enabled = true;
        c.parallel.coder_count = 2;
        let spec = spec_with(Complexity::Low);
        assert_eq!(select_topology(&spec, &c), TopologyMode::MultiAgent);
    }

    #[test]
    fn select_topology_explicit_overrides_auto() {
        let mut single = NikiConfig::default();
        single.pipeline.topology = TopologyMode::SingleAgent;
        // High complexity would otherwise pick multi-agent, but explicit wins.
        assert_eq!(
            select_topology(&spec_with(Complexity::High), &single),
            TopologyMode::SingleAgent
        );

        let mut multi = NikiConfig::default();
        multi.pipeline.topology = TopologyMode::MultiAgent;
        // Low complexity would otherwise pick single-agent, but explicit wins.
        assert_eq!(
            select_topology(&spec_with(Complexity::Low), &multi),
            TopologyMode::MultiAgent
        );
    }

    #[test]
    fn parallel_spend_cap_sums_coder_costs() {
        // Phase 5.5: the hard ceiling the parallel-coder hole-close relies
        // on — N coder metrics are summed, so parallel mode honors the cap.
        let metric = |cost: f64| StageMetric {
            role: AgentRole::Coder,
            provider: "mock".into(),
            model: "m".into(),
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: 0,
            reasoning_tokens: 0,
            latency_ms: 0,
            cost_usd: cost,
            retry_count: 0,
            ttft_ms: 0,
        };
        let metrics = vec![metric(0.05), metric(0.05), metric(0.05)];
        assert!(enforce_spend_cap(0.10, &metrics).is_err());
        assert!(enforce_spend_cap(0.20, &metrics).is_ok());
        assert!(enforce_spend_cap(0.0, &metrics).is_ok(), "0 disables");
    }

    #[test]
    fn body_stages_for_single_agent_keeps_only_coder() {
        let stages = [
            PipelineStageConfig {
                role: AgentRole::Coder,
                provider: "a".into(),
                model: "m".into(),
                skip: false,
                max_tokens: 0,
                temperature: 0.0,
                fallbacks: Vec::new(),
                reasoning_effort: None,
            },
            PipelineStageConfig {
                role: AgentRole::Tester,
                provider: "a".into(),
                model: "m".into(),
                skip: false,
                max_tokens: 0,
                temperature: 0.0,
                fallbacks: Vec::new(),
                reasoning_effort: None,
            },
            PipelineStageConfig {
                role: AgentRole::Reviewer,
                provider: "a".into(),
                model: "m".into(),
                skip: false,
                max_tokens: 0,
                temperature: 0.0,
                fallbacks: Vec::new(),
                reasoning_effort: None,
            },
        ];
        // Single-agent collapses everything but the Coder.
        let refs: Vec<&PipelineStageConfig> = stages.iter().collect();

        let solo = body_stages_for(TopologyMode::SingleAgent, &refs);
        assert_eq!(solo.len(), 1);
        assert_eq!(solo[0].role, AgentRole::Coder);

        // Multi-agent passes every body stage through unchanged.
        let multi = body_stages_for(TopologyMode::MultiAgent, &refs);
        assert_eq!(multi.len(), stages.len());
    }

    #[test]
    fn resolve_stages_honors_effort_preset_and_explicit_override() {
        let mut config = NikiConfig::default();
        config.agents.coder.effort = Some("low".to_string());
        config.agents.coder.max_tokens = 0;
        config.agents.coder.temperature = 0.0;

        let stages = resolve_stages(&config);
        let coder = stages.iter().find(|s| s.role == AgentRole::Coder).unwrap();
        assert_eq!(
            coder.max_tokens, 4096,
            "low effort preset resolves to 4096 tokens"
        );
        assert_eq!(
            coder.temperature, 0.0,
            "low effort preset resolves to 0.0 temperature"
        );

        // Explicit override wins over effort preset
        config.agents.coder.max_tokens = 2048;
        config.agents.coder.temperature = 0.7;
        let stages2 = resolve_stages(&config);
        let coder2 = stages2.iter().find(|s| s.role == AgentRole::Coder).unwrap();
        assert_eq!(
            coder2.max_tokens, 2048,
            "explicit max_tokens overrides effort"
        );
        assert_eq!(
            coder2.temperature, 0.7,
            "explicit temperature overrides effort"
        );
    }
}
