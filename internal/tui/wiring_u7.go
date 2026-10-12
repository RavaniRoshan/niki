package tui

// U7 wiring audit: the single source of truth for every rendered
// element in the Workbench. Each entry names the REAL Op/Event that
// drives the element, its absent-state rule, and the test that
// proves absent-before/present-after. The audit test fails the
// build if any element has no event, any DEFER has no reason, any
// proven claim names a test that does not exist, or this registry
// drifts from docs/tui/WORKBENCH.md's wiring table.

// WiringElement is one rendered element.
type WiringElement struct {
	// ID is the stable id used by tests and logs.
	ID string
	// DocElement is the exact element label in WORKBENCH.md §3.
	DocElement string
	// Region names where it renders.
	Region string
	// Driver is the real Op/Event source. Empty fails the audit:
	// no element may exist without an event.
	Driver string
	// Absent is the absent-state rule ("" when always present).
	Absent string
	// Status is HAVE or DEFER.
	Status string
	// DeferWhy is required for DEFER elements.
	DeferWhy string
	// Proof names the test proving absent-before/present-after
	// (required for HAVE elements with an absent state).
	Proof string
}

const (
	wiringHave  = "HAVE"
	wiringDefer = "DEFER"
)

// WiringTable is the registry.
var WiringTable = []WiringElement{
	{ID: "header-product", DocElement: "product/version", Region: "header",
		Driver: "build info via SetVersion (state.Version)", Status: wiringHave},
	{ID: "header-model", DocElement: "model", Region: "header",
		Driver: "config at boot (SetModel) + Op echo (modelPickedMsg/keyConnectedMsg/ollamaConfiguredMsg)",
		Absent:  "shows the em dash until a model is configured",
		Status:  wiringHave,
		Proof:   "TestWorkbenchModelHeaderAbsentThenPresent"},
	{ID: "header-mode", DocElement: "mode", Region: "header",
		Driver: "ModeChanged events + boot echo (SetMode/SetPermissionMode)", Status: wiringHave},
	{ID: "header-env", DocElement: "cwd + branch", Region: "header",
		Driver: "the real environment at boot (SetDirectory/SetGitBranch)", Status: wiringHave},
	{ID: "header-mascot", DocElement: "mascot (Nib)", Region: "header",
		Driver: "the run state machine (Busy/TurnOutcome/Gate/Verdict)",
		Absent:  "never (idle state)",
		Status:  wiringHave,
		Proof:   "TestNibStatesHeaderAndStatusBar"},
	{ID: "rail-plan", DocElement: "Plan list", Region: "left rail",
		Driver:  "plan_updated (Plan []PlanStep)",
		Absent:  "ABSENT when no plan event has arrived",
		Status:  wiringHave,
		Proof:   "TestU3PlanRail"},
	{ID: "rail-agents", DocElement: "Agents list", Region: "left rail",
		Driver:  "subagent_started / subagent_completed / subagent_failed",
		Absent:  "ABSENT when no subagent event",
		Status:  wiringHave,
		Proof:   "TestU3AgentsRail"},
	{ID: "centre-request", DocElement: "request line", Region: "centre",
		Driver:  "the user's actual submit (CmdSubmitPrompt + local append)",
		Absent:  "ABSENT before the first submit",
		Status:  wiringHave,
		Proof:   "TestTraceRequestAndResultLines"},
	{ID: "centre-steps", DocElement: "Step headers", Region: "centre",
		Driver:  "EngineEvent.Step on tool events + plan_updated",
		Absent:  "ABSENT until a tool is attributed to a step",
		Status:  wiringHave,
		Proof:   "TestTraceStepHeaders"},
	{ID: "centre-tools", DocElement: "Tool rows", Region: "centre",
		Driver:  "tool_started / tool_completed / tool_failed (Target/Result/Excerpt)",
		Absent:  "ABSENT when no tool ran",
		Status:  wiringHave,
		Proof:   "TestTraceRequestAndResultLines"},
	{ID: "centre-diff", DocElement: "Diff block", Region: "centre",
		Driver:  "EngineEvent.Diff on the patching tool's completion",
		Absent:  "ABSENT when no patch event fired",
		Status:  wiringHave,
		Proof:   "TestTraceDiffBlock"},
	{ID: "centre-reasoning", DocElement: "Reasoning line", Region: "centre",
		Driver:   "reasoning_delta (seam event; renderer built)",
		Absent:   "ABSENT when the model produced none",
		Status:   wiringDefer,
		DeferWhy: "no provider emits reasoning_delta yet, so live runs show no line; the renderer and its event are proven through the same path",
		Proof:    "TestTraceReasoningLine"},
	{ID: "centre-activity", DocElement: "Activity line", Region: "centre",
		Driver:  "the in-flight op from real events (turn_started, tool_started, subagent_*, mcp_*, steer, budget_*, assistant_text_delta)",
		Absent:  "ABSENT when idle",
		Status:  wiringHave,
		Proof:   "TestTraceActivityLine"},
	{ID: "centre-prose", DocElement: "Prose", Region: "centre",
		Driver:  "assistant_text_delta (streamed) -> assistant_message_done (settled)",
		Absent:  "ABSENT before the first delta",
		Status:  wiringHave,
		Proof:   "TestWorkbenchAndStreamSameEvents"},
	{ID: "centre-interrupt", DocElement: "Esc-interrupt hint", Region: "centre",
		Driver:  "Busy + the registry's esc key",
		Absent:  "ABSENT when idle",
		Status:  wiringHave,
		Proof:   "TestWorkbenchElementsAbsentBeforePresentAfter"},
	{ID: "composer-input", DocElement: "input", Region: "composer",
		Driver: "local editor state (the real textarea)", Status: wiringHave},
	{ID: "composer-chip", DocElement: "mode chip", Region: "composer",
		Driver: "ModeChanged events + the registry's palette hint", Status: wiringHave,
		Proof: "TestModeChipFromModeChangedEvents"},
	{ID: "popup-approvals", DocElement: "Approvals (gate)", Region: "popup",
		Driver:  "question_prompted (Questions, Safe index) — the engine's real approval emitter",
		Absent:  "ABSENT when nothing is pending",
		Status:  wiringHave,
		Proof:   "TestApprovalsPopup"},
	{ID: "popup-palette", DocElement: "Command palette", Region: "popup",
		Driver:  "the one registry: DefaultPaletteCatalog + CoreSlashCommands + KeyBindings",
		Absent:  "ABSENT until invoked",
		Status:  wiringHave,
		Proof:   "TestPaletteOpenAbsentUntilInvoked"},
	{ID: "popup-diffviewer", DocElement: "Diff viewer", Region: "popup",
		Driver:  "the patch events' accumulated diffs + /diff's labeled working-tree read",
		Absent:  "ABSENT until a diff is shown",
		Status:  wiringHave,
		Proof:   "TestDiffViewerFromRealDiff"},
	{ID: "popup-agentdetail", DocElement: "Agent detail", Region: "popup",
		Driver:   "none — the subagent child stream is intentionally not forwarded (engine/subagent.go)",
		Status:   wiringDefer,
		DeferWhy: "forwarding the child stream is a core design change; the panel is absent rather than fabricated"},
	{ID: "rail-context", DocElement: "Context meter", Region: "right rail",
		Driver:  "Usage on assistant_message_done (+ window from SetModel)",
		Absent:  "shows the em dash until the first usage event; never a fake 0",
		Status:  wiringHave,
		Proof:   "TestU3ContextMeter"},
	{ID: "rail-checkpoints", DocElement: "Checkpoints", Region: "right rail",
		Driver:  "checkpoint_created (Text = the checkpoint's turn number)",
		Absent:  "ABSENT when none exist",
		Status:  wiringHave,
		Proof:   "TestU3CheckpointsRail"},
	{ID: "rail-cost", DocElement: "Cost", Region: "right rail",
		Driver:  "Usage -> routing.CalculateCost (turn and session accumulators)",
		Absent:  "the em dash until the provider reports usage",
		Status:  wiringHave,
		Proof:   "TestU3CostRail"},
	{ID: "status-mascot", DocElement: "mascot + run state", Region: "status bar",
		Driver:  "the run state machine",
		Absent:  "never (idle state)",
		Status:  wiringHave,
		Proof:   "TestNibHeaderAndStatusBarAgree"},
	{ID: "status-counts", DocElement: "agent count / checkpoint count", Region: "status bar",
		Driver:  "the live agent and checkpoint lists",
		Absent:  "ABSENT when zero",
		Status:  wiringHave,
		Proof:   "TestU3CheckpointsRail"},
	{ID: "status-interrupt", DocElement: "interrupt hint", Region: "status bar",
		Driver:  "the registry's esc key + Busy",
		Absent:  "ABSENT when idle",
		Status:  wiringHave,
		Proof:   "TestWorkbenchShellRendersAllRegions"},
	{ID: "notice-row", DocElement: "notice row", Region: "trace row",
		Driver:  "warning, mcp_server_ready/failed, config_reloaded, context_compacted, skill_discovered, steer",
		Absent:  "ABSENT when none fired",
		Status:  wiringHave,
		Proof:   "TestTraceNoticeRow"},
	{ID: "error-banner", DocElement: "error banner", Region: "trace row",
		Driver:  "error, turn_failed, turn_not_verified",
		Absent:  "ABSENT when no error",
		Status:  wiringHave,
		Proof:   "TestWorkbenchElementsAbsentBeforePresentAfter"},
	{ID: "contract-strip", DocElement: "contract strip + verdict card", Region: "live area",
		Driver:  "turn_started.Contract -> verification / turn_not_verified",
		Absent:  "contract ABSENT before turn_started; verdict ABSENT until the verification event",
		Status:  wiringHave,
		Proof:   "TestWorkbenchElementsAbsentBeforePresentAfter"},
}

