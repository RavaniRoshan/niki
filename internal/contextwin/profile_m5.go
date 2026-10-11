package contextwin

// Small-window budgets (M5): the repository map, file reads, and tool
// output all have token limits sized to the model's context window,
// and compaction triggers earlier for small windows. A frontier model
// with a 200k window can afford generous budgets; an 8k-window local
// model must be fed far less or it degrades in long contexts.

// Profile sizes context budgets to a model's window.
type Profile struct {
	Name         string // e.g. "qwen2.5-coder:3b"
	WindowTokens int    // total context window in tokens
}

// Preset profiles. WindowTokens are the model's advertised context
// sizes; the budgets below are fractions of them.
var (
	// Profile8K is a small local model (e.g. a 3B quantized coder).
	Profile8K = Profile{Name: "small-8k", WindowTokens: 8_000}
	// Profile32K is a mid local model.
	Profile32K = Profile{Name: "mid-32k", WindowTokens: 32_000}
	// Profile128K is a large local or API model.
	Profile128K = Profile{Name: "large-128k", WindowTokens: 128_000}
	// Profile200K is a frontier model.
	Profile200K = Profile{Name: "frontier-200k", WindowTokens: 200_000}
)

// ProfileForWindow picks the closest preset for a window size.
func ProfileForWindow(window int) Profile {
	switch {
	case window <= 8_000:
		return Profile8K
	case window <= 32_000:
		return Profile32K
	case window <= 128_000:
		return Profile128K
	default:
		return Profile200K
	}
}

// Budgets are the per-surface token limits for a profile.
type Budgets struct {
	RepoMapTokens    int // the repository map (symbol outline)
	FileReadTokens   int // a single file read
	ToolOutputTokens int // a single tool result
	// CompactAtRatio is the context-usage fraction that triggers
	// compaction. Small windows compact earlier (more headroom for the
	// model to reason); large windows can run hotter.
	CompactAtRatio float64
}

// BudgetsFor sizes the budgets to the profile's window (M5). Every
// surface is a fixed fraction of the window, floored at a usable
// minimum and capped so no single surface can dominate a small window.
func (p Profile) Budgets() Budgets {
	w := p.WindowTokens
	if w <= 0 {
		w = 8_000
	}
	// Fractions tuned so an 8k model gets small, safe surfaces and a
	// 200k model gets generous ones. Repo map and file read scale with
	// the window; tool output is capped tighter to avoid one huge
	// result crowding out the conversation.
	repoMap := clamp(w/8, 200, 4000)
	fileRead := clamp(w/4, 400, 16000)
	toolOut := clamp(w/6, 300, 12000)

	// Compaction: small windows compact at 60% usage, large at 80%.
	compact := 0.80
	if w <= 32_000 {
		compact = 0.60
	}
	return Budgets{
		RepoMapTokens:    repoMap,
		FileReadTokens:   fileRead,
		ToolOutputTokens: toolOut,
		CompactAtRatio:   compact,
	}
}

// CompactThresholdTokens is the absolute token count at which
// compaction should trigger for this profile.
func (p Profile) CompactThresholdTokens() int {
	b := p.Budgets()
	return int(float64(p.WindowTokens) * b.CompactAtRatio)
}

func clamp(v, lo, hi int) int {
	if v < lo {
		return lo
	}
	if v > hi {
		return hi
	}
	return v
}
