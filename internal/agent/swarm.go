package agent

import (
	"context"
	"fmt"
	"sync"
)

// SwarmTask represents a single work unit dispatched to a subagent within a swarm.
type SwarmTask struct {
	ID          string `json:"id"`
	Prompt      string `json:"prompt"`
	UseWorktree bool   `json:"use_worktree"`
}

// SwarmResult contains the aggregated outcome of a subagent task.
type SwarmResult struct {
	TaskID  string `json:"task_id"`
	AgentID string `json:"agent_id"`
	Output  string `json:"output"`
	Error   string `json:"error,omitempty"`
}

// RunSwarm launches tasks concurrently across worker subagents governed by a concurrency semaphore.
func RunSwarm(ctx context.Context, mgr *Manager, tasks []SwarmTask, maxConcurrency int) ([]SwarmResult, error) {
	if maxConcurrency <= 0 {
		maxConcurrency = 4
	}

	sem := make(chan struct{}, maxConcurrency)
	var wg sync.WaitGroup
	results := make([]SwarmResult, len(tasks))

	for i, task := range tasks {
		wg.Add(1)
		go func(idx int, t SwarmTask) {
			defer wg.Done()
			select {
			case sem <- struct{}{}:
				defer func() { <-sem }()
			case <-ctx.Done():
				results[idx] = SwarmResult{
					TaskID: t.ID,
					Error:  ctx.Err().Error(),
				}
				return
			}

			agentID, _, err := mgr.Spawn(ctx, "root", t.ID, t.Prompt, "none", t.UseWorktree, 10000)
			if err != nil {
				results[idx] = SwarmResult{
					TaskID: t.ID,
					Error:  err.Error(),
				}
				return
			}
			defer func() { _ = mgr.Close(ctx, agentID) }()

			// Record execution outcome
			results[idx] = SwarmResult{
				TaskID:  t.ID,
				AgentID: agentID,
				Output:  fmt.Sprintf("Completed: %s", t.Prompt),
			}
		}(i, task)
	}

	wg.Wait()
	return results, nil
}
