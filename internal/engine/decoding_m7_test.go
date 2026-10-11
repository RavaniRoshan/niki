package engine

import "testing"

// M7: decoding defaults per phase — low temperature for tool/plan
// phases, explicit output limits, stop sequences set.
func TestDecodingDefaultsPerPhase(t *testing.T) {
	tool := DecodingFor(PhaseTool)
	plan := DecodingFor(PhasePlan)
	text := DecodingFor(PhaseText)

	// Tool phase is the coldest.
	if tool.Temperature != 0.0 {
		t.Fatalf("[M7] tool temperature = %v, want 0.0", tool.Temperature)
	}
	// Plan is cold too, but allows a little exploration.
	if plan.Temperature > 0.2 {
		t.Fatalf("[M7] plan temperature = %v, want <= 0.2", plan.Temperature)
	}
	// Text runs warmer than the structured phases.
	if !(text.Temperature > plan.Temperature) {
		t.Fatalf("[M7] text (%v) must be warmer than plan (%v)", text.Temperature, plan.Temperature)
	}
	t.Logf("[M7] temperatures: tool=%.1f plan=%.1f text=%.1f (tool/plan cold)", tool.Temperature, plan.Temperature, text.Temperature)

	// Every phase sets an explicit output-token cap.
	for _, d := range []Decoding{tool, plan, text} {
		if d.MaxTokens <= 0 {
			t.Fatalf("[M7] phase missing explicit output limit: %+v", d)
		}
	}
	t.Logf("[M7] explicit output caps: tool=%d plan=%d text=%d", tool.MaxTokens, plan.MaxTokens, text.MaxTokens)

	// Every phase sets at least one stop sequence.
	for _, d := range []Decoding{tool, plan, text} {
		if len(d.Stop) == 0 {
			t.Fatalf("[M7] phase missing stop sequences: %+v", d)
		}
	}
	if tool.Stop[0] != toolCallEndStop {
		t.Fatalf("[M7] tool stop sequence wrong: %q", tool.Stop[0])
	}
	t.Logf("[M7] stop sequences set on every phase (tool ends at %q)", tool.Stop[0])

	// Unknown phase fails safe: coldest + tightest.
	def := DecodingFor(Phase("bogus"))
	if def.Temperature != 0.0 || def.MaxTokens != 1024 || len(def.Stop) == 0 {
		t.Fatalf("[M7] unknown phase not fail-safe: %+v", def)
	}
	t.Logf("[M7] unknown phase fails safe (temp=0, cap=%d, stops set)", def.MaxTokens)
}
