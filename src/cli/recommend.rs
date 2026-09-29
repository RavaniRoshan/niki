use crate::artifacts::types::AgentRole;
use crate::cost::lookup_price;
use crate::recommend::{
    RoleRec, estimate_cost, estimate_tokens, observed_spend, recommendations, role_prefers_strong,
};
use anyhow::Result;
use clap::Args;
use std::collections::HashMap;

/// A provider's catalogue, and whether we managed to read one.
pub type Catalogues = HashMap<String, Vec<crate::cli::catalogue::CatalogueEntry>>;

/// Every provider this configuration can actually reach a model on.
///
/// `[providers]` is not the whole story and used to be treated as if it were.
/// A user who exports `OPENROUTER_API_KEY` and never writes a `[providers]`
/// block — which the README tells them is enough — has a working provider,
/// and it is the one their agents are pointed at. Probing only the config
/// table reported that account as having no provider at all.
///
/// So the agent roles are walked as well, and their `fallbacks` with them:
/// a recommendation is only useful if it survives the provider chain the run
/// will actually take.
fn probe_targets(config: &crate::config::NikiConfig) -> Vec<String> {
    use std::collections::BTreeSet;
    // `config.providers` has a slot for every known slug whether or not
    // anyone filled it in — see `ProviderConfig::is_configured`. Counting
    // slots made this report "12 of 12" for a fresh install, which put the
    // Unverified banner on every user's advice permanently and fired twelve
    // catalogue requests at an account that has none.
    let mut names: BTreeSet<String> = config
        .providers
        .iter()
        .filter(|(_, cfg)| cfg.is_configured())
        .map(|(name, _)| name.clone())
        .collect();
    for name in crate::config::types::AgentsConfig::NAMES {
        let Some(agent) = config.agents.agent_named(name) else {
            continue;
        };
        if !agent.provider.is_empty() {
            names.insert(agent.provider.clone());
        }
        names.extend(agent.fallbacks.iter().cloned());
    }
    names.into_iter().collect()
}

