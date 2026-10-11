package tui

import (
	"fmt"
	"strings"

	"charm.land/lipgloss/v2"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// C3: the contract strip, the gate block, and the verdict card.
// Every element renders from real engine events only: the contract
// from the turn's requirement extraction, the gate from the
// question event, the card from the verification verdict.

// verdictFromEvent converts a verification event into the card
// state. The word "verified" is allowed only at trust T2 or higher
// with passing evidence for every required requirement.
func verdictFromEvent(evt protocol.EngineEvent) VerdictState {
	if evt.Verification == nil {
		return VerdictState{}
	}
	ver := evt.Verification
	v := VerdictState{
		Active:      true,
		Trust:       ver.Tier,
		Lines:       nil,
		NotVerified: append([]string(nil), ver.NotVerified...),
		Branch:      "",
		Passed:      0,
		Required:    len(ver.Requirements),
	}
	for _, r := range ver.Requirements {
		v.Lines = append(v.Lines, VerdictLine{
			State:    r.State,
			ID:       r.ID,
			Text:     r.Text,
			Evidence: r.Evidence,
		})
		if r.State == "pass" {
			v.Passed++
		}
	}
	// "verified" appears only at T2+ with passing evidence for every
	// REQUIRED requirement. The NotVerified list may still carry
	// non-required notes (design §1.5) without blocking the
	// headline.
	everyRequiredPassed := v.Required > 0 && v.Passed == v.Required
	tierOK := ver.Tier == "T2" || ver.Tier == "T3"
	v.Verified = ver.Result == "verified" && tierOK && everyRequiredPassed
	return v
}

// classicContractLine renders the contract strip during a run:
// `contract  R1 ✓  R2 ✗  R3 ◐  R4 ○  R5 ■   1 of 5`, or counts only
// under 80 columns (design §5).
func (m AppModel) classicContractLine() string {
	if len(m.state.Contract) == 0 {
		return ""
	}
	var parts []string
	done := 0
	for _, c := range m.state.Contract {
		parts = append(parts, c.ID+" "+m.renderContractState(c.State))
		if c.State == "pass" {
			done++
		}
	}
	total := len(m.state.Contract)
	label := m.theme.Muted.Render("contract")
	if m.state.Width < 80 {
		return fmt.Sprintf("%s  %s %d of %d", label,
			m.theme.Muted.Render(m.summaryState(m.state.Contract)), done, total)
	}
	states := strings.Join(parts, "  ")
	count := m.theme.Muted.Render(fmt.Sprintf("%d of %d", done, total))
	return fmt.Sprintf("%s  %s   %s", label, states, count)
}

// summaryState is the strip's aggregate state for narrow widths.
func (m AppModel) summaryState(contract []protocol.ContractReq) string {
	for _, c := range contract {
		if c.State == "fail" || c.State == "needs" {
			return "\u2717"
		}
	}
	for _, c := range contract {
		if c.State == "running" {
			return "◐"
		}
	}
	allPass := true
	for _, c := range contract {
		if c.State != "pass" {
			allPass = false
			break
		}
	}
	if allPass {
		return "✓"
	}
	return "○"
}

// renderContractState maps a contract state to its glyph.
func (m AppModel) renderContractState(state string) string {
	switch state {
	case "pass":
		return m.renderStateGlyph(stateDone)
	case "fail":
		return m.renderStateGlyph(stateFailed)
	case "running":
		return m.renderStateGlyph(stateRunning)
	default:
		return m.renderStateGlyph("queued")
	}
}

// classicGate renders the gate block that replaces the activity line
// when a decision is needed (design §1.3). The safe option is
// focused and marked (default).
func (m AppModel) classicGate() string {
	q := m.state.QuestionModal
	if !q.Active || len(q.Questions) == 0 {
		return ""
	}
	cur := q.Questions[q.CurrentIdx]
	safe := cur.Safe
	if safe < 0 || safe >= len(cur.Options) {
		safe = 0
	}
	glyph := "■"
	if UseASCII() {
		glyph = "!"
	}
	head := m.theme.BadgePerm.Render(glyph+" needs you") + "  " + m.theme.UserText.Render(cur.Question)

	// Letters are semantic, not positional: the option that keeps
	// the constraint is always n; the others take y, v, d in order.
	letters := []string{"y", "v", "d"}
	var optParts []string
	next := 0
	for i, opt := range cur.Options {
		letter := "?"
		if i == safe {
			letter = "n"
		} else if next < len(letters) {
			letter = letters[next]
			next++
		}
		label := "[" + letter + "] " + opt
		if i == safe {
			label += " (default)"
		}
		style := m.theme.Muted
		if i == safe {
			style = m.theme.Accent
		}
		optParts = append(optParts, style.Render(label))
	}
	options := "  " + strings.Join(optParts, "    ")
	hintText := "  n keep the constraint " + sep() + " y allow once " + sep() + " v view diff " + sep() + " esc skip"
	if m.state.Width > 0 && CellWidth(hintText) > m.state.Width {
		hintText = TruncateTo(hintText, m.state.Width)
	}
	hint := m.theme.Muted.Render(hintText)
	if m.state.Width < 80 {
		// 50-79: one option per line so nothing is cut off.
		var lines []string
		lines = append(lines, head, hint)
		lines = append(lines, strings.Split(options, "    ")...)
		return strings.Join(lines, "\n")
	}
	return head + "\n" + options + "\n" + hint
}

// classicVerdictCard renders the end-of-run card (design §1.5).
func (m AppModel) classicVerdictCard() string {
	v := m.state.Verdict
	if !v.Active {
		return ""
	}
	width := m.state.Width
	if width < 20 {
		width = 20
	}
	var b strings.Builder
	if v.Verified {
		head := m.theme.Success.Render("verified") + "  " +
			m.theme.Assistant.Render(fmt.Sprintf("%d of %d required", v.Passed, v.Required)) + "  " + sep() + "  " +
			m.theme.Muted.Render("trust "+trustLabel(v.Trust))
		b.WriteString(head + "\n")
		b.WriteString(strings.Repeat(ruleChar(), width) + "\n")
		for _, line := range v.Lines {
			b.WriteString(m.verdictLine(line) + "\n")
		}
		// Non-required notes (things no check covers) are listed
		// even on a verified card — the design's honest line.
		for _, nv := range v.NotVerified {
			b.WriteString(m.theme.Muted.Render("not verified  "+localizeSep(nv)) + "\n")
		}
	} else {
		reason := "the run ended without a passing verification"
		if len(v.NotVerified) > 0 {
			reason = v.NotVerified[0]
		}
		b.WriteString(m.theme.Error.Render("not verified") + "  " + m.theme.Assistant.Render(localizeSep(reason)) + "\n")
		b.WriteString(strings.Repeat(ruleChar(), width) + "\n")
		for _, line := range v.Lines {
			b.WriteString(m.verdictLine(line) + "\n")
		}
		for _, nv := range v.NotVerified {
			b.WriteString("  " + m.theme.Muted.Render(localizeSep(nv)) + "\n")
		}
	}
	facts := []string{m.theme.Muted.Render("branch " + orBranch(v.Branch))}
	if m.state.TotalCost > 0 {
		facts = append(facts, m.theme.Muted.Render(fmtCost(m.state.TotalCost)))
	}
	b.WriteString(strings.Join(facts, "  "))
	return strings.TrimRight(b.String(), "\n")
}

// verdictLine renders one requirement evidence row: glyph, id,
// text, evidence.
func (m AppModel) verdictLine(line VerdictLine) string {
	glyph := m.renderContractState(line.State)
	id := m.theme.ToolName.Render(line.ID)
	text := m.theme.Assistant.Render(localizeSep(line.Text))
	evidence := m.theme.Muted.Render(localizeSep(line.Evidence))
	width := m.state.Width
	left := glyph + " " + id + "  " + text
	leftW := lipgloss.Width(left)
	evW := lipgloss.Width(evidence)
	if width >= 80 && leftW+evW+2 <= width {
		pad := width - leftW - evW
		return left + strings.Repeat(" ", pad) + evidence
	}
	return left + "\n      " + evidence
}

// trustLabel names the trust tier for the card headline.
func trustLabel(tier string) string {
	switch tier {
	case "T3":
		return "T3 integrity-clean"
	case "T2":
		return "T2 baseline-validated"
	case "T1":
		return "T1 weak"
	default:
		return "T0 nothing ran"
	}
}

func orBranch(b string) string {
	if b == "" {
		return "(none)"
	}
	return b
}

func fmtCost(c float64) string {
	return fmt.Sprintf("$%.4f", c)
}
