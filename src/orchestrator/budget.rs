//! Unified run hysteresis (Phase 5.5).
//!
//! One step/cost/wallclock budget for the whole run. Every retry, repair,
//! revision, tool-loop step, and goal iteration accrues against it;
//! exhaustion yields a typed [`crate::NikiError::BudgetExhausted`] recorded
//! in `task.json`, never a silent stop.
//!
//! Resolution: `[budget]` table, CLI overrides land there (see `cli/run.rs`).
//! `max_usd` falls back to `[general] spend_cap_usd` when unset, so there is
//! one effective money ceiling. `0` disables a dimension (unlimited).
//! The legacy `spend_cap_usd` hard ceiling at stage boundaries is kept as-is;
//! this budget is the unified accrual around it.
//!
//! Approximation, documented: provider-internal failover attempts are
//! cost-bounded (their recorded usage accrues) and step-bounded by the stage
//! they belong to, but are not individually counted — the provider trait has
//! no budget channel. Wallclock is checked at stage boundaries, not
//! mid-LLM-call.

use crate::config::NikiConfig;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Run-scoped hysteresis budget.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunBudget {
    pub max_steps: u32,
    pub max_usd: f64,
    pub max_wallclock_secs: u64,
    #[serde(default)]
    pub steps_used: u32,
    #[serde(default)]
    pub cost_used: f64,
    #[serde(default)]
    pub accounted_metrics: usize,
    #[serde(skip, default = "now_instant")]
    pub started: Instant,
    #[serde(default = "now_string")]
    pub started_at: String,
}

fn now_instant() -> Instant {
    Instant::now()
}

fn now_string() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

impl Default for RunBudget {
    fn default() -> Self {
        Self::new(0, 0.0, 0)
    }
}

impl RunBudget {
    pub fn new(max_steps: u32, max_usd: f64, max_wallclock_secs: u64) -> Self {
        Self {
            max_steps,
            max_usd,
            max_wallclock_secs,
            steps_used: 0,
            cost_used: 0.0,
            accounted_metrics: 0,
            started: Instant::now(),
            started_at: now_string(),
        }
    }

    /// Resolve from config: explicit `[budget]` wins; `max_usd` falls back to
    /// `[general] spend_cap_usd` so the money ceiling has one effective value.
    pub fn resolve(config: &NikiConfig) -> Self {
        let max_usd = if config.budget.max_usd > 0.0 {
            config.budget.max_usd
        } else {
            config.general.spend_cap_usd
        };
        Self::new(
            config.budget.max_steps,
            max_usd,
            config.budget.max_wallclock_secs,
        )
    }

    pub fn elapsed_secs(&self) -> u64 {
        self.started.elapsed().as_secs()
    }

    /// Accrue one step plus its cost.
    pub fn accrue(&mut self, steps: u32, cost: f64) {
        self.steps_used = self.steps_used.saturating_add(steps);
        self.cost_used += cost.max(0.0);
    }

    /// Accrue every not-yet-accounted stage metric: one step per stage plus
    /// its recorded transient retries, plus the stage cost. Idempotent per
    /// metric (tracks `accounted_metrics`), so calling after every stage
    /// boundary never double-counts.
    pub fn accrue_new_stages(&mut self, metrics: &[crate::orchestrator::state::StageMetric]) {
        for m in metrics.iter().skip(self.accounted_metrics) {
            self.accrue(1 + m.retry_count, m.cost_usd);
        }
        self.accounted_metrics = metrics.len();
    }

    /// Mark the first `n` metrics accounted without accruing them. Used for
    /// metrics whose cost/steps already accrued through a finer channel
    /// (e.g. the tool loop accrues per iteration, so its summary metric
    /// must not accrue again when it lands in the metrics vec).
    pub fn account_metrics_up_to(&mut self, n: usize) {
        self.accounted_metrics = self.accounted_metrics.max(n);
    }

