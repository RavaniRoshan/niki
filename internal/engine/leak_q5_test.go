package engine

import (
	"runtime"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
)

// Q5: goroutine-leak check on a full run — after the turn completes
// and the engine stops, no run goroutines may remain.
func TestFullRunLeaksNoGoroutines(t *testing.T) {
	baseline := settledGoroutines()
	p := &scriptProvider{steps: [][]provider.ToolCall{
		{{ID: "c1", Tool: "read_file", Args: `{"path":"leak_q5_test.go"}`}},
	}}
	eng, cmdChan, eventChan := newScriptEngine(t, p)
	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "read"}
	drainUntil(t, eventChan, protocol.EventTurnCompleted, 15*time.Second)
	eng.Stop()

	deadline := time.Now().Add(10 * time.Second)
	for {
		if n := settledGoroutines(); n <= baseline {
			t.Logf("[Q5] goroutines %d -> %d (no leak)", baseline, n)
			return
		} else if time.Now().After(deadline) {
			t.Fatalf("[Q5] goroutine leak: baseline %d, now %d", baseline, settledGoroutines())
		}
		time.Sleep(100 * time.Millisecond)
	}
}

// settledGoroutines GCs twice and counts, so one-off finalizers
// don't flake the measurement.
func settledGoroutines() int {
	runtime.GC()
	time.Sleep(50 * time.Millisecond)
	runtime.GC()
	return runtime.NumGoroutine()
}
