package verifier

import (
	"testing"
)

// V1: the request becomes numbered, testable requirements — golden
// contracts for five fixture requests.
func TestExtractRequirementsGolden(t *testing.T) {
	fixtures := []struct {
		name   string
		prompt string
		want   []string
	}{
		{"empty", "", []string{"do the task as stated"}},
		{"single", "Fix the login redirect loop.", []string{"Fix the login redirect loop"}},
		{"multi", "Add retries with jitter.\nLog every attempt.\nThanks!", []string{"Add retries with jitter", "Log every attempt"}},
		{"bullets", "- Ship the parser\n- Keep CRLF intact\n- hello", []string{"Ship the parser", "Keep CRLF intact"}},
		{"prose", "The cache must expire entries after 60 seconds! Stale reads are a bug; evict eagerly.", []string{"The cache must expire entries after 60 seconds", "Stale reads are a bug", "evict eagerly"}},
	}
	for _, f := range fixtures {
		got := ExtractRequirements(f.prompt)
		if len(got) != len(f.want) {
			t.Fatalf("[V1 %s] got %d requirements %v, want %d %v", f.name, len(got), got, len(f.want), f.want)
		}
		for i := range f.want {
			if got[i].ID != "R"+itoa(i+1) || got[i].Text != f.want[i] {
				t.Fatalf("[V1 %s] req %d = %+v, want R%d %q", f.name, i, got[i], i+1, f.want[i])
			}
		}
		t.Logf("[V1 %s] contract: %v", f.name, f.want)
	}
}

func itoa(i int) string {
	if i < 10 {
		return string(rune('0' + i))
	}
	return "R?"
}
