package engine

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"strings"
	"time"

	"github.com/RavaniRoshan/niki/internal/budget"
	"github.com/RavaniRoshan/niki/internal/checkpoint"
	"github.com/RavaniRoshan/niki/internal/diagnostics"
	"github.com/RavaniRoshan/niki/internal/envctx"
	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/routing"
	"github.com/RavaniRoshan/niki/internal/tools"
	"github.com/RavaniRoshan/niki/internal/undo"
	"github.com/RavaniRoshan/niki/internal/verifier"
)

// TurnRunner executes a single turn: stream model, dispatch tools, loop.
type TurnRunner struct {
	Provider     provider.ModelProvider
	Registry     *tools.Registry
	Context      *ContextAssembler
	Perm         *permissions.Guard
	Hooks        *HookRunner
	SteerChannel chan string

	// L2 budgets (per turn): tracker, cost model name, and the check
	// that produces the WRAP/stop verdict.
	budgetTracker *budget.Tracker
	budgetModel   string
	budgetCheck   func(ctx context.Context) (string, error)

	// L3 verifier gate: a run ends as done only through the verifier.
	verifier      verifier.Verifier
	verMaxLoops   int
	verifyAttempt int

	// L5: reports whether an Esc pause is pending; the turn ends at
	// the next safe point (between tool calls) with all edits kept.
	PauseRequested func() bool

	// E3: per-turn checkpoints (files captured at first touch, git
	// tree snapshot at turn start). Nil disables checkpointing.
	Checkpoints *checkpoint.Manager
	// E4: the session undo stack; file-mutating tool effects are
	// pushed here so CmdUndo can revert the last agent change.
	Undo *undo.Stack
	// HistoryEvents supplies the conversation snapshot stored in each
	// new checkpoint (events recorded before this turn).
	HistoryEvents func() []protocol.EngineEvent

	// currentTurn is the turn id of the Run currently in flight;
	// dispatchTool keys checkpoint/undo captures by it.
	currentTurn string

	// envWatches the working directory (C9): collected once, the
	// block is refreshed on the context whenever it changes.
	env *envctx.Watcher
}

// streamRetryBase is the first-retry floor for L6 stream retries
// (jittered to [0.5x, 1.5x) of base*2^attempt).
const streamRetryBase = 50 * time.Millisecond

func (t *TurnRunner) pauseRequested() bool {
	return t.PauseRequested != nil && t.PauseRequested()
}

