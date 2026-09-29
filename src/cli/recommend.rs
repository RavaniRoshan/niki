use crate::artifacts::types::AgentRole;
use crate::cost::lookup_price;
use crate::recommend::{
    RoleRec, estimate_cost, estimate_tokens, observed_spend, recommendations, role_prefers_strong,
};
use anyhow::Result;
use clap::Args;
use std::collections::HashMap;

/// Fetch every configured provider's catalogue, keyed by provider name.
///
/// Failures are dropped rather than propagated: a catalogue is advice, and a
/// provider that cannot be reached must not stop the rest of the report.
async fn load_catalogues() -> HashMap<String, Vec<crate::cli::catalogue::CatalogueEntry>> {
    use crate::cli::catalogue;
    let Ok(config) = crate::config::NikiConfig::load(std::path::Path::new(".")) else {
        return HashMap::new();
    };
    let mut out = HashMap::new();
    for (name, cfg) in &config.providers {
        let base = cfg
            .base_url
            .clone()
            .or_else(|| crate::llm::provider::default_base_url(name).map(str::to_string));
        let key = cfg
            .api_key
            .clone()
            .or_else(|| crate::cli::auth::resolve_api_key(name));
        if let Ok(models) = catalogue::fetch(name, base.as_deref(), key.as_deref()).await {
            out.insert(name.clone(), models);
        }
    }
    out
}

#[derive(Args)]
pub struct RecommendArgs {
    /// Only recommend for this role (planner | coder | tester | reviewer |
    /// synthesizer | security_auditor). Defaults to all roles.
    #[arg(long)]
    pub role: Option<String>,

    /// Describe the task to get a per-run cost estimate (rough heuristic).
    #[arg(long)]
    pub task: Option<String>,

    /// Preference: `balanced` (default), `strong`, or `cheap`.
    #[arg(long, default_value = "balanced")]
    pub preference: String,

    /// Project directory whose `.niki/tasks` history informs observed spend.
    #[arg(short, long, default_value = ".")]
    pub project: String,
}

fn role_name(role: AgentRole) -> &'static str {
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

fn fmt(pm: (&'static str, &'static str)) -> String {
    format!("{} ({})", pm.1, pm.0)
}

/// What to tell a user whose recommendation this provider cannot serve.
///
/// Kept separate from the printing so the wording is testable: it is the part
/// that quietly rots, and the part a user reads when deciding whether to trust
/// the rest of the report.
pub fn not_offered_lines(
    provider: &str,
    model: &str,
    catalogue: Option<&[crate::cli::catalogue::CatalogueEntry]>,
) -> Vec<String> {
    let alts = crate::recommend::suggestions(catalogue, model, 3);
    if alts.is_empty() {
        vec![format!(
            "**Not offered by `{provider}`.** The table is a static opinion about models \
             that existed when it was written; this provider's catalogue does not list \
             `{model}`."
        )]
    } else {
        vec![format!(
            "**Not offered by `{provider}`.** Candidates from its catalogue: {}",
            alts.join(", ")
        )]
    }
}

pub async fn handle(args: &RecommendArgs) -> Result<()> {
    let recs = recommendations();
    // Read each provider's real catalogue once, so the advice below is checked
    // rather than asserted. Best effort: a provider with no `/models`
    // endpoint, or a key that cannot read one, leaves `Unknown`, which is
    // printed as silence rather than as a claim.
    // Fetching a catalogue is async, and this is the shape the other commands
    // that need the network already use (`eval`, `plan`, `goal`). A nested
    // `block_on` from inside `main`'s runtime panics — including via
    // `Handle::block_on` — so the command is async rather than working around
    // it.
    let catalogues = load_catalogues().await;

    let pref = args.preference.to_lowercase();
    let filtered: Vec<&RoleRec> = match &args.role {
        Some(r) => recs
            .iter()
            .filter(|x| role_name(x.role) == r.to_lowercase())
            .collect(),
        None => recs.iter().collect(),
    };
    if filtered.is_empty() {
        eprintln!(
            "No recommendation for role '{}'. Valid: planner, coder, tester, reviewer, synthesizer, security_auditor.",
            args.role.as_deref().unwrap_or("")
        );
        std::process::exit(2);
    }

    let (est_in, est_out) = estimate_tokens(args.task.as_deref());

    println!("# NIKI Model Recommendations\n");
    println!(
        "Preference: `{}` · est. tokens/run: {} in / {} out\n",
        pref, est_in, est_out
    );

    for rec in filtered {
        let chosen = match pref.as_str() {
            "strong" => rec.strong,
            "cheap" => rec.cheap,
            _ => {
                if role_prefers_strong(rec.role) {
                    rec.strong
                } else {
                    rec.cheap
                }
            }
        };

        println!("## {}  (`{}`)", role_name(rec.role), chosen.0);
        println!("  - Recommended now: **{}** (`{}`)", chosen.1, chosen.0);
        println!(
            "  - Strong: {}  ·  Cheap: {}",
            fmt(rec.strong),
            fmt(rec.cheap)
        );
        println!("  - Why: {}", rec.rationale);

        // The check that matters: is this model actually reachable?
        let catalogue = catalogues.get(chosen.0).map(|v| &v[..]);
        match crate::recommend::availability(catalogue, chosen.1) {
            crate::recommend::Availability::Offered => {}
            crate::recommend::Availability::NotOffered => {
                for line in not_offered_lines(chosen.0, chosen.1, catalogue) {
                    println!("  - {line}");
                }
            }
            crate::recommend::Availability::Unknown => {}
        }

        let cost = estimate_cost(chosen.0, chosen.1, est_in, est_out);
        match lookup_price(chosen.0, chosen.1) {
            Some(_) => println!("  - Est. cost/run: ${:.4}", cost),
            None => println!("  - Est. cost/run: $0.0000 (local / unknown model)"),
        }
        println!();
    }

    // Observed spend from this project's own history. Static heuristics above
    // are generic; these numbers are what past runs actually cost here.
    let tasks_dir = std::path::Path::new(&args.project).join(".niki/tasks");
    let observed: Vec<_> = observed_spend(&tasks_dir)
        .into_iter()
        .filter(|o| match &args.role {
            Some(r) => role_name(o.role) == r.to_lowercase(),
            None => true,
        })
        .collect();
    println!("## Observed in past runs  (`{}`)", tasks_dir.display());
    if observed.is_empty() {
        println!("  - No past runs found — figures above are static heuristics.");
    } else {
        let total_runs: usize = observed.iter().map(|o| o.runs).sum();
        println!(
            "  - Aggregated from {} stage executions in past runs.",
            total_runs
        );
        for o in observed {
            let unpriced_note =
                if o.avg_cost_usd == 0.0 && crate::cost::is_unpriced(&o.provider, &o.model) {
                    " — unpriced model, spend unmeasured"
                } else {
                    ""
                };
            println!(
                "  - {} {} ({}): {} run(s), avg ${:.4} ({} in / {} out tok){}",
                role_name(o.role),
                o.model,
                o.provider,
                o.runs,
                o.avg_cost_usd,
                o.avg_input_tokens as u64,
                o.avg_output_tokens as u64,
                unpriced_note,
            );
        }
    }
    println!();

    println!(
        "Tip: set these via `[agents]` in niki.toml, or override per run with `--coder-model`, etc."
    );
    Ok(())
}
