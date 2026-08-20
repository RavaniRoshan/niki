//! High-performance rendering engine shell.
//!
//! Owns the `ratatui` terminal and the dirty-flag / frame-rate policy for the
//! TUI. Differential screen updates and cell diffing are handled internally by
//! `ratatui`'s `Terminal::draw` (it keeps its own front/back `Buffer` and only
//! emits changed cells), so we deliberately do NOT maintain a parallel
//! `CellBuffer` here — that was dead weight that duplicated ratatui.
//!
//! - CSI 2026 synchronized output (DEC 2026) is wrapped around each frame by
//!   the caller (`tui.rs`) for flicker-free updates.
//! - Targets 60fps during streaming (`High`), 30fps idle (`Low`).

use std::io::{self};
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

/// Rolling frame-time statistics for performance monitoring.
///
/// Tracks the last 120 frame durations (≈2s at 60fps) in a ring buffer.
/// Exposes min/max/p95/mean without allocations on the hot path.
#[derive(Debug)]
pub struct FrameStats {
    samples: [Duration; 120],
    write: usize,
    count: usize,
}

impl FrameStats {
    fn new() -> Self {
        Self {
            samples: [Duration::ZERO; 120],
            write: 0,
            count: 0,
        }
    }

    fn record(&mut self, d: Duration) {
        self.samples[self.write] = d;
        self.write = (self.write + 1) % 120;
        if self.count < 120 {
            self.count += 1;
        }
    }

    /// Mean frame time over the window.
    pub fn mean(&self) -> Duration {
        if self.count == 0 {
            return Duration::ZERO;
        }
        let total: Duration = self.samples[..self.count].iter().sum();
        total / self.count as u32
    }

    /// Minimum (fastest) frame time.
    pub fn min(&self) -> Duration {
        if self.count == 0 {
            return Duration::ZERO;
        }
        self.samples[..self.count]
            .iter()
            .min()
            .copied()
            .unwrap_or(Duration::ZERO)
    }

    /// Maximum (slowest) frame time.
    pub fn max(&self) -> Duration {
        if self.count == 0 {
            return Duration::ZERO;
        }
        self.samples[..self.count]
            .iter()
            .max()
            .copied()
            .unwrap_or(Duration::ZERO)
    }

    /// 95th percentile frame time.
    pub fn p95(&self) -> Duration {
        if self.count == 0 {
            return Duration::ZERO;
        }
        let mut sorted = self.samples[..self.count].to_vec();
        sorted.sort();
        let idx = (sorted.len() as f64 * 0.95) as usize;
        sorted[idx.min(sorted.len() - 1)]
    }

    /// Number of samples collected so far (up to 120).
    pub fn len(&self) -> usize {
        self.count
    }

    /// Whether no samples have been collected yet.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

/// Render frame rate target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameTarget {
    /// 16ms interval for streaming — 60fps.
    High,
    /// 33ms interval for idle — 30fps.
    Low,
}

/// Thin shell around the `ratatui` terminal that owns the frame-rate policy and
/// dirty flag. All actual pixel work happens in `ratatui::Terminal::draw`.
pub struct RenderEngine {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    target: FrameTarget,
    dirty: bool,
    frame_start: Option<Instant>,
    stats: FrameStats,
}

impl RenderEngine {
    /// Create a new render engine, taking ownership of the terminal.
    pub fn new(terminal: Terminal<CrosstermBackend<io::Stdout>>, _synchronized: bool) -> Self {
        Self {
            terminal,
            target: FrameTarget::Low,
            dirty: true,
            frame_start: None,
            stats: FrameStats::new(),
        }
    }

    /// Whether the engine needs to redraw.
    pub fn needs_render(&self) -> bool {
        self.dirty
    }

    /// Mark the engine as needing a redraw.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Mark the engine as up to date after a successful render.
    pub fn mark_clean_for_render(&mut self) {
        self.dirty = false;
    }

    /// Set the frame target (high for streaming, low for idle).
    pub fn set_target(&mut self, target: FrameTarget) {
        self.target = target;
    }

    /// Get the frame interval based on current target.
    pub fn frame_interval_ms(&self) -> u64 {
        match self.target {
            FrameTarget::High => 16,
            FrameTarget::Low => 33,
        }
    }

    /// Get a reference to the terminal (for size queries etc.).
    pub fn terminal(&self) -> &Terminal<CrosstermBackend<io::Stdout>> {
        &self.terminal
    }

    /// Get a mutable reference to the terminal (the caller does the real draw).
    pub fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<io::Stdout>> {
        &mut self.terminal
    }

    /// Mark the start of a frame (call before draw).
    pub fn begin_frame(&mut self) {
        self.frame_start = Some(Instant::now());
    }

    /// Mark the end of a frame (call after draw). Records timing stats.
    pub fn end_frame(&mut self) {
        if let Some(start) = self.frame_start.take() {
            self.stats.record(start.elapsed());
        }
    }

    /// Get a reference to the rolling frame stats.
    pub fn stats(&self) -> &FrameStats {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_target_intervals() {
        assert_eq!(FrameTarget::High as u8, 0);
        assert_eq!(FrameTarget::Low as u8, 1);
    }

    #[test]
    fn engine_dirty_flag_lifecycle() {
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend).expect("terminal");
        let mut engine = RenderEngine::new(terminal, false);
        assert!(engine.needs_render());
        engine.mark_clean_for_render();
        assert!(!engine.needs_render());
        engine.mark_dirty();
        assert!(engine.needs_render());
    }

    #[test]
    fn engine_frame_interval_matches_target() {
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend).expect("terminal");
        let mut engine = RenderEngine::new(terminal, false);
        assert_eq!(engine.frame_interval_ms(), 33);
        engine.set_target(FrameTarget::High);
        assert_eq!(engine.frame_interval_ms(), 16);
    }

    #[test]
    fn frame_stats_basics() {
        let mut stats = FrameStats::new();
        assert_eq!(stats.len(), 0);
        assert_eq!(stats.mean(), Duration::ZERO);
        stats.record(Duration::from_millis(5));
        stats.record(Duration::from_millis(15));
        stats.record(Duration::from_millis(10));
        assert_eq!(stats.len(), 3);
        assert_eq!(stats.min(), Duration::from_millis(5));
        assert_eq!(stats.max(), Duration::from_millis(15));
        assert_eq!(stats.mean(), Duration::from_millis(10));
    }

    #[test]
    fn frame_stats_p95() {
        let mut stats = FrameStats::new();
        for i in 0..100 {
            stats.record(Duration::from_millis(i));
        }
        // Sorted: 0..99, p95 index = 95, value = 95ms
        assert_eq!(stats.p95(), Duration::from_millis(95));
    }

    #[test]
    fn engine_begin_end_frame_records_stats() {
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend).expect("terminal");
        let mut engine = RenderEngine::new(terminal, false);
        assert_eq!(engine.stats().len(), 0);
        engine.begin_frame();
        // Simulate some work
        std::thread::sleep(Duration::from_millis(1));
        engine.end_frame();
        assert_eq!(engine.stats().len(), 1);
        assert!(engine.stats().mean() >= Duration::from_millis(1));
    }
}