func (t *TurnRunner) Run(ctx context.Context, prompt string, emit func(protocol.EngineEvent)) error {
	turnID := protocol.NewTurnId()
	t.currentTurn = string(turnID)
	emit(protocol.EngineEvent{Type: protocol.EventTurnStarted, Timestamp: time.Now(), TurnID: turnID})

	// C9: environment context is collected once and refreshed on
	// change — the block on the context always names the current
	// OS/shell/cwd/branch/status.
	if t.env == nil {
		t.env = envctx.New()
	}
	if cwd, err := os.Getwd(); err == nil {
		if snap, changed := t.env.Refresh(cwd); changed || t.Context.EnvBlock == "" {
			t.Context.SetEnvBlock(snap.Block())
		}
	}

	// E3: a checkpoint exists BEFORE the turn does anything — files
	// are captured at first touch, the git tree snapshot (shadow ref,
	// never the user's branches) is taken now.
	if t.Checkpoints != nil {
		var hist []protocol.EngineEvent
		if t.HistoryEvents != nil {
			hist = t.HistoryEvents()
		}
		_, _ = t.Checkpoints.CreateCheckpoint(string(turnID), nil, hist)
		if cwd, err := os.Getwd(); err == nil {
			_, _ = t.Checkpoints.CreateGitTreeSnapshot(string(turnID), cwd)
		}
	}

	t.Context.Add(provider.Message{Role: "user", Content: prompt})
	if t.Context.Compact() {
		emit(protocol.EngineEvent{Type: protocol.EventContextCompacted, Timestamp: time.Now(), TurnID: turnID})
	}

	const maxSteps = 30
	const maxStreamAttempts = 3 // L6: 1 try + 2 jittered retries
	doomDetector := NewDoomLoopDetector(DefaultDoomLoopThreshold)
	var trippedDoomLoop bool
	// M1: consecutive schema-repair attempts for the current bad
	// call shape. Two retries carry the validator error plus a
	// schema excerpt back to the model; the third strike is a
	// logged failure, not another retry.
	var argRepairs int
	// O1: live usage totals for the final report. Every step's
	// reported usage accumulates here; the completed event carries
	// the totals so they match the fixture's recorded usage exactly.
	var turnUsage provider.Usage

	for step := 0; step < maxSteps; step++ {
		// L2 budget gauges: evaluated at each safe point (the step
		// boundary); on WRAP/stop the check runs and the run ends.
		if t.observeBudget(ctx, turnID, emit) {
			return WrapBudget(string(t.budgetTracker.State()))
		}
		// L5: a hard stop (Esc twice / interrupt) cancels ctx; end
		// here without mislabeling it as a graceful pause.
		if ctx.Err() != nil {
			emit(protocol.EngineEvent{Type: protocol.EventTurnCancelled, Timestamp: time.Now(), TurnID: turnID})
			return WrapCancelled()
		}
		// L5 safe point: a pending Esc pause ends the run here —
		// between tool calls — with every completed edit kept.
		if t.pauseRequested() {
			emit(protocol.EngineEvent{
				Type:      protocol.EventTurnCancelled,
				Timestamp: time.Now(),
				TurnID:    turnID,
				Text:      "paused at a safe point (Esc)",
			})
			return WrapCancelled()
		}
		// Mid-turn steering: drain any injected steering prompt from user without aborting turn
		if t.SteerChannel != nil {
			select {
			case steer := <-t.SteerChannel:
				if strings.TrimSpace(steer) != "" {
					t.Context.Add(provider.Message{Role: "user", Content: steer})
					// L4: the queued message becomes a steer at this
					// safe point and is shown as its own event.
					emit(protocol.EngineEvent{
						Type:      protocol.EventSteer,
						Timestamp: time.Now(),
						TurnID:    turnID,
						Text:      steer,
					})
				}
			default:
			}
		}

		projected := ProjectContext(t.Context.Snapshot())

		var (
			assistant strings.Builder
			toolCalls []provider.ToolCall
			stepUsage *provider.Usage
		)

		// L6: retryable stream errors (truncation, resets) get
		// jittered backoff retries — but only while this step has
		// emitted nothing, because re-streaming after output would
		// duplicate text. HTTP 429/5xx are retried inside the
		// providers themselves.
		var streamErr error
		for attempt := 0; ; attempt++ {
			var deltas <-chan provider.Delta
			var errs <-chan error
			if ts, ok := t.Provider.(provider.ToolStreamer); ok {
				deltas, errs = ts.StreamWithTools(ctx, projected, t.toolSpecs())
			} else {
				deltas, errs = t.Provider.Stream(ctx, projected)
			}
			gotDelta := false
			for d := range deltas {
				gotDelta = true
				switch d.Kind {
				case provider.DeltaText:
					assistant.WriteString(d.Text)
					emit(protocol.EngineEvent{Type: protocol.EventAssistantTextDelta, Timestamp: time.Now(), TurnID: turnID, Text: d.Text})
				case provider.DeltaToolCall:
					id := d.CallID
					if id == "" {
						id = string(protocol.NewToolCallId())
					}
					toolCalls = append(toolCalls, provider.ToolCall{
						ID:   id,
						Tool: d.ToolName,
						Args: d.ToolArgs,
					})
				case provider.DeltaUsage:
					if d.Usage != nil {
						stepUsage = d.Usage
						emit(protocol.EngineEvent{Type: protocol.EventAssistantMessageDone, Timestamp: time.Now(), TurnID: turnID, Usage: d.Usage})
					}
				case provider.DeltaDone:
				}
			}
			streamErr = <-errs
			if streamErr == nil {
				break
			}
			if ctx.Err() != nil {
				emit(protocol.EngineEvent{Type: protocol.EventTurnCancelled, Timestamp: time.Now(), TurnID: turnID})
				return WrapCancelled()
			}
			if !provider.IsRetryable(streamErr) || gotDelta || attempt >= maxStreamAttempts-1 {
				break
			}
			emit(protocol.EngineEvent{
				Type:      protocol.EventWarning,
				Timestamp: time.Now(),
				TurnID:    turnID,
				Text: fmt.Sprintf("stream error: %v — retrying %d/%d (jittered backoff)",
					streamErr, attempt+1, maxStreamAttempts-1),
			})
			select {
			case <-ctx.Done():
				emit(protocol.EngineEvent{Type: protocol.EventTurnCancelled, Timestamp: time.Now(), TurnID: turnID})
				return WrapCancelled()
			case <-time.After(provider.JitteredBackoff(streamRetryBase, attempt, nil)):
			}
		}
		if streamErr != nil {
			emit(protocol.EngineEvent{Type: protocol.EventTurnFailed, Timestamp: time.Now(), TurnID: turnID, Error: streamErr.Error()})
			return WrapProvider(streamErr)
		}
		if stepUsage != nil {
			turnUsage.PromptTokens += stepUsage.PromptTokens
			turnUsage.CompletionTokens += stepUsage.CompletionTokens
			turnUsage.TotalTokens += stepUsage.TotalTokens
		}

		if t.budgetTracker != nil {
			var stepCost float64
			if stepUsage != nil {
				stepCost = routing.CalculateCost(t.budgetModel, *stepUsage)
			}
			t.budgetTracker.AddStep(stepCost)
		}

		if assistant.Len() > 0 || len(toolCalls) > 0 {
			t.Context.Add(provider.Message{Role: "assistant", Content: assistant.String(), ToolCalls: toolCalls})
		}

		// If no tool calls were made, the model claims completion.
		// L3: the claim is recorded but never ends the run by itself —
		// only the verifier does.
		if len(toolCalls) == 0 {
			if stepUsage == nil {
				emit(protocol.EngineEvent{Type: protocol.EventAssistantMessageDone, Timestamp: time.Now(), TurnID: turnID})
			}
			claim := assistant.String()
			if t.verifier == nil {
				emit(protocol.EngineEvent{
					Type: protocol.EventTurnNotVerified, Timestamp: time.Now(), TurnID: turnID,
					Text: "completion claim recorded, but no verifier is configured (inconclusive is never a pass)",
				})
				return WrapNotVerified("no verifier configured")
			}
			t.verifyAttempt++
			v, verr := t.verifier.Verify(ctx, verifier.Request{Prompt: prompt, Claim: claim, Attempt: t.verifyAttempt})
			if verr != nil {
				// An erroring verifier is never a pass (V3).
				v = verifier.Verdict{Result: verifier.NotVerified, Reason: "verifier error: " + verr.Error()}
			}
			emit(protocol.EngineEvent{
				Type: protocol.EventVerification, Timestamp: time.Now(), TurnID: turnID,
				Verification: &protocol.VerificationInfo{Attempt: t.verifyAttempt, Result: string(v.Result), Reason: v.Reason},
			})
			if v.Passed() {
				// O1: the final report carries totals (tokens +
				// cost over the run's steps), matching the sum of
				// the per-step usage deltas.
				stepsUsed := step + 1
				report := turnUsage
				if report.TotalTokens == 0 {
					report.TotalTokens = report.PromptTokens + report.CompletionTokens
				}
				emit(protocol.EngineEvent{Type: protocol.EventTurnCompleted, Timestamp: time.Now(), TurnID: turnID,
					Usage: &report,
					Text:  fmt.Sprintf("usage: %d prompt + %d completion = %d tokens over %d steps (est. cost $%.4f)", report.PromptTokens, report.CompletionTokens, report.TotalTokens, stepsUsed, routing.CalculateCost(t.budgetModel, report)),
				})
				return nil
			}
			if t.verMaxLoops > 0 && t.verifyAttempt >= t.verMaxLoops {
				reason := fmt.Sprintf("verification failed after %d attempt(s): %s", t.verifyAttempt, v.Reason)
				emit(protocol.EngineEvent{Type: protocol.EventTurnNotVerified, Timestamp: time.Now(), TurnID: turnID, Text: reason})
				return WrapNotVerified(reason)
			}
			// Feed the failure back and keep working (unbounded loop,
			// backstopped by L2 budgets / maxSteps — D-04).
			t.Context.Add(provider.Message{
				Role:    "user",
				Content: fmt.Sprintf("Verification attempt %d did not pass (%s): %s. Fix the issues, re-run checks, and do not claim done until verification passes.", t.verifyAttempt, v.Result, v.Reason),
			})
			continue
		}

		// Dispatch tools
		for i, tc := range toolCalls {
			// L5: pause lands between tool calls — the current tool
			// finishes; skipped calls get explicit tool messages so
			// the assistant's tool_calls stay matched in context.
			if t.pauseRequested() {
				for _, sk := range toolCalls[i:] {
					t.Context.Add(provider.Message{
						Role: "tool", Name: sk.Tool, ToolCallID: sk.ID,
						Content: "Tool skipped: paused at a safe point (Esc)",
					})
				}
				emit(protocol.EngineEvent{
					Type:      protocol.EventTurnCancelled,
					Timestamp: time.Now(),
					TurnID:    turnID,
					Text:      "paused at a safe point (Esc)",
				})
				return WrapCancelled()
			}
			callID := tc.ID
			if callID == "" {
				callID = string(protocol.NewToolCallId())
			}
			emit(protocol.EngineEvent{
				Type:      protocol.EventToolStarted,
				Timestamp: time.Now(),
				TurnID:    turnID,
				CallID:    protocol.ToolCallId(callID),
				ToolName:  tc.Tool,
			})

			res, err := t.dispatchTool(ctx, tc.Tool, tc.Args)
			output := res.Output
			// Outcome before M1 repair nils the error for message
			// flow: validation failures must still count as
			// failures for the repetition guards.
			dispatchOK := err == nil
			if schema, bad := invalidArgsSchema(t.Registry, tc.Tool, err); bad {
				if argRepairs < 2 {
					argRepairs++
					output = fmt.Sprintf("Tool error: %v\n\nArguments failed schema validation (repair attempt %d of 2). Schema excerpt:\n%s\nFix the arguments and retry.",
						err, argRepairs, schema)
					err = nil // the repair, not the failure, goes back
				} else {
					argRepairs = 0
					emit(protocol.EngineEvent{
						Type:      protocol.EventWarning,
						Timestamp: time.Now(),
						TurnID:    turnID,
						Text:      fmt.Sprintf("tool call repair failed after 2 attempts for %s; giving up on these arguments", tc.Tool),
					})
				}
			} else if dispatchOK {
				argRepairs = 0
			}
			if err != nil {
				emit(protocol.EngineEvent{
					Type:      protocol.EventToolFailed,
					Timestamp: time.Now(),
					TurnID:    turnID,
					CallID:    protocol.ToolCallId(callID),
					Error:     err.Error(),
				})
				output = fmt.Sprintf("Tool error: %v", err)
			} else {
				emit(protocol.EngineEvent{
					Type:      protocol.EventToolCompleted,
					Timestamp: time.Now(),
					TurnID:    turnID,
					CallID:    protocol.ToolCallId(callID),
					ToolName:  tc.Tool,
					Text:      res.Output,
				})
				// X4: tool output is data, not instructions. Text
				// that asks for approval changes, network access,
				// or disclosure is flagged right here; it is still
				// delivered to the model as data.
				if kinds := tools.ScanDirectives(res.Output); len(kinds) > 0 {
					emit(protocol.EngineEvent{
						Type:      protocol.EventWarning,
						Timestamp: time.Now(),
						TurnID:    turnID,
						Text:      tools.DirectiveNote(kinds),
					})
				}
				if tc.Tool == "edit_file" || tc.Tool == "write_file" || tc.Tool == "apply_patch" {
					var pathObj struct {
						Path string `json:"path"`
					}
					_ = json.Unmarshal([]byte(tc.Args), &pathObj)
					if pathObj.Path != "" {
						diag := diagnostics.CollectDiagnostics(ctx, ".", pathObj.Path)
						if xml := diag.FormatXML(); xml != "" {
							output += "\n\n" + xml
						}
					}
				}
			}

			if n := doomDetector.RepeatCount(tc.Tool, tc.Args); n == 2 {
				// M6: the second identical call earns a reminder
				// with a different strategy, fed back to the model.
				emit(protocol.EngineEvent{
					Type:      protocol.EventWarning,
					Timestamp: time.Now(),
					TurnID:    turnID,
					Text:      fmt.Sprintf("same tool call twice in a row (%s) — try a different strategy", tc.Tool),
				})
				output += "\n\nReminder: you just made this exact tool call twice in a row. Try a different strategy instead of repeating it."
			}
			if doomDetector.Record(tc.Tool, tc.Args, dispatchOK) {
				trippedDoomLoop = true
				emit(protocol.EngineEvent{
					Type:      protocol.EventWarning,
					Timestamp: time.Now(),
					TurnID:    turnID,
					Text:      fmt.Sprintf("Stopped recurring failing tool call %s after 3 attempts (doom loop guard)", tc.Tool),
				})
			}

			t.Context.Add(provider.Message{Role: "tool", Content: output, Name: tc.Tool, ToolCallID: callID})
		}

		// Doom loop guard: 3 consecutive identical failures
		if trippedDoomLoop {
			break
		}
	}

	// Loop exhausted (step limit or doom-loop guard): the run is NOT
	// done — only the verifier can end a run as done (L3).
	emit(protocol.EngineEvent{Type: protocol.EventAssistantMessageDone, Timestamp: time.Now(), TurnID: turnID})
	reason := "step limit reached without verification"
	if trippedDoomLoop {
		reason = "stopped by doom-loop guard without verification"
	}
	emit(protocol.EngineEvent{Type: protocol.EventTurnNotVerified, Timestamp: time.Now(), TurnID: turnID, Text: reason})
	return WrapNotVerified(reason)
}

