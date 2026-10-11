package contextwin

import (
	"testing"
)

// M5: small-window budgets — the repo map, file reads, and tool output
// have token limits sized to the model profile, and compaction triggers
// earlier for small windows. Golden expectations frozen for an 8k
// window profile.
func TestSmallWindowBudgetsGolden(t *testing.T) {
	small := Profile8K.Budgets()

	// Golden: the 8k profile's budgets are the frozen expected values.
	if small.RepoMapTokens != 1000 {
		t.Fatalf("[M5] 8k repo-map budget = %d, want 1000", small.RepoMapTokens)
	}
	if small.FileReadTokens != 2000 {
		t.Fatalf("[M5] 8k file-read budget = %d, want 2000", small.FileReadTokens)
	}
	if small.ToolOutputTokens != 1333 {
		t.Fatalf("[M5] 8k tool-output budget = %d, want 1333", small.ToolOutputTokens)
	}
	if small.CompactAtRatio != 0.60 {
		t.Fatalf("[M5] 8k compact ratio = %v, want 0.60", small.CompactAtRatio)
	}
	t.Logf("[M5] 8k profile: repo-map=%d file-read=%d tool-output=%d compact@%.0f%%",
		small.RepoMapTokens, small.FileReadTokens, small.ToolOutputTokens, small.CompactAtRatio*100)

	// Compaction triggers EARLIER for small windows than large ones.
	large := Profile200K.Budgets()
	if !(small.CompactAtRatio < large.CompactAtRatio) {
		t.Fatalf("[M5] small window must compact earlier: small=%.2f large=%.2f",
			small.CompactAtRatio, large.CompactAtRatio)
	}
	if Profile8K.CompactThresholdTokens() >= Profile200K.CompactThresholdTokens()/10 {
		// 8k triggers at 60% of 8k = 4800; 200k at 80% of 200k = 160000.
		t.Fatalf("[M5] small-window compaction threshold not earlier in absolute terms")
	}
	t.Logf("[M5] compaction: 8k@%d tokens (%.0f%%) vs 200k@%d tokens (%.0f%%)",
		Profile8K.CompactThresholdTokens(), small.CompactAtRatio*100,
		Profile200K.CompactThresholdTokens(), large.CompactAtRatio*100)

	// Every surface budget is strictly smaller for the small window.
	if small.RepoMapTokens >= large.RepoMapTokens ||
		small.FileReadTokens >= large.FileReadTokens ||
		small.ToolOutputTokens >= large.ToolOutputTokens {
		t.Fatalf("[M5] small-window budgets must be strictly smaller than large-window")
	}
	t.Logf("[M5] 8k surfaces all smaller than 200k (repo-map %d<%d, file %d<%d, tool %d<%d)",
		small.RepoMapTokens, large.RepoMapTokens,
		small.FileReadTokens, large.FileReadTokens,
		small.ToolOutputTokens, large.ToolOutputTokens)

	// No surface can exceed the whole window (a small model is never
	// handed a map bigger than its context).
	for _, p := range []Profile{Profile8K, Profile32K, Profile128K, Profile200K} {
		b := p.Budgets()
		if b.RepoMapTokens > p.WindowTokens || b.FileReadTokens > p.WindowTokens || b.ToolOutputTokens > p.WindowTokens {
			t.Fatalf("[M5] %s: a surface budget exceeds the window", p.Name)
		}
	}
	t.Logf("[M5] no surface budget exceeds its window, for all presets")

	// ProfileForWindow snaps to the right preset.
	if ProfileForWindow(8000).Name != "small-8k" || ProfileForWindow(200000).Name != "frontier-200k" {
		t.Fatalf("[M5] ProfileForWindow snapped wrong")
	}
	t.Logf("[M5] window->profile snapping correct")
}
