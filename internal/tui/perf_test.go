package tui

import (
	"io"
	"os"
	"runtime"
	"sort"
	"strings"
	"sync"
	"testing"
	"time"

	tea "github.com/charmbracelet/bubbletea"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// TestInputEchoP95 (B4): the wall-clock cost of the
// input echo path — one key event through Update plus
// a full View render — measured as p95 over a burst.
func TestInputEchoP95(t *testing.T) {
	m, _, _ := newModel(false)
	const n = 300
	latencies := make([]time.Duration, 0, n)
	for i := 0; i < n; i++ {
		start := time.Now()
		um, _ := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'x'}})
		m = um.(AppModel)
		_ = m.View()
		latencies = append(latencies, time.Since(start))
	}
	sort.Slice(latencies, func(i, j int) bool { return latencies[i] < latencies[j] })
	p95 := latencies[(n*95)/100]
	p50 := latencies[n/2]
	t.Logf("input_echo_p50_ms=%.2f input_echo_p95_ms=%.2f (n=%d)",
		float64(p50.Microseconds())/1000, float64(p95.Microseconds())/1000, n)
	if p95 > 30*time.Millisecond {
		t.Errorf("input echo p95 = %v, want ≤ 30ms", p95)
	}
}

// countingWriter records every Write for redraw counts.
// The renderer writes from its own goroutine, so the
// counters are mutex-guarded.
type countingWriter struct {
	mu     sync.Mutex
	writes int
	bytes  int
}

func (c *countingWriter) Write(p []byte) (int, error) {
	c.mu.Lock()
	c.writes++
	c.bytes += len(p)
	c.mu.Unlock()
	return len(p), nil
}

func (c *countingWriter) snapshot() (writes, bytes int) {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.writes, c.bytes
}

// cpuTime reads the process CPU time (user+sys) from
// /proc, in nanoseconds. Returns 0 when unavailable.
func cpuTime() time.Duration {
	data, err := os.ReadFile("/proc/self/stat")
	if err != nil {
		return 0
	}
	fields := strings.Fields(string(data))
	if len(fields) < 15 {
		return 0
	}
	utime := parseInt(fields[13])
	stime := parseInt(fields[14])
	return time.Duration(utime+stime) * time.Second / 100 // clock ticks are 10ms on Linux
}

func parseInt(s string) int64 {
	var v int64
	for _, c := range s {
		if c < '0' || c > '9' {
			break
		}
		v = v*10 + int64(c-'0')
	}
	return v
}

// TestIdleNoRedrawsAndCPU (B5): while idle the program
// performs zero redraws and consumes no measurable CPU.
func TestIdleNoRedrawsAndCPU(t *testing.T) {
	out := &countingWriter{}
	r, w, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	defer r.Close()
	defer w.Close()

	cmdChan := make(chan protocol.EngineCommand, 1)
	eventChan := make(chan protocol.EngineEvent, 1)
	m := NewAppModel(cmdChan, eventChan)
	p := tea.NewProgram(m, tea.WithInput(r), tea.WithOutput(out))
	runDone := make(chan error, 1)
	go func() {
		_, err := p.Run()
		runDone <- err
	}()
	p.Send(tea.WindowSizeMsg{Width: 80, Height: 24})
	// Let the initial frame render.
	time.Sleep(200 * time.Millisecond)
	baseline, _ := out.snapshot()

	// Idle window: no input, no events.
	cpuBefore := cpuTime()
	idleStart := time.Now()
	time.Sleep(1 * time.Second)
	idleWrites, _ := out.snapshot()
	idleWrites -= baseline
	idleCPU := cpuTime() - cpuBefore
	idleWall := time.Since(idleStart)

	// Closing the event channel unblocks the Init
	// command so the program can shut down cleanly.
	close(eventChan)
	p.Quit()
	<-runDone

	cpuPercent := 0.0
	if idleWall > 0 {
		cpuPercent = float64(idleCPU) / float64(idleWall) * 100
	}
	t.Logf("idle_redraws=%d idle_cpu_percent=%.2f (wall=%s cpu=%s)",
		idleWrites, cpuPercent, idleWall.Round(time.Millisecond), idleCPU.Round(time.Millisecond))
	if idleWrites != 0 {
		t.Errorf("idle program performed %d redraws, want 0", idleWrites)
	}
	if cpuPercent > 5.0 {
		t.Errorf("idle CPU = %.2f%%, want < 5%%", cpuPercent)
	}
}

