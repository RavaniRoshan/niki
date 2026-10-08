package terminal

import (
	"bytes"
	"io"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/charmbracelet/x/term"
)

func TestKittySupportedParsesReplies(t *testing.T) {
	cases := []struct {
		name  string
		reply string
		want  bool
	}{
		{"kitty flags 1", "\x1b[?1u", true},
		{"kitty flags 13", "\x1b[?13u", true},
		{"kitty unsupported", "\x1b[?0u", false},
		{"no kitty reply", "\x1b[?2026;1$y", false},
		{"empty", "", false},
	}
	for _, tc := range cases {
		if got := kittySupported([]byte(tc.reply)); got != tc.want {
			t.Errorf("%s: kittySupported(%q) = %v, want %v", tc.name, tc.reply, got, tc.want)
		}
	}
}

func TestSyncSupportedParsesDecrqm(t *testing.T) {
	cases := []struct {
		name  string
		reply string
		want  bool
	}{
		{"mode set", "\x1b[?2026;1$y", true},
		{"mode reset", "\x1b[?2026;2$y", true},
		{"mode permanent", "\x1b[?2026;3$y", true},
		{"unrecognized", "\x1b[?2026;0$y", false},
		{"other mode", "\x1b[?2027;1$y", false},
		{"empty", "", false},
	}
	for _, tc := range cases {
		if got := syncSupported([]byte(tc.reply)); got != tc.want {
			t.Errorf("%s: syncSupported(%q) = %v, want %v", tc.name, tc.reply, got, tc.want)
		}
	}
}

func TestSplitCSIDropsGarbage(t *testing.T) {
	raw := "garbage\x1b[?1u\x1b[?2026;1$ymore\x1b[?0u"
	seqs := splitCSI([]byte(raw))
	want := []string{"\x1b[?1u", "\x1b[?2026;1$y", "\x1b[?0u"}
	if len(seqs) != len(want) {
		t.Fatalf("splitCSI = %q, want %q", seqs, want)
	}
	for i := range want {
		if seqs[i] != want[i] {
			t.Errorf("seq %d = %q, want %q", i, seqs[i], want[i])
		}
	}
}

func TestEnableWritesOnlySupportedModes(t *testing.T) {
	var buf bytes.Buffer
	Enable(Capabilities{KittyKeyboard: true, SyncOutput: false}, &buf)
	if got := buf.String(); got != kittyEnable {
		t.Errorf("Enable(kitty only) wrote %q, want %q", got, kittyEnable)
	}
	buf.Reset()
	Enable(Capabilities{}, &buf)
	if buf.Len() != 0 {
		t.Errorf("Enable(none) wrote %q, want nothing", buf.String())
	}
}

// TestDisableAlwaysWritesRestoreSequences pins the restore
// contract: both pop sequences are written unconditionally so
// every exit path restores the terminal (L7).
func TestDisableAlwaysWritesRestoreSequences(t *testing.T) {
	var buf bytes.Buffer
	Disable(&buf)
	want := kittyDisable + syncDisable
	if got := buf.String(); got != want {
		t.Errorf("Disable wrote %q, want %q", got, want)
	}
}

func TestSyncWriterBracketsWrites(t *testing.T) {
	var buf bytes.Buffer
	// A terminal file wrap: the file is only consulted
	// for term.File conformance, never written to here.
	w := &syncTTYWriter{w: &buf, file: os.Stdout, supported: true}
	if _, err := w.Write([]byte("frame")); err != nil {
		t.Fatal(err)
	}
	want := syncEnable + "frame" + syncDisable
	if got := buf.String(); got != want {
		t.Errorf("SyncWriter output %q, want %q", got, want)
	}
}

func TestSyncWriterPassesThroughWhenUnsupported(t *testing.T) {
	var buf bytes.Buffer
	w := &syncTTYWriter{w: &buf, file: os.Stdout, supported: false}
	if _, err := w.Write([]byte("frame")); err != nil {
		t.Fatal(err)
	}
	if got := buf.String(); got != "frame" {
		t.Errorf("unsupported SyncWriter output %q, want plain frame", got)
	}
}

// TestNewSyncWritesOnlyWrapTerminals asserts the wrapper
// is applied to terminal files only: a plain buffer
// passes through untouched.
func TestNewSyncWritesOnlyWrapTerminals(t *testing.T) {
	var buf bytes.Buffer
	if got := NewSyncWriter(&buf, true); got != io.Writer(&buf) {
		t.Errorf("non-terminal writer was wrapped: %T", got)
	}
}

// TestSyncWriterIsTermFile asserts the wrapper keeps
// term.File conformance so the TUI can query the real
// descriptor for size and state.
func TestSyncWriterIsTermFile(t *testing.T) {
	w := &syncTTYWriter{w: os.Stdout, file: os.Stdout, supported: false}
	if _, ok := interface{}(w).(term.File); !ok {
		t.Fatalf("terminal output lost term.File conformance: %T", w)
	}
}

// TestDetectOnNonTerminal asserts detection fails closed and
// quickly when there is no controlling terminal.
func TestDetectOnNonTerminal(t *testing.T) {
	done := make(chan struct{})
	var err error
	go func() {
		defer close(done)
		_, err = Detect(50 * time.Millisecond)
	}()
	select {
	case <-done:
	case <-time.After(3 * time.Second):
		t.Fatal("Detect hung without a terminal")
	}
	if err == nil {
		t.Error("Detect on non-terminal should return an error")
	}
}

// TestConstValues pins the exact escape sequences so a refactor
// cannot silently change the wire format.
func TestConstValues(t *testing.T) {
	if kittyEnable != "\x1b[>1u" || kittyDisable != "\x1b[<u" {
		t.Errorf("kitty sequences drifted: %q %q", kittyEnable, kittyDisable)
	}
	if syncEnable != "\x1b[?2026h" || syncDisable != "\x1b[?2026l" {
		t.Errorf("sync sequences drifted: %q %q", syncEnable, syncDisable)
	}
	if !strings.HasPrefix(kittyQuery, "\x1b[?") {
		t.Errorf("kitty query malformed: %q", kittyQuery)
	}
}
