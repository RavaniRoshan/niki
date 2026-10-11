package agent

import (
	"context"
	"testing"
	"time"
)

// S4: subagents have their own timeout — a subagent that runs past its
// deadline is cancelled and marked failed, and Wait reports it.
func TestSubagentTimeout(t *testing.T) {
	mgr := NewManager(NewMemoryGraphStore(), 3, 6, nil)
	id, _, err := mgr.SpawnTimed(context.Background(), "root", "slow", "a task that runs long", "none", false, 0, 1*time.Nanosecond)
	if err != nil {
		t.Fatal(err)
	}
	status, out, _, _ := mgr.Wait(context.Background(), id, 2*time.Second)
	if status != string(StatusFailed) && status != string(StatusClosed) {
		t.Fatalf("[S4] timed-out subagent must end failed/closed, got %s (%s)", status, out)
	}
	t.Logf("[S4] subagent past its timeout -> status=%s out=%q", status, out)

	id2, _, err := mgr.SpawnTimed(context.Background(), "root", "fast", "a quick task", "none", false, 0, 5*time.Second)
	if err != nil {
		t.Fatal(err)
	}
	status2, _, _, _ := mgr.Wait(context.Background(), id2, 2*time.Second)
	if status2 != string(StatusCompleted) {
		t.Fatalf("[S4] generous timeout should complete, got %s", status2)
	}
	t.Logf("[S4] generous timeout -> completed")
}

// S5: nesting is limited to depth 2 — a depth-3 spawn is refused.
func TestNestingDepthLimitIs2(t *testing.T) {
	if MaxNestingDepth != 2 {
		t.Fatalf("[S5] MaxNestingDepth must be 2, got %d", MaxNestingDepth)
	}
	mgr := NewManager(NewMemoryGraphStore(), MaxNestingDepth, 6, nil)
	id1, _, err := mgr.Spawn(context.Background(), "root", "level1", "task", "none", false, 0)
	if err != nil {
		t.Fatalf("[S5] depth 1 must be allowed: %v", err)
	}
	id2, _, err := mgr.Spawn(context.Background(), id1, "level2", "task", "none", false, 0)
	if err != nil {
		t.Fatalf("[S5] depth 2 must be allowed: %v", err)
	}
	if _, _, err = mgr.Spawn(context.Background(), id2, "level3", "task", "none", false, 0); err != ErrDepthLimit {
		t.Fatalf("[S5] depth 3 must be refused with ErrDepthLimit, got %v", err)
	}
	_, _, _, _ = mgr.Wait(context.Background(), id1, time.Second)
	_, _, _, _ = mgr.Wait(context.Background(), id2, time.Second)
	t.Logf("[S5] depth 1 and 2 allowed, depth 3 refused (ErrDepthLimit)")
}
