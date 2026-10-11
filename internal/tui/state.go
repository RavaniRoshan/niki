package tui

import (
	"time"

	"github.com/RavaniRoshan/niki/internal/git"
	"github.com/RavaniRoshan/niki/internal/protocol"
)

// State holds UI presentation state.
type State struct {
	Ready  bool
	Width  int
	Height int
	Busy   bool
	// Inline marks scrollback (non-altscreen) mode,
	// where finalized history belongs to the
	// terminal's native scrollback (U9).
	Inline bool
	// Layout selects the renderer: "classic" (the
	// Classic Flow, default) or "ledger" (the
	// previous full-screen layout, experimental)
	// (C0).
	Layout string
	// TurnOutcome is set by real turn-end events and drives the
	// orb state only: "done", "error", "interrupted", or ""
	// (C0). Never invented between events.
	TurnOutcome string
	// Gate is the active decision block (C3); Active is false
	// until a gate event arrives.
	Gate GateState
	// Verdict is the end-of-run card state (C3); Active is false
	// until a verification event arrives.
	Verdict VerdictState
	// Debug toggles the telemetry overlay (B9).
	Debug bool
	// Activity is the single live activity line
	// above the composer, driven by real events
	// only (U3).
	Activity string
	// ReducedMotion disables non-essential animation
	// (U8).
	ReducedMotion bool
	// SpinFrame is the activity-sweep position, advanced only
	// while Busy (120ms cadence). Idle schedules no ticks, so
	// the B5 zero-redraw gate is unaffected.
	SpinFrame int

	// Plan is the latest plan from a real plan event (C2);
	// the classic next line renders only from it.
	Plan []protocol.PlanStep
	// Contract is the turn's requirement contract (C3): extracted
	// from the prompt at turn start and re-stated by the
	// verification event. The contract strip renders only this.
	Contract []protocol.ContractReq

	// Demo replays the recorded five-frame fixture (C3): a
	// scripted run, clearly not a live model. DemoStep advances
	// through classicStageEvents; -1 when inactive.
	Demo     bool
	DemoStep int

	// HideContractUI renders Classic without the contract strip and
	// the verdict card — the plain-transcript variant of the C5
	// validation study (design §9). Off by default.
	HideContractUI bool

	// Environment & Session Metadata
	Directory      string
	SessionID      string
	ModelName      string
	Version        string
	PermissionMode string
	Mode           string
	ThinkingEffort string
	ShellInputMode bool
	GitBranch      string
	GitSubmods     []git.Submodule

	// Token Context Meter & Cost Accounting
	UsedTokens int
	MaxTokens  int
	TotalCost  float64

	// L2 budget gauges: state (ok/wrap/stopped) and worst enabled
	// utilization fraction for the current run.
	BudgetState string
	BudgetPct   float64

	// Visual & Interactive Customizations
	SpinnerStyle     SpinnerStyle
	Palette          PaletteState
	ExpandToolOutput bool
	QuestionModal    QuestionState
	DiffViewer       DiffViewerState
	Btw              BtwState
	SwarmAgents      []SwarmAgent
	SessionPicker    SessionPickerState
	MentionOverlay   MentionOverlayState
}

// FrameTelemetry records event counters and boot-phase
// timings for the debug overlay (B9). Render timing is
// deliberately not collected: View() is a pure function
// (no clock reads), so per-frame render cost is not
// measurable inside the render path.
type FrameTelemetry struct {
	Events     int
	Deltas     int
	BootPhases []BootPhase
}

// BootPhase is one recorded boot phase timing.
type BootPhase struct {
	Name     string
	Duration time.Duration
}
