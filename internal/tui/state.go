package tui

import "time"

// State holds UI presentation state.
type State struct {
	Ready  bool
	Width  int
	Height int
	Busy   bool
	// Inline marks scrollback (non-altscreen) mode,
	// where finalized history belongs to the
	// terminal's native scrollback (U9).
	Inline bool
	// Debug toggles the telemetry overlay (B9).
	Debug bool
	// Activity is the single live activity line
	// above the composer, driven by real events
	// only (U3).
	Activity string
	// ReducedMotion disables non-essential animation
	// (U8).
	ReducedMotion bool

	// Environment & Session Metadata
	Directory      string
	SessionID      string
	ModelName      string
	Version        string
	PermissionMode string
	Mode           string
	GitBranch      string

	// Token Context Meter & Cost Accounting
	UsedTokens int
	MaxTokens  int
	TotalCost  float64
}

// FrameTelemetry records render-cost and event
// counters for the debug overlay (B9).
type FrameTelemetry struct {
	Frames        int
	Events        int
	LastRender    time.Duration
	RenderCosts   []time.Duration // rolling, capped
	BootPhases    []BootPhase
	Deltas        int
	maxRenderCost time.Duration
}

// BootPhase is one recorded boot phase timing.
type BootPhase struct {
	Name     string
	Duration time.Duration
}

const maxRenderCosts = 120

// RecordRender records one render cost.
func (f *FrameTelemetry) RecordRender(d time.Duration) {
	f.Frames++
	f.LastRender = d
	if d > f.maxRenderCost {
		f.maxRenderCost = d
	}
	f.RenderCosts = append(f.RenderCosts, d)
	if len(f.RenderCosts) > maxRenderCosts {
		f.RenderCosts = f.RenderCosts[len(f.RenderCosts)-maxRenderCosts:]
	}
}

// RenderP95 returns the 95th-percentile render
// cost over the recorded window.
func (f *FrameTelemetry) RenderP95() time.Duration {
	if len(f.RenderCosts) == 0 {
		return 0
	}
	sorted := make([]time.Duration, len(f.RenderCosts))
	copy(sorted, f.RenderCosts)
	for i := 1; i < len(sorted); i++ {
		for j := i; j > 0 && sorted[j] < sorted[j-1]; j-- {
			sorted[j], sorted[j-1] = sorted[j-1], sorted[j]
		}
	}
	idx := (len(sorted) * 95) / 100
	if idx >= len(sorted) {
		idx = len(sorted) - 1
	}
	return sorted[idx]
}

// MaxRenderCost returns the worst render cost seen.
func (f *FrameTelemetry) MaxRenderCost() time.Duration { return f.maxRenderCost }
