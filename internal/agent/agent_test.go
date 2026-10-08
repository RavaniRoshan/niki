package agent

import (
	"context"
	"os"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

func TestAgentGraphStore(t *testing.T) {
	store := NewMemoryGraphStore()

	node1 := &AgentNode{
		ID:            "a1",
		CanonicalPath: "/root/researcher",
		ParentID:      "root",
		Name:          "researcher",
		Status:        StatusActive,
		Depth:         1,
	}

	if err := store.AddNode(node1); err != nil {
		t.Fatalf("failed adding node: %v", err)
	}

	// Duplicate add must fail
	if err := store.AddNode(node1); err == nil {
		t.Fatalf("expected error on duplicate node add")
	}

	got, ok := store.GetNode("a1")
	if !ok || got.Name != "researcher" {
		t.Fatalf("unexpected node from store: %v", got)
	}

	// Update status
	if err := store.UpdateStatus("a1", StatusCompleted, 150); err != nil {
		t.Fatalf("failed updating status: %v", err)
	}
	got, _ = store.GetNode("a1")
	if got.Status != StatusCompleted || got.TokensUsed != 150 {
		t.Fatalf("status update failed: %v", got)
	}

	// Close node
	if err := store.CloseNode("a1"); err != nil {
		t.Fatalf("failed closing node: %v", err)
	}
	got, _ = store.GetNode("a1")
	if got.Status != StatusClosed {
		t.Fatalf("expected closed status: %v", got)
	}

	// Canonical path computation
	p := ComputeCanonicalPath("/root/worker-1", "analyzer")
	if p != "/root/worker-1/analyzer" {
		t.Fatalf("unexpected canonical path: %s", p)
	}
}

func TestSubagentLifecycleAndEvents(t *testing.T) {
	var events []protocol.EngineEvent
	var mu sync.Mutex
	emitter := func(evt protocol.EngineEvent) {
		mu.Lock()
		events = append(events, evt)
		mu.Unlock()
	}

	store := NewMemoryGraphStore()
	mgr := NewManager(store, 3, 6, emitter)

	// 1. Spawn root subagent
	id, canonPath, err := mgr.Spawn(context.Background(), "root", "worker", "perform task", "none", false, 1000)
	if err != nil {
		t.Fatalf("spawn failed: %v", err)
	}
	if canonPath != "/root/worker" {
		t.Fatalf("unexpected canonical path: %s", canonPath)
	}

	// 2. Send input
	sendRes, err := mgr.SendInput(context.Background(), id, "more details")
	if err != nil {
		t.Fatalf("send input failed: %v", err)
	}
	if !strings.Contains(sendRes, id) {
		t.Fatalf("unexpected send input result: %s", sendRes)
	}

	// 3. Wait for completion
	status, output, tokens, err := mgr.Wait(context.Background(), id, 2*time.Second)
	if err != nil {
		t.Fatalf("wait failed: %v", err)
	}
	if status != string(StatusCompleted) {
		t.Fatalf("expected completed status, got %s", status)
	}
	if !strings.Contains(output, "Acknowledged and completed") {
		t.Fatalf("unexpected output: %s", output)
	}
	if tokens <= 0 {
		t.Fatalf("expected non-zero token usage: %d", tokens)
	}

	// 4. Close agent
	if err := mgr.Close(context.Background(), id); err != nil {
		t.Fatalf("close failed: %v", err)
	}
	node, _ := store.GetNode(id)
	if node.Status != StatusClosed {
		t.Fatalf("node should be closed, got %s", node.Status)
	}

	// Verify paired lifecycle events: EventSubagentStarted & EventSubagentCompleted
	mu.Lock()
	defer mu.Unlock()
	hasStarted := false
	hasCompleted := false
	for _, e := range events {
		if e.Type == protocol.EventSubagentStarted && strings.Contains(e.Text, id) {
			hasStarted = true
		}
		if e.Type == protocol.EventSubagentCompleted && strings.Contains(e.Text, id) {
			hasCompleted = true
		}
	}
	if !hasStarted || !hasCompleted {
		t.Fatalf("expected paired subagent events (started: %v, completed: %v)", hasStarted, hasCompleted)
	}
}

func TestSubagentRunawayControls(t *testing.T) {
	store := NewMemoryGraphStore()
	// Depth limit = 2, Concurrency limit = 2
	mgr := NewManager(store, 2, 2, nil)

	// 1. Depth limit test: depth 1 ok, depth 2 ok, depth 3 denied
	id1, _, err := mgr.Spawn(context.Background(), "root", "level1", "task 1", "none", false, 0)
	if err != nil {
		t.Fatalf("level 1 spawn failed: %v", err)
	}
	id2, _, err := mgr.Spawn(context.Background(), id1, "level2", "task 2", "none", false, 0)
	if err != nil {
		t.Fatalf("level 2 spawn failed: %v", err)
	}
	_, _, err = mgr.Spawn(context.Background(), id2, "level3", "task 3", "none", false, 0)
	if err != ErrDepthLimit {
		t.Fatalf("expected ErrDepthLimit, got %v", err)
	}

	// Wait for agents to free semaphore
	_, _, _, _ = mgr.Wait(context.Background(), id1, time.Second)
	_, _, _, _ = mgr.Wait(context.Background(), id2, time.Second)

	// 2. Concurrency limit test: limit is 2
	cStore := NewMemoryGraphStore()
	cMgr := NewManager(cStore, 5, 2, nil)

	cid1, _, err := cMgr.Spawn(context.Background(), "root", "c1", "task", "none", false, 0)
	if err != nil {
		t.Fatalf("c1 failed: %v", err)
	}
	cid2, _, err := cMgr.Spawn(context.Background(), "root", "c2", "task", "none", false, 0)
	if err != nil {
		t.Fatalf("c2 failed: %v", err)
	}

	// Third concurrent spawn must fail with ErrConcurrencyLimit
	_, _, err = cMgr.Spawn(context.Background(), "root", "c3", "task", "none", false, 0)
	if err != ErrConcurrencyLimit {
		t.Fatalf("expected ErrConcurrencyLimit, got %v", err)
	}

	_, _, _, _ = cMgr.Wait(context.Background(), cid1, time.Second)
	_, _, _, _ = cMgr.Wait(context.Background(), cid2, time.Second)

	// 3. Delegation allowlist
	aStore := NewMemoryGraphStore()
	aMgr := NewManager(aStore, 3, 5, nil)
	aMgr.SetAllowlist([]string{"allowed_agent"})

	_, _, err = aMgr.Spawn(context.Background(), "root", "disallowed", "task", "none", false, 0)
	if err != ErrDelegationDenied {
		t.Fatalf("expected ErrDelegationDenied, got %v", err)
	}

	aid, _, err := aMgr.Spawn(context.Background(), "root", "allowed_agent", "task", "none", false, 0)
	if err != nil {
		t.Fatalf("allowed agent spawn failed: %v", err)
	}
	_, _, _, _ = aMgr.Wait(context.Background(), aid, time.Second)

	// 4. Token budget control
	bStore := NewMemoryGraphStore()
	bMgr := NewManager(bStore, 3, 5, nil)
	// Budget is 5 tokens, but prompt length produces > 10 tokens
	bid, _, err := bMgr.Spawn(context.Background(), "root", "budget_test", "this prompt is long enough to exceed the tiny budget", "none", false, 5)
	if err != nil {
		t.Fatalf("budget agent spawn failed: %v", err)
	}
	status, output, _, _ := bMgr.Wait(context.Background(), bid, time.Second)
	if status != string(StatusFailed) || !strings.Contains(output, "budget exceeded") {
		t.Fatalf("expected budget failure, got status %s, output: %s", status, output)
	}
}

func TestSubagentWorktreeIsolation(t *testing.T) {
	store := NewMemoryGraphStore()
	mgr := NewManager(store, 3, 5, nil)

	id, _, err := mgr.Spawn(context.Background(), "root", "wt-worker", "task", "none", true, 0)
	if err != nil {
		t.Fatalf("worktree spawn failed: %v", err)
	}

	node, ok := store.GetNode(id)
	if !ok || node.WorktreeDir == "" {
		t.Fatalf("worktree directory was not provisioned")
	}

	// Verify worktree directory exists on disk
	if _, err := os.Stat(node.WorktreeDir); err != nil {
		t.Fatalf("worktree directory does not exist: %v", err)
	}

	wtDir := node.WorktreeDir
	// Close agent should remove temporary worktree directory
	if err := mgr.Close(context.Background(), id); err != nil {
		t.Fatalf("close failed: %v", err)
	}

	if _, err := os.Stat(wtDir); !os.IsNotExist(err) {
		t.Fatalf("worktree directory should be deleted on close")
	}
}
