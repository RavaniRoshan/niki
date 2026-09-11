use crate::artifacts::types::AgentRole;
use crate::cost::lookup_price;
use crate::recommend::{
    RoleRec, estimate_cost, estimate_tokens, observed_spend, recommendations, role_prefers_strong,
};
use anyhow::Result;
use clap::Args;

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

pub fn handle(args: &RecommendArgs) -> Result<()> {
    let recs = recommendations();

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