// beginBudget arms the per-turn budget tracker (L2). Called by the
// engine at turn start; nil-safe when budgets are disabled.
func (t *TurnRunner) beginBudget(l budget.Limits, model string, check func(ctx context.Context) (string, error)) {
	t.budgetModel = model
	t.budgetCheck = check
	if l.Enabled() {
		t.budgetTracker = budget.New(l)
	} else {
		t.budgetTracker = nil
	}
}

// budgetInfo converts a gauge snapshot into the wire payload.
func budgetInfo(g budget.Gauges) *protocol.BudgetInfo {
	return &protocol.BudgetInfo{
		State:       string(g.State),
		Hit:         string(g.Hit),
		Wall:        g.Wall,
		Cost:        g.Cost,
		Step:        g.Step,
		WallEnabled: g.WallEnabled,
		CostEnabled: g.CostEnabled,
		StepEnabled: g.StepEnabled,
		WallSeconds: g.WallUsed.Seconds(),
		CostUSD:     g.CostUsed,
		StepsUsed:   g.StepsUsed,
	}
}

// runBudgetCheck produces the verdict recorded with a WRAP/stop event.
func (t *TurnRunner) runBudgetCheck(ctx context.Context) string {
	if t.budgetCheck == nil {
		return "no-verifier"
	}
	v, err := t.budgetCheck(ctx)
	if err != nil {
		return "not-verified: " + err.Error()
	}
	if v == "" {
		return "not-verified"
	}
	return v
}

