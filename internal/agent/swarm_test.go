package agent

import (
	"context"
	"testing"
	"time"
)

func TestRunSwarmConcurrency(t *testing.T) {
	mgr := NewManager(nil, 3, 6, nil)
	tasks := []SwarmTask{
		{ID: "t1", Prompt: "Task 1", UseWorktree: false},
		{ID: "t2", Prompt: "Task 2", UseWorktree: false},
		{ID: "t3", Prompt: "Task 3", UseWorktree: false},
	}

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	results, err := RunSwarm(ctx, mgr, tasks, 2)
	if err != nil {
		t.Fatalf("unexpected error running swarm: %v", err)
	}

	if len(results) != 3 {
		t.Fatalf("expected 3 results, got %d", len(results))
	}
	for _, res := range results {
		if res.TaskID == "" || res.AgentID == "" {
			t.Fatalf("incomplete swarm result: %+v", res)
		}
	}
}
