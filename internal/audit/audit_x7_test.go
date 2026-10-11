package audit

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// X7: a local audit log of approvals and sandbox denials, rotated.
func TestAuditLogRecordsAndRotates(t *testing.T) {
	path := filepath.Join(t.TempDir(), "audit.jsonl")
	lg, err := Open(path)
	if err != nil {
		t.Fatal(err)
	}
	// A cap large enough to hold the first batch in the live log, small
	// enough that the filler batch forces a rotation.
	lg.SetMaxBytes(400)

	// Record a mix of approvals and sandbox denials.
	if err := lg.RecordApproval("shell", "ls -la", "allow", "safe command"); err != nil {
		t.Fatal(err)
	}
	if err := lg.RecordSandbox("shell", "curl evil.example", "denied", "network isolated"); err != nil {
		t.Fatal(err)
	}
	if err := lg.RecordApproval("write_file", "/etc/passwd", "deny", "outside workspace"); err != nil {
		t.Fatal(err)
	}

	entries, err := lg.Read()
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 3 {
		t.Fatalf("[X7] expected 3 entries, got %d", len(entries))
	}
	approvals, sandboxDenials := 0, 0
	for _, e := range entries {
		switch e.Kind {
		case KindApproval:
			approvals++
		case KindSandbox:
			sandboxDenials++
		}
	}
	if approvals != 2 || sandboxDenials != 1 {
		t.Fatalf("[X7] wrong kind mix: approvals=%d sandbox=%d", approvals, sandboxDenials)
	}
	t.Logf("[X7] recorded %d approvals + %d sandbox denials locally", approvals, sandboxDenials)

	// Rotation: fill the log exactly until the first rotation fires
	// (live log moves to .1, original entries preserved in the backup).
	rotated := false
	for i := 0; i < 50 && !rotated; i++ {
		_ = lg.RecordApproval("shell", "echo "+strings.Repeat("x", 40), "allow", "filler")
		if _, err := os.Stat(path + ".1"); err == nil {
			rotated = true
		}
	}
	if !rotated {
		t.Fatalf("[X7] log did not rotate (no .1 backup)")
	}
	// The live log is bounded after rotation (a fresh log started).
	fi, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	if fi.Size() > lg.maxBytes {
		t.Fatalf("[X7] live log not bounded after rotation: %d bytes", fi.Size())
	}
	// Read returns the rotated history (.1 = originals + some fillers)
	// plus the post-rotation live entries, oldest first.
	all, _ := lg.Read()
	foundOrig, origFirst := false, false
	for idx, e := range all {
		if e.Command == "curl evil.example" && e.Kind == KindSandbox {
			foundOrig = true
			if idx < 3 { // originals are the first three entries overall
				origFirst = true
			}
		}
	}
	if !foundOrig || !origFirst {
		t.Fatalf("[X7] rotated history lost or misordered (found=%v first=%v)", foundOrig, origFirst)
	}
	if len(all) < 3 {
		t.Fatalf("[X7] rotation lost entries: %d", len(all))
	}
	t.Logf("[X7] rotated at %d-byte cap; live log %d bytes; %d entries retained across rotation (originals in .1)",
		lg.maxBytes, fi.Size(), len(all))
}
