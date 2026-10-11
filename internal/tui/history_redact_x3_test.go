package tui

import (
	"strings"
	"testing"
)

// X3 (UI): the visible transcript never retains credential text,
// including streamed deltas.
func TestHistoryRedactsSecrets(t *testing.T) {
	secret := "sk-plantedsecretKEY1234567890"
	var h History
	h.Append("tool", "output with "+secret+" inside")
	h.AppendDelta("streamed " + secret + " tail")
	for _, c := range h.Cells {
		if strings.Contains(c.Text, secret) {
			t.Fatalf("[X3] UI transcript leaked: %q", c.Text)
		}
	}
	if !strings.Contains(h.Cells[0].Text, "[REDACTED]") {
		t.Fatalf("[X3] UI transcript missing marker: %q", h.Cells[0].Text)
	}
	t.Logf("[X3] UI history cells scrubbed, marker present")
}