/// Fetch a catalogue for every provider this configuration can reach, plus how
/// many were probed.
///
/// The count is the point. Fetch failures are dropped rather than propagated —
/// a catalogue is advice, and a provider that cannot be reached must not stop
/// the rest of the report — but dropping them silently is what made this
/// command lie. A dropped failure and a configuration that names no provider
/// used to look identical from here: an empty map, `Unknown` for every model,
/// and a report that printed an unrunnable recommendation with the same
/// confident formatting as a verified one. The caller needs both numbers to
/// say which of those happened.
async fn load_catalogues(project: &std::path::Path) -> (Catalogues, usize) {
    use crate::cli::catalogue;
    let Ok(config) = crate::config::NikiConfig::load(project) else {
        return (HashMap::new(), 0);
    };
    let targets = probe_targets(&config);
    let configured = targets.len();
    let mut out = HashMap::new();
    for name in targets {
        let base = config
            .providers
            .get(&name)
            .and_then(|c| c.base_url.clone())
            .or_else(|| crate::llm::provider::default_base_url(&name).map(str::to_string));
        let key = config
            .providers
            .get(&name)
            .and_then(|c| c.api_key.clone())
            .or_else(|| crate::cli::auth::resolve_api_key(&name));
        if let Ok(models) = catalogue::fetch(&name, base.as_deref(), key.as_deref()).await {
            out.insert(name, models);
        }
    }
    (out, configured)
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

/// The line that has to appear when this report could not check its own advice.
///
/// The recommendation table is an opinion about models that existed when it
/// was written, and on a provider that fronts hundreds under its own names it
/// is wrong more often than right. Checking it against a live catalogue is
/// what makes it advice rather than folklore — and the check fails *open* by
/// design, because a provider with no `/models` endpoint, or a key that
/// cannot read one, is normal and says nothing about what exists.
///
/// "Fails open" is only safe if the user is told it failed open. Before this,
/// every one of those cases printed the same confident report as a fully
/// verified one, so a user on OpenRouter with an unreadable key was told to
/// use `claude-opus-4 (anthropic)` with no hint that the name had never been
/// checked. Silence is the defect: it reads as verification.
///
/// `configured` is how many providers the config named, `readable` how many
/// catalogues came back.
pub fn verification_banner(configured: usize, readable: usize) -> Option<String> {
    match (configured, readable) {
        (0, _) => Some(
            "> **Unverified.** No provider could be reached from this project, so none of the \
             models below were checked against a real catalogue. Point an agent at a provider \
             (`[agents.<role>] provider = \"...\"` in niki.toml, or `<PROVIDER>_API_KEY` in the \
             environment) and re-run this — until then the table is a static opinion written when \
             the models in it were current."
                .to_string(),
        ),
        (c, 0) => Some(format!(
            "> **Unverified.** {c} provider{} configured, but no catalogue could be read from \
             any of them (no API key, or no `/models` endpoint). The models below were **not** \
             checked; treat them as a static opinion, not as advice for this account.",
            if c == 1 { " is" } else { "s are" }
        )),
        (c, r) if r < c => Some(format!(
            "> **Partly unverified.** Catalogues were read for {r} of {c} reachable providers. \
             Roles recommending a model on an unreadable provider were not checked."
        )),
        _ => None,
    }
}

/// Everything the report prints, as one string.
///
/// Split out from `handle` so the wording is testable without a network, a
/// config, or a TTY. The part of this command that quietly rots is exactly
/// the text — the banner that is missing when it should be there, the warning
/// that fires for the wrong provider — and none of it was reachable from a
/// test that only ever ran the binary end to end.
#[allow(clippy::too_many_arguments)]
pub fn render_report(
    filtered: &[RoleRec],
    pref: &str,
    est_in: u32,
    est_out: u32,
    catalogues: &Catalogues,
    configured: usize,
    tasks_dir: &std::path::Path,
    role_filter: Option<&str>,
) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();

    let _ = writeln!(out, "# NIKI Model Recommendations\n");
    let _ = writeln!(
        out,
        "Preference: `{pref}` · est. tokens/run: {est_in} in / {est_out} out\n"
    );
    if let Some(banner) = verification_banner(configured, catalogues.len()) {
        let _ = writeln!(out, "{banner}\n");
    }

    for rec in filtered {
        let chosen = match pref {
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

        let _ = writeln!(out, "## {}  (`{}`)", role_name(rec.role), chosen.0);
        let _ = writeln!(
            out,
            "  - Recommended now: **{}** (`{}`)",
            chosen.1, chosen.0
        );
        let _ = writeln!(
            out,
            "  - Strong: {}  ·  Cheap: {}",
            fmt(rec.strong),
            fmt(rec.cheap)
        );
        let _ = writeln!(out, "  - Why: {}", rec.rationale);

        // The check that matters: is this model actually reachable?
        let catalogue = catalogues.get(chosen.0).map(|v| &v[..]);
        match crate::recommend::availability(catalogue, chosen.1) {
            crate::recommend::Availability::Offered => {}
            crate::recommend::Availability::NotOffered => {
                for line in not_offered_lines(chosen.0, chosen.1, catalogue) {
                    let _ = writeln!(out, "  - {line}");
                }
            }
            // `Unknown` says nothing on its own — a provider with no
            // `/models` endpoint is normal. The banner above is what tells
            // the user this whole report went unchecked.
            crate::recommend::Availability::Unknown => {}
        }

        let cost = estimate_cost(chosen.0, chosen.1, est_in, est_out);
        match lookup_price(chosen.0, chosen.1) {
            Some(_) => {
                let _ = writeln!(out, "  - Est. cost/run: ${cost:.4}");
            }
            None => {
                let _ = writeln!(out, "  - Est. cost/run: $0.0000 (local / unknown model)");
            }
        }
        let _ = writeln!(out);
    }

    // Observed spend from this project's own history. Static heuristics above
    // are generic; these numbers are what past runs actually cost here.
    let observed: Vec<_> = observed_spend(tasks_dir)
        .into_iter()
        .filter(|o| match role_filter {
            Some(r) => role_name(o.role) == r.to_lowercase(),
            None => true,
        })
        .collect();
    let _ = writeln!(out, "## Observed in past runs  (`{}`)", tasks_dir.display());
    if observed.is_empty() {
        let _ = writeln!(
            out,
            "  - No past runs found — figures above are static heuristics."
        );
    } else {
        let total_runs: usize = observed.iter().map(|o| o.runs).sum();
        let _ = writeln!(
            out,
            "  - Aggregated from {total_runs} stage executions in past runs."
        );
        for o in observed {
            let unpriced_note =
                if o.avg_cost_usd == 0.0 && crate::cost::is_unpriced(&o.provider, &o.model) {
                    " — unpriced model, spend unmeasured"
                } else {
                    ""
                };
            let _ = writeln!(
                out,
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
    let _ = writeln!(out);

    let _ = writeln!(
        out,
        "Tip: set these via `[agents]` in niki.toml, or override per run with `--coder-model`, etc."
    );
    out
}

pub async fn handle(args: &RecommendArgs) -> Result<()> {
    let recs = recommendations();
    let project = std::path::Path::new(&args.project);
    // Read each provider's real catalogue once, so the advice below is checked
    // rather than asserted. Best effort: a provider with no `/models`
    // endpoint, or a key that cannot read one, leaves `Unknown`, which is
    // printed as silence rather than as a claim.
    // Fetching a catalogue is async, and this is the shape the other commands
    // that need the network already use (`eval`, `plan`, `goal`). A nested
    // `block_on` from inside `main`'s runtime panics — including via
    // `Handle::block_on` — so the command is async rather than working around
    // it.
    //
    // The project argument is what config is read from. It used to be `.`,
    // so `--project ~/elsewhere` checked ~/elsewhere's run history against
    // the current directory's providers: the history and the catalogue came
    // from two different repositories and neither line said so.
    let (catalogues, configured) = load_catalogues(project).await;

    let pref = args.preference.to_lowercase();
    let filtered: Vec<RoleRec> = match &args.role {
        Some(r) => recs
            .iter()
            .filter(|x| role_name(x.role) == r.to_lowercase())
            .cloned()
            .collect(),
        None => recs,
    };
    if filtered.is_empty() {
        eprintln!(
            "No recommendation for role '{}'. Valid: planner, coder, tester, reviewer, synthesizer, security_auditor.",
            args.role.as_deref().unwrap_or("")
        );
        std::process::exit(2);
    }

    let (est_in, est_out) = estimate_tokens(args.task.as_deref());
    let tasks_dir = project.join(".niki/tasks");
    print!(
        "{}",
        render_report(
            &filtered,
            &pref,
            est_in,
            est_out,
            &catalogues,
            configured,
            &tasks_dir,
            args.role.as_deref(),
        )
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> crate::cli::catalogue::CatalogueEntry {
        crate::cli::catalogue::CatalogueEntry {
            id: id.to_string(),
            price_per_mtok: None,
            traits: Vec::new(),
        }
    }

    fn one_role() -> Vec<RoleRec> {
        let recs = recommendations();
        vec![
            recs.into_iter()
                .find(|r| r.role == AgentRole::Coder)
                .unwrap(),
        ]
    }

    fn render(catalogues: &Catalogues, configured: usize) -> String {
        let recs = one_role();
        render_report(
            &recs,
            "balanced",
            1000,
            500,
            catalogues,
            configured,
            std::path::Path::new("/nonexistent/.niki/tasks"),
            None,
        )
    }

    /// The failure this fixes is a user being told, in the same confident
    /// voice as a verified recommendation, to use a model nobody checked.
    /// Before the banner there was no output difference at all between "we
    /// read the catalogue and the model is there" and "we never read
    /// anything" — so the report's formatting was asserting a verification
    /// that had not happened.
    #[test]
    fn a_report_that_checked_nothing_says_so() {
        let out = render(&Catalogues::new(), 0);
        assert!(
            out.contains("Unverified"),
            "with no provider configured the report must admit it is unchecked:\n{out}"
        );
    }

    /// The same silence, one layer down: a provider *is* configured, the
    /// network or the key failed, and the report is still unchecked. Both
    /// cases used to produce byte-identical output.
    #[test]
    fn a_configured_provider_whose_catalogue_could_not_be_read_says_so() {
        let out = render(&Catalogues::new(), 1);
        assert!(
            out.contains("Unverified") && out.contains("1 provider is configured"),
            "a configured-but-unreadable provider must be named, not silently dropped:\n{out}"
        );
    }

    /// The banner has an inverse. Once a catalogue really is read, warning
    /// the user their advice is unchecked trains them to ignore the warning,
    /// which is worse than never printing it.
    #[test]
    fn a_fully_verified_report_carries_no_banner() {
        let mut c = Catalogues::new();
        c.insert("anthropic".to_string(), vec![entry("claude-sonnet-4")]);
        let out = render(&c, 1);
        assert!(
            !out.contains("Unverified"),
            "a verified report must not cry wolf:\n{out}"
        );
    }

    /// Partial verification is the case that used to read worst: some
    /// providers checked, the one this role recommends on not checked, and
    /// the per-role line said nothing because `Unknown` is not a claim.
    #[test]
    fn a_half_read_fleet_says_it_is_half_read() {
        let mut c = Catalogues::new();
        c.insert("openai".to_string(), vec![entry("gpt-4o-mini")]);
        let out = render(&c, 2);
        assert!(
            out.contains("Partly unverified") && out.contains("1 of 2"),
            "1 of 2 readable must be stated, since the Coder is on the other one:\n{out}"
        );
    }

    /// A model the provider demonstrably does not carry still gets its own,
    /// more specific warning. The banner is about the fleet; this is about
    /// one name, and the banner must not have replaced it.
    #[test]
    fn a_verified_fleet_still_names_the_model_it_lacks() {
        let mut c = Catalogues::new();
        c.insert("anthropic".to_string(), vec![entry("claude-haiku-4-5")]);
        let out = render(&c, 1);
        assert!(
            out.contains("Not offered by"),
            "a readable catalogue that lacks the model must still say so:\n{out}"
        );
        assert!(
            !out.contains("Unverified"),
            "and it is verified, so no fleet banner:\n{out}"
        );
    }

    /// The gap that made the banner above necessary in the first place: the
    /// documented way to give NIKI a key is an environment variable, which
    /// writes no `[providers]` entry. Probing the config table alone reported
    /// that account as having no provider, so its report was permanently,
    /// wrongly labelled unverified — and the user had no way to fix it.
    #[test]
    fn a_provider_reachable_only_through_an_agent_still_counts() {
        let mut config = crate::config::NikiConfig::default();
        config.providers.clear();
        config.agents.coder.provider = "openrouter".into();

        let targets = probe_targets(&config);
        assert!(
            targets.contains(&"openrouter".to_string()),
            "an agent pointed at openrouter is a provider this account can reach: {targets:?}"
        );
    }

    /// A fallback is part of the chain a run will actually take, so a
    /// recommendation has to survive it too.
    #[test]
    fn fallback_providers_are_probed_too() {
        let mut config = crate::config::NikiConfig::default();
        config.providers.clear();
        config.agents.coder.fallbacks = vec!["groq".into()];
        let targets = probe_targets(&config);
        assert!(
            targets.contains(&"groq".to_string()),
            "a fallback is a provider the run can land on: {targets:?}"
        );
    }

    /// The count has to mean something.
    ///
    /// `NikiConfig` seeds a slot for all twelve known providers, so counting
    /// `providers` entries makes every project look like it has twelve
    /// reachable providers. `recommend` would then fire twelve catalogue
    /// requests at an account that has none, and — because ten of the twelve
    /// have no key — report its own advice as "Partly unverified" for every
    /// user, permanently, on a command whose job is to be believed.
    ///
    /// This is the regression this check was added for: it was written into
    /// `probe_targets` and the bug shipped in the same commit.
    #[test]
    fn a_fresh_install_probes_nothing_rather_than_twelve_providers() {
        let mut config = crate::config::NikiConfig::default();
        config.providers.clear();
        for name in crate::config::types::AgentsConfig::NAMES {
            config.agents.agent_named_mut(name).unwrap().provider = String::new();
        }
        let targets = probe_targets(&config);
        assert!(
            targets.is_empty(),
            "an agent pointed at nothing cannot reach anything, but the probe \
             list was {targets:?}"
        );
    }

    /// And the sealed config is the shape a real fresh install has. If this
    /// fails, the map is no longer pre-seeded and the guards above are
    /// defending a condition that no longer exists — which is worth knowing,
    /// because they would then be silently dead.
    #[test]
    fn the_sealed_config_does_not_make_every_provider_look_reachable() {
        let mut config = crate::config::NikiConfig::default();
        config.apply_env_lookup(&|_| None);
        let targets = probe_targets(&config);
        assert!(
            targets.len() < 3,
            "expected only the default agent providers, got {}: {targets:?}",
            targets.len()
        );
    }

    /// Duplicates are common — every role defaults to the same provider — and
    /// fetching the same catalogue twice would report "1 of 3 readable" for a
    /// fleet of one and read as partial verification.
    #[test]
    fn a_provider_named_by_several_roles_is_probed_once() {
        let mut config = crate::config::NikiConfig::default();
        config.providers.clear();
        for name in crate::config::types::AgentsConfig::NAMES {
            config.agents.agent_named_mut(name).unwrap().provider = "anthropic".into();
        }
        let targets = probe_targets(&config);
        assert_eq!(
            targets.iter().filter(|t| *t == "anthropic").count(),
            1,
            "one provider, one probe: {targets:?}"
        );
    }
}
