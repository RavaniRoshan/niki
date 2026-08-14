use crate::config::ProviderConfig;
use crate::llm::provider::{
    CompletionRequest, CompletionResponse, LlmProvider, StreamChunk, TokenUsage, create_provider,
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use futures::Stream;
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Circuit breaker state per provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation — requests pass through.
    Closed,
    /// Too many failures — reject requests immediately.
    Open,
    /// Testing recovery — allow one probe request through.
    HalfOpen,
}

/// Sliding-window circuit breaker for a single provider.
///
/// Tracks failures over a rolling window. After `threshold` failures within
/// `window`, the breaker opens for `reset_timeout`. Then it transitions to
/// half-open and allows one probe request. Success closes the breaker; failure
/// reopens it.
#[derive(Debug)]
pub struct CircuitBreaker {
    state: CircuitState,
    /// Timestamps of recent failures within the sliding window.
    failures: VecDeque<Instant>,
    /// When the breaker opened (used to calculate reset timeout).
    opened_at: Option<Instant>,
    /// Number of failures within the window to trip the breaker.
    threshold: u32,
    /// Sliding window duration.
    window: Duration,
    /// How long to stay open before transitioning to half-open.
    reset_timeout: Duration,
}

impl CircuitBreaker {
    pub fn new(threshold: u32, window: Duration, reset_timeout: Duration) -> Self {
        Self {
            state: CircuitState::Closed,
            failures: VecDeque::new(),
            opened_at: None,
            threshold,
            window,
            reset_timeout,
        }
    }

    /// Record a failure. May trip the breaker.
    pub fn record_failure(&mut self) {
        let now = Instant::now();
        self.failures.push_back(now);
        // Prune old failures outside the window.
        while self
            .failures
            .front()
            .is_some_and(|&t| now.duration_since(t) > self.window)
        {
            self.failures.pop_front();
        }

        match self.state {
            CircuitState::Closed => {
                if self.failures.len() as u32 >= self.threshold {
                    tracing::warn!(
                        target: "niki::circuit_breaker",
                        threshold = self.threshold,
                        "Circuit breaker tripped — opening"
                    );
                    self.state = CircuitState::Open;
                    self.opened_at = Some(now);
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in half-open reopens the breaker.
                tracing::warn!(
                    target: "niki::circuit_breaker",
                    "Half-open probe failed — reopening"
                );
                self.state = CircuitState::Open;
                self.opened_at = Some(now);
            }
            CircuitState::Open => {}
        }
    }

    /// Record a success. May close the breaker.
    pub fn record_success(&mut self) {
        match self.state {
            CircuitState::HalfOpen => {
                tracing::info!(
                    target: "niki::circuit_breaker",
                    "Half-open probe succeeded — closing"
                );
                self.state = CircuitState::Closed;
                self.failures.clear();
                self.opened_at = None;
            }
            CircuitState::Closed => {
                // Prune old failures on success too.
                let now = Instant::now();
                while self
                    .failures
                    .front()
                    .is_some_and(|&t| now.duration_since(t) > self.window)
                {
                    self.failures.pop_front();
                }
            }
            CircuitState::Open => {}
        }
    }

    /// Check if the breaker allows a request.
    pub fn allows_request(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::HalfOpen => true, // Allow one probe.
            CircuitState::Open => {
                // Check if reset timeout has elapsed.
                if let Some(opened) = self.opened_at {
                    if opened.elapsed() >= self.reset_timeout {
                        tracing::info!(
                            target: "niki::circuit_breaker",
                            "Reset timeout elapsed — transitioning to half-open"
                        );
                        self.state = CircuitState::HalfOpen;
                        return true;
                    }
                }
                false
            }
        }
    }

    pub fn state(&self) -> CircuitState {
        self.state
    }
}

/// A provider entry in the failover chain: (name, provider, circuit breaker).
type ProviderEntry = (
    String,
    Arc<dyn LlmProvider>,
    Arc<tokio::sync::Mutex<CircuitBreaker>>,
);

/// A provider that tries the primary, then falls back to alternatives.
///
/// Each provider in the chain has its own circuit breaker. If the primary is
/// open, we skip straight to the next available provider.
pub struct FailoverProvider {
    chain: Vec<ProviderEntry>,
}

impl FailoverProvider {
    /// Build a failover chain from config.
    ///
    /// `primary_name` is the first provider to try. `fallback_names` are tried
    /// in order. All providers must be present in `provider_configs`.
    pub fn new(
        primary_name: &str,
        fallback_names: &[String],
        provider_configs: &std::collections::HashMap<String, ProviderConfig>,
    ) -> Result<Self> {
        let mut chain = Vec::new();

        // Collect all provider names in order: primary first, then fallbacks.
        let mut names = vec![primary_name.to_string()];
        names.extend(fallback_names.iter().cloned());
        // Deduplicate while preserving order.
        names.dedup();

        for name in &names {
            let cfg = provider_configs.get(name).ok_or_else(|| {
                anyhow!(
                    "Provider '{}' not configured (referenced in failover chain)",
                    name
                )
            })?;
            let provider = create_provider(name, cfg)?;
            let breaker = CircuitBreaker::new(3, Duration::from_secs(60), Duration::from_secs(60));
            chain.push((
                name.clone(),
                Arc::from(provider),
                Arc::new(tokio::sync::Mutex::new(breaker)),
            ));
        }

        Ok(Self { chain })
    }
}