// WiringRemovals is the printed record of everything removed for
// having no event and no purpose (P20), with the reason.
var WiringRemovals = []struct{ Element, Reason string }{
	{"/btw overlay and command", "no event source exists for a docked mini-agent, and its response text was fabricated ('verified in docs/PACK.md…') — deleted, not hidden"},
	{"Swarm progress renderer (RenderSwarmProgress, BrailleLevels, SwarmAgent, State.SwarmAgents)", "no event ever filled SwarmAgents — the subagent lifecycle events feed the Agents rail instead; the renderer was unreachable chrome"},
	{"spinGlyph helper", "dead function superseded by SpinGlyphWithStyle; its width-stability test now points at the live function"},
	{"State.ShellInputMode", "written by nothing, read by nothing"},
	{"Composer's 'queue with ctrl+enter' affordance", "no queue state exists in the TUI and no QueueChanged event exists; the placeholder no longer advertises queueing"},
	{"Transcript pager row in the wiring table", "no implementation, no key, and no event — removed from the table rather than listed as HAVE"},
	{"/agents' hardcoded '0 active child agents'", "fabricated count; the command now renders the real agent list from the real subagent events"},
	{"/export's 'ready for session' confirmation", "fabricated confirmation for a file that was never written; the command now writes the real transcript to a real file or reports the real failure"},
	{"/compact's 'Past turns folded' confirmation", "fabricated confirmation; the engine now has a real CmdCompact handler that compacts or refuses with a real event"},
}
