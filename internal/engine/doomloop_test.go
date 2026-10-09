package engine

import (
	"testing"
)

func TestDoomLoopDetection(t *testing.T) {
	d := NewDoomLoopDetector(3)

	// Call 1 failed
	if d.Record("shell", `{"command":"cat missing.txt"}`, false) {
		t.Fatalf("call 1 should not trip doom loop")
	}

	// Call 2 failed (same args)
	if d.Record("shell", `{"command":"cat missing.txt"}`, false) {
		t.Fatalf("call 2 should not trip doom loop")
	}

	// Call 3 failed (same args) -> Trip!
	if !d.Record("shell", `{"command":"cat missing.txt"}`, false) {
		t.Fatalf("call 3 should trip doom loop")
	}

	// Reset
	d.Reset()
	if d.Record("shell", `{"command":"cat missing.txt"}`, false) {
		t.Fatalf("post-reset call 1 should not trip doom loop")
	}
}

func TestDoomLoopDifferentArgsDoesNotTrip(t *testing.T) {
	d := NewDoomLoopDetector(3)
	_ = d.Record("shell", `{"command":"cat a.txt"}`, false)
	_ = d.Record("shell", `{"command":"cat b.txt"}`, false)
	if d.Record("shell", `{"command":"cat c.txt"}`, false) {
		t.Fatalf("different arguments should not trip doom loop")
	}
}