    /// Check all three dimensions. First exhausted dimension wins, named in
    /// the typed error.
    pub fn check(&self) -> anyhow::Result<()> {
        if self.max_steps > 0 && self.steps_used >= self.max_steps {
            return Err(crate::NikiError::BudgetExhausted {
                dimension: "steps".to_string(),
                detail: format!(
                    "used {} of {} allowed steps",
                    self.steps_used, self.max_steps
                ),
            }
            .into());
        }
        if self.max_usd > 0.0 && self.cost_used >= self.max_usd {
            return Err(crate::NikiError::BudgetExhausted {
                dimension: "cost".to_string(),
                detail: format!(
                    "used ${:.4} of ${:.2} allowed",
                    self.cost_used, self.max_usd
                ),
            }
            .into());
        }
        if self.max_wallclock_secs > 0 && self.elapsed_secs() >= self.max_wallclock_secs {
            return Err(crate::NikiError::BudgetExhausted {
                dimension: "wallclock".to_string(),
                detail: format!(
                    "elapsed {}s of {}s allowed",
                    self.elapsed_secs(),
                    self.max_wallclock_secs
                ),
            }
            .into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_with(max_steps: u32, max_usd: f64, max_wallclock: u64) -> NikiConfig {
        let mut c = NikiConfig::default();
        c.budget.max_steps = max_steps;
        c.budget.max_usd = max_usd;
        c.budget.max_wallclock_secs = max_wallclock;
        c
    }

    #[test]
    fn steps_exhaust_regardless_of_mechanism() {
        // Phase 5.5 acceptance shape: a tiny budget stops whatever would
        // otherwise retry — the check, not the mechanism, decides.
        let mut b = RunBudget::resolve(&cfg_with(2, 0.0, 0));
        b.accrue(1, 0.0);
        assert!(b.check().is_ok());
        b.accrue(1, 0.0);
        let err = b.check().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("budget exhausted"), "{msg}");
        assert!(msg.contains("steps"), "{msg}");
    }

    #[test]
    fn cost_dimension_names_itself() {
        let mut b = RunBudget::resolve(&cfg_with(0, 1.0, 0));
        b.accrue(0, 1.5);
        let msg = b.check().unwrap_err().to_string();
        assert!(msg.contains("cost"), "{msg}");
    }

    #[test]
    fn money_falls_back_to_spend_cap() {
        let mut c = NikiConfig::default();
        c.general.spend_cap_usd = 5.0;
        let b = RunBudget::resolve(&c);
        assert_eq!(
            b.max_usd, 5.0,
            "unset budget.max_usd inherits spend_cap_usd"
        );
        let mut c2 = NikiConfig::default();
        c2.general.spend_cap_usd = 5.0;
        c2.budget.max_usd = 2.0;
        let b2 = RunBudget::resolve(&c2);
        assert_eq!(b2.max_usd, 2.0, "explicit budget.max_usd wins");
    }

    #[test]
    fn unlimited_by_default_preserves_behavior() {
        let b = RunBudget::resolve(&NikiConfig::default());
        assert_eq!((b.max_steps, b.max_wallclock_secs), (0, 0));
        assert!(b.check().is_ok());
    }

    #[test]
    fn accrue_new_stages_counts_retries_once() {
        use crate::artifacts::types::AgentRole;
        let mut b = RunBudget::new(100, 0.0, 0);
        let m = || crate::orchestrator::state::StageMetric {
            role: AgentRole::Coder,
            provider: "p".into(),
            model: "m".into(),
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: 0,
            reasoning_tokens: 0,
            latency_ms: 0,
            cost_usd: 0.25,
            retry_count: 2,
            ttft_ms: 0,
        };
        b.accrue_new_stages(&[m()]);
        assert_eq!(b.steps_used, 3, "1 stage + 2 retries");
        b.accrue_new_stages(&[m(), m()]);
        assert_eq!(b.steps_used, 6, "only the new metric accrues");
        assert!((b.cost_used - 0.5).abs() < 1e-9);
    }
}