#[async_trait]
impl LlmProvider for FailoverProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let mut last_err = None;

        for (name, provider, breaker) in &self.chain {
            {
                let mut b = breaker.lock().await;
                if !b.allows_request() {
                    continue;
                }
            }

            match provider.complete(request.clone()).await {
                Ok(response) => {
                    let mut b = breaker.lock().await;
                    b.record_success();
                    tracing::debug!(
                        target: "niki::failover",
                        provider = name.as_str(),
                        "Request succeeded"
                    );
                    return Ok(response);
                }
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    let is_transient = err_str.contains("timeout")
                        || err_str.contains("rate")
                        || err_str.contains("429")
                        || err_str.contains("503")
                        || err_str.contains("overloaded")
                        || err_str.contains("connection")
                        || err_str.contains("network");

                    {
                        let mut b = breaker.lock().await;
                        b.record_failure();
                    }

                    if is_transient {
                        tracing::warn!(
                            target: "niki::failover",
                            provider = name.as_str(),
                            error = %e,
                            "Transient error — trying next provider"
                        );
                        last_err = Some(e);
                        continue;
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow!("All providers in failover chain failed")))
    }

    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>> {
        // Stream failover: try each provider, return the first successful stream.
        // Once a stream is returned, failover for that request is done — the
        // caller handles stream-level errors.
        let mut last_err = None;

        for (name, provider, breaker) in &self.chain {
            {
                let mut b = breaker.lock().await;
                if !b.allows_request() {
                    continue;
                }
            }

            match provider.stream(request.clone()).await {
                Ok(stream) => {
                    let mut b = breaker.lock().await;
                    b.record_success();
                    tracing::debug!(
                        target: "niki::failover",
                        provider = name.as_str(),
                        "Stream started"
                    );
                    return Ok(stream);
                }
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    let is_transient = err_str.contains("timeout")
                        || err_str.contains("rate")
                        || err_str.contains("429")
                        || err_str.contains("503")
                        || err_str.contains("overloaded")
                        || err_str.contains("connection")
                        || err_str.contains("network");

                    {
                        let mut b = breaker.lock().await;
                        b.record_failure();
                    }

                    if is_transient {
                        tracing::warn!(
                            target: "niki::failover",
                            provider = name.as_str(),
                            error = %e,
                            "Transient stream error — trying next provider"
                        );
                        last_err = Some(e);
                        continue;
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow!("All providers in failover chain failed")))
    }

    fn provider_name(&self) -> &str {
        // Return the primary (first) provider name.
        self.chain
            .first()
            .map(|(name, _, _)| name.as_str())
            .unwrap_or("failover")
    }
}

/// Health check result for a single provider.
#[derive(Debug)]
pub struct HealthCheckResult {
    pub provider: String,
    pub ok: bool,
    pub latency_ms: u64,
    pub error: Option<String>,
}

/// Check the health of all configured providers by sending a minimal request.
pub async fn check_provider_health(
    provider_configs: &std::collections::HashMap<String, ProviderConfig>,
) -> Vec<HealthCheckResult> {
    let mut results = Vec::new();

    for (name, cfg) in provider_configs {
        // Skip mock providers.
        if name == "mock" {
            continue;
        }

        let start = Instant::now();
        let result = check_single_provider(name, cfg).await;
        let latency = start.elapsed().as_millis() as u64;

        match result {
            Ok(_usage) => {
                results.push(HealthCheckResult {
                    provider: name.clone(),
                    ok: true,
                    latency_ms: latency,
                    error: None,
                });
            }
            Err(e) => {
                results.push(HealthCheckResult {
                    provider: name.clone(),
                    ok: false,
                    latency_ms: latency,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    results
}

async fn check_single_provider(name: &str, cfg: &ProviderConfig) -> Result<TokenUsage> {
    let provider = create_provider(name, cfg)?;
    let request = CompletionRequest {
        model: cfg.default_model.clone(),
        system_prompt: "You are a health check. Reply with exactly: ok".to_string(),
        user_message: "Reply with exactly: ok".to_string(),
        max_tokens: 10,
        temperature: 0.0,
        json_schema: None,
    };

    let response = provider.complete(request).await?;
    Ok(response.usage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circuit_breaker_starts_closed() {
        let mut cb = CircuitBreaker::new(3, Duration::from_secs(60), Duration::from_secs(60));
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.allows_request());
    }

    #[test]
    fn circuit_breaker_trips_after_threshold() {
        let mut cb = CircuitBreaker::new(3, Duration::from_secs(60), Duration::from_secs(60));
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.allows_request());
    }

    #[test]
    fn circuit_breaker_half_open_after_timeout() {
        let mut cb = CircuitBreaker::new(2, Duration::from_secs(60), Duration::from_millis(100));
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);

        // Wait for reset timeout.
        std::thread::sleep(Duration::from_millis(150));
        assert!(cb.allows_request());
        assert_eq!(cb.state(), CircuitState::HalfOpen);
    }

    #[test]
    fn circuit_breaker_closes_on_success_from_half_open() {
        let mut cb = CircuitBreaker::new(2, Duration::from_secs(60), Duration::from_millis(100));
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);

        std::thread::sleep(Duration::from_millis(150));
        assert!(cb.allows_request());
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn circuit_breaker_reopens_on_failure_from_half_open() {
        let mut cb = CircuitBreaker::new(2, Duration::from_secs(60), Duration::from_millis(100));
        cb.record_failure();
        cb.record_failure();

        std::thread::sleep(Duration::from_millis(150));
        assert!(cb.allows_request());
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[test]
    fn circuit_breaker_prunes_old_failures() {
        let mut cb = CircuitBreaker::new(3, Duration::from_millis(100), Duration::from_secs(60));
        cb.record_failure();
        cb.record_failure();

        std::thread::sleep(Duration::from_millis(150));

        // Old failures are pruned, so one more should not trip.
        cb.record_failure();
        // The failures from 150ms ago are pruned; only the recent one counts.
        assert_eq!(cb.state(), CircuitState::Closed);
    }
}