// observeBudget is called at each safe point (step boundary). It emits
// the gauge and, on WRAP/stopped, runs the check, emits the terminal
// budget event with the verdict, and ends the run.
func (t *TurnRunner) observeBudget(ctx context.Context, turnID protocol.TurnId, emit func(protocol.EngineEvent)) bool {
	if t.budgetTracker == nil {
		return false
	}
	g := t.budgetTracker.Observe()
	emit(protocol.EngineEvent{Type: protocol.EventBudgetGauge, Timestamp: time.Now(), TurnID: turnID, Budget: budgetInfo(g)})
	if g.State != budget.StateWrap && g.State != budget.StateStopped {
		return false
	}
	verdict := t.runBudgetCheck(ctx)
	info := budgetInfo(g)
	info.Verdict = verdict
	typ := protocol.EventBudgetWrap
	if g.State == budget.StateStopped {
		typ = protocol.EventBudgetStopped
	}
	emit(protocol.EngineEvent{Type: typ, Timestamp: time.Now(), TurnID: turnID, Budget: info})
	return true
}

// toolSpecs exposes the registry to the provider as request-side tool schemas.
// This is the request half of the tool loop: the model can only call tools the
// request told it about.
// planMutatingTools are never offered to the model during PLAN:
// the turn exposes only the tool family for the current phase (M3).
// Dispatch denies them too (permissions.Guard), so a smuggled call
// still cannot run.
func (t *TurnRunner) toolSpecs() []provider.ToolSpec {
	if t.Registry == nil {
		return nil
	}
	planMode := t.Perm != nil && t.Perm.InPlanMode()
	list := t.Registry.List()
	specs := make([]provider.ToolSpec, 0, len(list))
	for _, tool := range list {
		if planMode && !permissions.IsReadOnlyTool(tool.Name()) {
			continue
		}
		specs = append(specs, provider.ToolSpec{
			Name:        tool.Name(),
			Description: tool.Description(),
			Parameters:  provider.SchemaToJSONSchema(tool.Schema()),
		})
	}
	return specs
}

