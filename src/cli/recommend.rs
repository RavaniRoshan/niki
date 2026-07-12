use anyhow::Result;
use clap::Args;
use crate::artifacts::types::AgentRole;
use crate::cost::lookup_price;
use crate::recommend::{estimate_cost, estimate_tokens, recommendations, role_prefers_strong, RoleRec};

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
}

fn role_name(role: AgentRole) -> &'static str {
    match role {
        AgentRole::Planner => "planner",
        AgentRole::Coder => "coder",
        AgentRole::Tester => "tester",
        AgentRole::Reviewer => "reviewer",
        AgentRole::Synthesizer => "synthesizer",
        AgentRole::SecurityAuditor => "security_auditor",
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

    println!("Tip: set these via `[agents]` in niki.toml, or override per run with `--coder-model`, etc.");
    Ok(())
}
