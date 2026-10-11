package tui

// GateState is the decision block that replaces the activity line
// when a run needs the user (docs/ui/CLASSIC.md §1.3). It is
// populated only by gate events (C3); Active is false until then.
type GateState struct {
	Active bool
	// Action is the thing needing a decision, e.g.
	// "add golang.org/x/time?".
	Action string
	// Reason names the conflicting requirement, e.g.
	// "R3 would use it. R5 says no new dependency."
	Reason string
	// Options are the choices; index 0 is the constraint-keeping
	// default and holds focus.
	Options []GateOption
	// Selected is the focused option index.
	Selected int
}

// GateOption is one gate choice.
type GateOption struct {
	// Key is the single letter that picks it ("n", "y", "v").
	Key string
	// Label is the shown text.
	Label string
	// Safe marks the option that keeps the constraint; it is the
	// default focus and is labeled "(default)".
	Safe bool
}

// VerdictState is the end-of-run verdict card (docs/ui/CLASSIC.md
// §1.5). Populated only by verification events (C3).
type VerdictState struct {
	Active bool
	// Verified is true only at trust T2 or higher with passing
	// evidence for every required requirement.
	Verified bool
	// Summary counts requirements: "5 of 5 required".
	Passed   int
	Required int
	// Trust names the tier, e.g. "trust T2 baseline-validated".
	Trust string
	// Lines are the per-requirement evidence rows.
	Lines []VerdictLine
	// NotVerified lists what failed or had no check; a non-empty
	// list forces Verified=false.
	NotVerified []string
	// Branch, Files, Plus, Minus, Cost, Duration are real run
	// counters for the card footer line.
	Branch   string
	Files    int
	Plus     int
	Minus    int
	Cost     float64
	Duration string
}

// VerdictLine is one requirement evidence row.
type VerdictLine struct {
	// State is "pass", "fail", or "queued" (glyph source).
	State string
	// ID is the requirement id, e.g. "R1".
	ID string
	// Text is the requirement statement.
	Text string
	// Evidence cites where it passed or failed.
	Evidence string
}