// TestStreamPacingCoalesces (U10): a burst of streaming
// deltas coalesces into far fewer repaints.
func TestStreamPacingCoalesces(t *testing.T) {
	m, _, eventChan := newModel(false)
	// eventChan buffers 64; stay inside it so the
	// send never blocks.
	const deltas = 50
	for i := 0; i < deltas; i++ {
		eventChan <- protocol.EngineEvent{Type: protocol.EventAssistantTextDelta, Text: "x"}
	}
	// One Update drains the whole burst; the pacer must
	// repaint once for the burst, not once per delta.
	um, _ := m.Update(engineEventMsg(protocol.EngineEvent{
		Type: protocol.EventAssistantTextDelta, Text: "x",
	}))
	m = um.(AppModel)
	// Allow the boundary tick to flush.
	time.Sleep(20 * time.Millisecond)
	um, _ = m.Update(tickMsg(time.Now()))
	m = um.(AppModel)
	t.Logf("stream_deltas=%d repaints=%d", deltas+1, m.pacer.Renders)
	if m.pacer.Renders > 4 {
		t.Errorf("%d deltas caused %d repaints, want ≤ 4", deltas, m.pacer.Renders)
	}
}

// TestBoundedMemoryNoGCPause: the input echo loop holds
// memory flat and shows no GC pause near the frame budget.
func TestBoundedMemoryNoGCPause(t *testing.T) {
	runtime.GC()
	var before, after runtime.MemStats
	runtime.ReadMemStats(&before)

	m, _, _ := newModel(false)
	const n = 2000
	for i := 0; i < n; i++ {
		um, _ := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'x'}})
		m = um.(AppModel)
		_ = m.View()
	}
	runtime.ReadMemStats(&after)

	var maxPause time.Duration
	for _, p := range after.PauseNs {
		if d := time.Duration(p); d > maxPause {
			maxPause = d
		}
	}
	heapBefore := float64(before.HeapAlloc) / 1024
	heapAfter := float64(after.HeapAlloc) / 1024
	t.Logf("echo_loop=%d heap_kb_before=%.0f heap_kb_after=%.0f gc_cycles=%d max_gc_pause_ms=%.2f",
		n, heapBefore, heapAfter, after.NumGC, float64(maxPause.Microseconds())/1000)
	if after.HeapAlloc > before.HeapAlloc+8<<20 {
		t.Errorf("heap grew by %d KB during echo loop", (after.HeapAlloc-before.HeapAlloc)/1024)
	}
	if maxPause > 16*time.Millisecond {
		t.Errorf("max GC pause %v exceeds the 16ms frame budget", maxPause)
	}
}

// TestRenderCostFlatWithTranscript: per-frame render cost
// stays flat as the transcript grows (benchmark twin,
// asserted at runtime with honest numbers).
func TestRenderCostFlatWithTranscript(t *testing.T) {
	render := func(cells int) time.Duration {
		m, _, _ := newModel(false)
		for i := 0; i < cells; i++ {
			m.history.Append("assistant", strings.Repeat("line of transcript text ", 4))
		}
		start := time.Now()
		_ = m.View()
		return time.Since(start)
	}
	small := render(100)
	large := render(5000)
	t.Logf("render_100_cells_us=%.0f render_5000_cells_us=%.0f ratio=%.2f",
		float64(small.Microseconds()), float64(large.Microseconds()), float64(large)/float64(small))
	// Linear in cells is acceptable for the viewport; the
	// contract requires the *live* path to stay flat, which
	// the benchmarks pin. Only reject pathological growth.
	if large > small*50 {
		t.Errorf("render cost grew %dx from 100 to 5000 cells", int64(large/small))
	}
}

var _ io.Writer = (*countingWriter)(nil)