// invalidArgsSchema reports whether err is a schema-validation
// rejection for the named tool, returning the tool's schema excerpt
// for the M1 repair round-trip (validator error + schema go back to
// the model).
func invalidArgsSchema(reg *tools.Registry, name string, err error) (string, bool) {
	if err == nil || reg == nil {
		return "", false
	}
	if !strings.HasPrefix(err.Error(), "invalid args for "+name) {
		return "", false
	}
	tool, ok := reg.Get(name)
	if !ok {
		return "", false
	}
	return tool.Schema(), true
}

func (t *TurnRunner) dispatchTool(ctx context.Context, name, argsJSON string) (tools.ToolResult, error) {
	if t.Perm != nil && !t.Perm.Allow(name) {
		return tools.ToolResult{}, &Error{Kind: ErrPermission, Message: "tool not allowed in current mode: " + name}
	}
	if t.Hooks != nil {
		if err := t.Hooks.FirePreTool(HookContext{Point: HookPreToolUse, ToolName: name, Payload: argsJSON}); err != nil {
			return tools.ToolResult{}, &Error{Kind: ErrPermission, Message: "blocked by hook: " + err.Error()}
		}
	}

	// E3/E4: capture the pre-image of a file-mutating target before
	// the tool runs; after a successful run, record the change on the
	// undo stack and in the turn checkpoint.
	var pre *filePreImage
	if path, ok := mutatingFilePath(name, argsJSON); ok {
		pre = captureFilePreImage(path)
		if pre != nil && t.Checkpoints != nil && t.currentTurn != "" {
			// The checkpoint records its own first-touch copy too
			// (CapturePreImage is a no-op when already present).
			_ = t.Checkpoints.CapturePreImage(t.currentTurn, path)
		}
	}

	res, err := t.Registry.Run(ctx, name, jsonRaw(argsJSON))

	if pre != nil && err == nil && !res.IsError {
		t.recordFileChange(name, pre)
	}
	return res, err
}

