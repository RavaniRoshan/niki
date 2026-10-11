package last

import (
	"fmt"
	"strings"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// Summary is a plain-language rundown of one completed run (O3): what
// was asked, what changed, what passed, what looped and why, and what
// is not verified.
type Summary struct {
	Asked      string   // the user prompt that started the run
	Changed    []string // files touched (tool evidence)
	Passed     []string // checks that passed
	Looped     []string // retried/repeated calls and why
	Unverified string   // what is not verified, and why not
}

// Summarize builds a Summary from the run's event log. The log is data:
// every section cites event text, and anything unverified says so.
func Summarize(evts []protocol.EngineEvent) Summary {
	var s Summary
	var toolsSeen []string
	retries := map[string]int{}
	for _, e := range evts {
		switch e.Type {
		case protocol.EventTurnStarted:
			if s.Asked == "" {
				s.Asked = e.Text
			}
		case protocol.EventToolStarted:
			toolsSeen = append(toolsSeen, e.ToolName)
		case protocol.EventToolCompleted:
			if e.ToolName != "" {
				s.Changed = appendTools(s.Changed, e.ToolName)
			}
		case protocol.EventVerification:
			if e.Verification != nil && e.Verification.Result == "verified" {
				s.Passed = append(s.Passed, "completion claim verified")
			}
		case protocol.EventTurnCompleted:
			if e.Usage != nil && e.Text != "" {
				s.Passed = append(s.Passed, strings.TrimSpace(e.Text))
			}
		case protocol.EventTurnNotVerified:
			s.Unverified = e.Text
		case protocol.EventWarning:
			if strings.Contains(e.Text, "retrying") {
				retries[e.Text]++
			}
		}
	}
	for text, n := range retries {
		s.Looped = append(s.Looped, fmt.Sprintf("%s (x%d)", text, n))
	}
	if len(toolsSeen) > 0 && len(s.Changed) == 0 {
		s.Changed = toolsSeen
	}
	return s
}

func appendTools(out []string, tool string) []string {
	for _, t := range out {
		if t == tool {
			return out
		}
	}
	return append(out, tool)
}

// Render writes a Summary in plain language (O3).
func Render(s Summary) string {
	var b strings.Builder
	b.WriteString("Last run:\n\n")
	if s.Asked != "" {
		b.WriteString("Asked: " + s.Asked + "\n\n")
	}
	if len(s.Changed) > 0 {
		b.WriteString("Changed: " + strings.Join(s.Changed, ", ") + "\n\n")
	}
	if len(s.Passed) > 0 {
		b.WriteString("Passed:\n")
		for _, p := range s.Passed {
			b.WriteString("- " + p + "\n")
		}
		b.WriteString("\n")
	}
	if len(s.Looped) > 0 {
		b.WriteString("Looped:\n")
		for _, l := range s.Looped {
			b.WriteString("- " + l + "\n")
		}
		b.WriteString("\n")
	}
	if s.Unverified != "" {
		b.WriteString("Not verified: " + s.Unverified + "\n")
	} else {
		b.WriteString("Not verified: nothing pending — the run ended verified.\n")
	}
	return b.String()
}
