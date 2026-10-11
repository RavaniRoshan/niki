package verifier

import (
	"fmt"
	"strings"
)

// Structured verdict evidence (C3): the verdict card shows only
// what the verifier actually measured — the tier, each
// requirement's state, and the evidence behind it. A requirement
// with no configured check is "queued" with that reason, never a
// pass.

// ReqEvidence is one requirement's measured state.
type ReqEvidence struct {
	ID       string `json:"id"`
	Text     string `json:"text"`
	State    string `json:"state"` // pass, fail, queued
	Evidence string `json:"evidence,omitempty"`
}

// TierCode is the short tier code ("T0".."T3").
func (t Tier) TierCode() string {
	switch t {
	case T3Clean:
		return "T3"
	case T2Passed:
		return "T2"
	case T1Weak:
		return "T1"
	default:
		return "T0"
	}
}

// structuredVerdict fills the structured evidence on a verdict from
// the real run: requirements in prompt order, each graded against
// its check when one exists.
func structuredVerdict(v Verdict, reqs []Requirement, results []CheckResult, tier Tier) Verdict {
	v.Tier = tier.TierCode()
	v.Requirements = make([]ReqEvidence, 0, len(reqs))
	for i, req := range reqs {
		ev := ReqEvidence{ID: req.ID, Text: req.Text}
		if i < len(results) {
			r := results[i]
			if r.Passed {
				ev.State = "pass"
				ev.Evidence = r.Check.Name + " · passed"
			} else {
				ev.State = "fail"
				why := r.Check.Name + " · failed"
				if first := firstLine(r.Output); first != "" {
					why = r.Check.Name + " · " + first
				}
				ev.Evidence = why
			}
		} else {
			ev.State = "queued"
			ev.Evidence = "no check is configured for it"
		}
		v.Requirements = append(v.Requirements, ev)
	}
	return v
}

func firstLine(s string) string {
	for _, line := range strings.Split(s, "\n") {
		if t := strings.TrimSpace(line); t != "" {
			if len(t) > 80 {
				t = t[:80] + "..."
			}
			return t
		}
	}
	return ""
}

// contractStates derives the contract strip states (R1..Rn) from the
// verdict evidence: the same measured states the card shows.
func ContractStates(reqs []ReqEvidence) []string {
	out := make([]string, 0, len(reqs))
	for _, r := range reqs {
		out = append(out, r.State)
	}
	return out
}

// notVerifiedLines lists what is not verified: failing requirements
// and requirements with no check. A verdict that passes never has
// entries here.
func NotVerifiedLines(reqs []ReqEvidence) []string {
	var out []string
	for _, r := range reqs {
		switch r.State {
		case "fail":
			out = append(out, fmt.Sprintf("%s %s: %s", r.ID, r.Text, r.Evidence))
		case "queued":
			out = append(out, fmt.Sprintf("%s %s: no check is configured for it", r.ID, r.Text))
		}
	}
	return out
}
