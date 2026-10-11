package agent

import (
	"context"
	"time"
)

// MaxNestingDepth is the subagent nesting limit (S5): a subagent may
// not spawn a grandchild. Depth 1 = a child of root, depth 2 = a
// grandchild; depth 3 is refused. Root is depth 0.
const MaxNestingDepth = 2

// DefaultSubagentTimeout bounds one subagent's wall-clock run (S4).
// A subagent that exceeds it is cancelled and marked timed-out; it
// never blocks its parent indefinitely.
const DefaultSubagentTimeout = 5 * time.Minute

// SpawnTimed spawns a subagent with an explicit wall-clock timeout
// (S4). A non-positive timeout uses DefaultSubagentTimeout. The
// subagent's context is cancelled when the timeout elapses, the same
// way an explicit Close cancels it, so the run cannot outlive its
// budget. Token budgets are enforced separately (the existing budget
// argument); this adds the time dimension.
func (m *Manager) SpawnTimed(ctx context.Context, parentID, name, prompt, contextMode string, worktree bool, budget int, timeout time.Duration) (string, string, error) {
	if timeout <= 0 {
		timeout = DefaultSubagentTimeout
	}
	// Reuse Spawn for graph/semaphore/bookkeeping, then arm a timer
	// that cancels this specific instance when the timeout fires.
	agentID, canonPath, err := m.Spawn(ctx, parentID, name, prompt, contextMode, worktree, budget)
	if err != nil {
		return "", "", err
	}
	m.mu.RLock()
	inst, ok := m.instances[agentID]
	m.mu.RUnlock()
	if ok && inst.Cancel != nil {
		timer := time.AfterFunc(timeout, func() {
			m.mu.RLock()
			it, ok := m.instances[agentID]
			m.mu.RUnlock()
			if !ok {
				return
			}
			it.mu.Lock()
			// Only stamp a timeout outcome if the run has not already
			// finished; a completed run is left intact.
			select {
			case <-it.Done:
				it.mu.Unlock()
				return
			default:
			}
			it.Output = "Error: subagent timeout exceeded"
			it.mu.Unlock()
			it.Cancel()
			_ = m.store.UpdateStatus(agentID, StatusFailed, 0)
		})
		// Stop the timer when the run finishes normally so it cannot
		// fire (and mutate state) after completion.
		go func() {
			<-inst.Done
			timer.Stop()
		}()
	}
	return agentID, canonPath, nil
}

// SpawnRole spawns a role-scoped subagent (S1/S4): the role decides
// the worktree policy and the timeout is applied per S4. The role's
// tool allowlist is recorded on the node for later dispatch gating.
func (m *Manager) SpawnRole(ctx context.Context, parentID, name, prompt string, role Role, budget int, timeout time.Duration) (string, string, error) {
	spec, err := RoleFor(string(role))
	if err != nil {
		return "", "", err
	}
	return m.SpawnTimed(ctx, parentID, name, prompt, "none", spec.OwnWorktree, budget, timeout)
}
