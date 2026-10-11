package verifier

import (
	"fmt"
	"strings"
)

// Requirement is one numbered, testable item extracted from the
// request: the contract the checks must satisfy (V1).
type Requirement struct {
	ID   string `json:"id"`
	Text string `json:"text"`
}

// maxRequirementRunes bounds a single requirement's text.
const maxRequirementRunes = 200

// ExtractRequirements turns a request into numbered, testable
// requirements (V1). It is deterministic and model-free: the prompt
// is split into sentences, filler is dropped, and each surviving
// sentence becomes one requirement R1..Rn in order. The contract is
// always explicit — an empty prompt yields one requirement ("do the
// task as stated") so there is never a vacuous contract.
func ExtractRequirements(prompt string) []Requirement {
	prompt = strings.TrimSpace(prompt)
	if prompt == "" {
		return []Requirement{{ID: "R1", Text: "do the task as stated"}}
	}
	// Split on sentence ends and newlines, keeping order.
	raw := strings.FieldsFunc(prompt, func(r rune) bool {
		return r == '\n' || r == '.' || r == '!' || r == '?' || r == ';'
	})
	var out []Requirement
	for _, s := range raw {
		s = strings.TrimSpace(strings.Trim(s, "-*•> \t"))
		if len(s) < 3 {
			continue
		}
		if isFiller(s) {
			continue
		}
		runes := []rune(s)
		if len(runes) > maxRequirementRunes {
			s = string(runes[:maxRequirementRunes])
		}
		out = append(out, Requirement{ID: fmt.Sprintf("R%d", len(out)+1), Text: s})
	}
	if len(out) == 0 {
		return []Requirement{{ID: "R1", Text: "do the task as stated"}}
	}
	return out
}

// isFiller drops sentences that carry no testable content.
func isFiller(s string) bool {
	lower := strings.ToLower(s)
	for _, f := range []string{"please", "thanks", "thank you", "hello", "hi there"} {
		if lower == f {
			return true
		}
	}
	return false
}
