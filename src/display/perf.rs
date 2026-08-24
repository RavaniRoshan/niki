//! Optional frame-timing instrumentation for the chat TUI, enabled by setting
//! `NIKI_PERF=1`.
//!
//! Measures the two numbers that define perceived snappiness (see
//! research/ultimate-test-suite-niki.md Phase 3):
//!   - **frame gap** — time between consecutive paints (jank detector; p95/p99
//!     are what users feel)
//!   - **input→paint** — keystroke to flushed frame (Nielsen's 100 ms budget;
//!     Claude Code targets ~16 ms frames)
//!
//! On exit the recorder prints percentiles to stderr. Overhead when disabled:
//! one atomic-free struct field check per event/frame.

use std::time::Instant;

#[derive(Default)]
pub struct PerfRecorder {
    enabled: bool,
    input_at: Option<Instant>,
    last_frame: Option<Instant>,
    frame_gaps_ms: Vec<f64>,
    input_paint_ms: Vec<f64>,
}

impl PerfRecorder {
    pub fn from_env() -> Self {
        let enabled = std::env::var("NIKI_PERF")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        Self {
            enabled,
            ..Self::default()
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Call when a key/mouse event is received.
    pub fn note_input(&mut self) {
        if self.enabled {
            self.input_at = Some(Instant::now());
        }
    }

    /// Call immediately after a frame is drawn and flushed.
    pub fn note_frame(&mut self) {
        if !self.enabled {
            return;
        }
        let now = Instant::now();
        if let Some(last) = self.last_frame.replace(now) {
            self.frame_gaps_ms.push(ms_since(last, now));
        }
        if let Some(input) = self.input_at.take() {
            self.input_paint_ms.push(ms_since(input, now));
        }
    }

    /// Human-readable percentile summary for stderr on shutdown.
    pub fn report(&self) -> String {
        format!(
            "[perf] frames={} frame-gap ms p50={:.1} p95={:.1} p99={:.1} | \
             inputs={} input->paint ms p50={:.1} p95={:.1} p99={:.1}",
            self.frame_gaps_ms.len(),
            pct(&mut self.frame_gaps_ms.clone(), 50.0),
            pct(&mut self.frame_gaps_ms.clone(), 95.0),
            pct(&mut self.frame_gaps_ms.clone(), 99.0),
            self.input_paint_ms.len(),
            pct(&mut self.input_paint_ms.clone(), 50.0),
            pct(&mut self.input_paint_ms.clone(), 95.0),
            pct(&mut self.input_paint_ms.clone(), 99.0),
        )
    }
}

fn ms_since(from: Instant, to: Instant) -> f64 {
    to.duration_since(from).as_secs_f64() * 1000.0
}

fn pct(samples: &mut [f64], p: f64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((p / 100.0) * (samples.len() as f64 - 1.0)).round() as usize;
    samples[idx.min(samples.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_recorder_is_zero_cost_noop() {
        let mut r = PerfRecorder::default();
        assert!(!r.enabled());
        r.note_input();
        r.note_frame();
        r.note_frame();
        assert!(r.report().contains("frames=0"));
        assert!(r.report().contains("inputs=0"));
    }

    #[test]
    fn percentiles_are_rank_based() {
        // Nearest-rank over the rounded index of p*(n-1).
        let mut s = vec![5.0, 1.0, 9.0, 3.0];
        assert_eq!(pct(&mut s, 50.0), 5.0); // round(0.5*3)=2 -> sorted[2]
        assert_eq!(pct(&mut s, 100.0), 9.0);
        assert_eq!(pct(&mut [], 50.0), 0.0);
    }
}
