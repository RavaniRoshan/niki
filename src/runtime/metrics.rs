//! Precise runtime metrics separating model latency from NIKI runtime latency.

use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Runtime metrics capturing detailed latency and token breakdowns.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RuntimeMetrics {
    // Latencies (in milliseconds)
    pub startup_ms: u64,
    pub prewarm_ms: u64,
    pub session_init_ms: u64,
    pub context_build_ms: u64,
    pub request_build_ms: u64,
    pub ttft_ms: u64,
    pub model_generation_ms: u64,
    pub tool_wait_ms: u64,
    pub tool_execution_ms: u64,
    pub compaction_ms: u64,
    pub checkpoint_ms: u64,
    pub turn_total_ms: u64,
    pub session_total_ms: u64,

    // Token and context counts
    pub context_tokens: usize,
    pub cacheable_context_tokens: usize,
    pub dynamic_context_tokens: usize,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cached_input_tokens: u32,
    pub reasoning_tokens: u32,

    // Counters
    pub compaction_count: usize,
    pub tool_call_count: usize,
    pub model_request_count: usize,
    pub retry_count: u32,
}

impl RuntimeMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Merge metrics from a step or turn into an aggregate accumulator.
    pub fn merge(&mut self, other: &RuntimeMetrics) {
        self.startup_ms = self.startup_ms.max(other.startup_ms);
        self.prewarm_ms = self.prewarm_ms.max(other.prewarm_ms);
        self.session_init_ms = self.session_init_ms.max(other.session_init_ms);
        self.context_build_ms = self.context_build_ms.saturating_add(other.context_build_ms);
        self.request_build_ms = self.request_build_ms.saturating_add(other.request_build_ms);
        self.ttft_ms = self.ttft_ms.max(other.ttft_ms);
        self.model_generation_ms = self
            .model_generation_ms
            .saturating_add(other.model_generation_ms);
        self.tool_wait_ms = self.tool_wait_ms.saturating_add(other.tool_wait_ms);
        self.tool_execution_ms = self
            .tool_execution_ms
            .saturating_add(other.tool_execution_ms);
        self.compaction_ms = self.compaction_ms.saturating_add(other.compaction_ms);
        self.checkpoint_ms = self.checkpoint_ms.saturating_add(other.checkpoint_ms);
        self.turn_total_ms = self.turn_total_ms.saturating_add(other.turn_total_ms);
        self.session_total_ms = self.session_total_ms.max(other.session_total_ms);

        self.context_tokens = self.context_tokens.max(other.context_tokens);
        self.cacheable_context_tokens = self
            .cacheable_context_tokens
            .max(other.cacheable_context_tokens);
        self.dynamic_context_tokens = self
            .dynamic_context_tokens
            .max(other.dynamic_context_tokens);

        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
        self.cached_input_tokens = self
            .cached_input_tokens
            .saturating_add(other.cached_input_tokens);
        self.reasoning_tokens = self.reasoning_tokens.saturating_add(other.reasoning_tokens);

        self.compaction_count += other.compaction_count;
        self.tool_call_count += other.tool_call_count;
        self.model_request_count += other.model_request_count;
        self.retry_count = self.retry_count.saturating_add(other.retry_count);
    }
}

/// Helper timer for measuring latency of a runtime block.
pub struct Timer {
    start: Instant,
}

impl Timer {
    pub fn start() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_merge() {
        let mut m1 = RuntimeMetrics::default();
        m1.context_build_ms = 50;
        m1.input_tokens = 100;
        m1.tool_call_count = 2;

        let mut m2 = RuntimeMetrics::default();
        m2.context_build_ms = 30;
        m2.input_tokens = 150;
        m2.tool_call_count = 1;

        m1.merge(&m2);
        assert_eq!(m1.context_build_ms, 80);
        assert_eq!(m1.input_tokens, 250);
        assert_eq!(m1.tool_call_count, 3);
    }
}