// fileMutatingTools are the text tools whose targets are captured
// for E3 checkpoints and the E4 undo stack. Shell commands can also
// change files; those changes are covered by the turn checkpoint's
// git tree snapshot, not the per-file stack.
var fileMutatingTools = map[string]bool{
	"edit_file":     true,
	"write_file":    true,
	"apply_patch":   true,
	"notebook_edit": true,
}

func mutatingFilePath(name, argsJSON string) (string, bool) {
	if !fileMutatingTools[name] {
		return "", false
	}
	var a struct {
		Path string `json:"path"`
	}
	if err := json.Unmarshal([]byte(argsJSON), &a); err != nil || a.Path == "" {
		return "", false
	}
	return a.Path, true
}

// filePreImage is the file's state before a mutating tool ran.
type filePreImage struct {
	path    string
	existed bool
	before  []byte
	mode    os.FileMode
}

func captureFilePreImage(path string) *filePreImage {
	fi, err := os.Stat(path)
	if err != nil {
		if os.IsNotExist(err) {
			return &filePreImage{path: path, existed: false}
		}
		return nil
	}
	if fi.IsDir() {
		return nil
	}
	data, err := os.ReadFile(path)
	if err != nil {
		return nil
	}
	return &filePreImage{path: path, existed: true, before: data, mode: fi.Mode().Perm()}
}

// recordFileChange pushes the effect when the tool actually changed
// the bytes (or created the file) and records the post-state in the
// turn checkpoint.
func (t *TurnRunner) recordFileChange(tool string, pre *filePreImage) {
	post, err := os.ReadFile(pre.path)
	if err != nil {
		if os.IsNotExist(err) && !pre.existed {
			return // still absent: nothing changed
		}
		return
	}
	if pre.existed && bytes.Equal(pre.before, post) {
		return // no byte change
	}
	mode := pre.mode
	if fi, serr := os.Stat(pre.path); serr == nil {
		if !pre.existed {
			mode = fi.Mode().Perm()
		}
	}
	if t.Checkpoints != nil && t.currentTurn != "" {
		t.Checkpoints.RecordAfter(t.currentTurn, pre.path, post)
	}
	if t.Undo != nil {
		t.Undo.Push(undo.Effect{
			Path:    pre.path,
			TurnID:  t.currentTurn,
			Tool:    tool,
			Existed: pre.existed,
			Before:  pre.before,
			After:   post,
			Mode:    mode,
		})
	}
}

func jsonRaw(s string) json.RawMessage {
	if s == "" {
		return json.RawMessage("{}")
	}
	return json.RawMessage(s)
}
