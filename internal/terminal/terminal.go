// Package terminal implements hand-rolled terminal capability
// negotiation and mode switching (L7): synchronized output
// (CSI ?2026h/l) and the kitty keyboard protocol (CSI >1u / <u),
// with feature detection before the TUI starts and an
// unconditional restore on every exit path.
package terminal

import (
	"fmt"
	"io"
	"os"
	"strings"
	"sync"
	"time"

	"github.com/charmbracelet/x/term"
)

// Escape sequences (L7). The disable sequences are safe to write
// even to terminals that never enabled the modes: unknown CSI
// private modes are ignored by convention.
const (
	kittyQuery       = "\x1b[?u"
	kittyEnable      = "\x1b[>1u"
	kittyDisable     = "\x1b[<u"
	syncQuery        = "\x1b[?2026$p"
	syncEnable       = "\x1b[?2026h"
	syncDisable      = "\x1b[?2026l"
	deviceAttrsQuery = "\x1b[c"

	ProgressIndeterminate  = "\x1b]9;4;3\x1b\\"
	ProgressClear          = "\x1b]9;4;0\x1b\\"
	CursorSteadyBar        = "\x1b[6 q"
	CursorBlinkingBar      = "\x1b[5 q"
	ModifyOtherKeysEnable  = "\x1b[>4;2m"
	ModifyOtherKeysDisable = "\x1b[>4;0m"
)

// SetProgress emits the OS taskbar/tab progress indicator escape (OSC 9;4).
func SetProgress(w io.Writer, active bool) {
	if active {
		_, _ = io.WriteString(w, ProgressIndeterminate)
	} else {
		_, _ = io.WriteString(w, ProgressClear)
	}
}

// SetCursorShape emits the DECSCUSR cursor shape sequence.
func SetCursorShape(w io.Writer, reducedMotion bool) {
	if reducedMotion {
		_, _ = io.WriteString(w, CursorSteadyBar)
	} else {
		_, _ = io.WriteString(w, CursorBlinkingBar)
	}
}

// SetWindowTitle sets the terminal window/tab title using OSC 0 / OSC 2.
func SetWindowTitle(title string) string {
	return fmt.Sprintf("\x1b]0;%s\x07", title)
}

// DesktopNotification emits OSC 9 and OSC 777 desktop notification escapes.
func DesktopNotification(title, message string) string {
	return fmt.Sprintf("\x1b]777;notify;%s;%s\x07\x1b]9;%s: %s\x07", title, message, title, message)
}

// SemanticPromptMark returns OSC 133 semantic prompt markers (A: prompt, B: command, C: output, D: finish).
func SemanticPromptMark(mark string) string {
	return fmt.Sprintf("\x1b]133;%s\x07", mark)
}

// Capabilities reports which negotiated modes the terminal
// actually supports.
type Capabilities struct {
	KittyKeyboard bool
	SyncOutput    bool
}

// Detect opens the controlling terminal, switches it to raw
// mode, queries for synchronized-output and kitty-keyboard
// support, and restores the terminal. A timeout bounds the
// wait for replies so a dumb terminal cannot stall startup.
func Detect(timeout time.Duration) (Capabilities, error) {
	tty, owns, err := openTTYFn()
	if err != nil {
		return Capabilities{}, err
	}
	// Only close descriptors we opened: closing the
	// process's own stdin would kill every later read
	// (bubbletea reads the same terminal).
	if owns {
		defer tty.Close()
	}

	fd := int(tty.Fd())
	if !term.IsTerminal(uintptr(fd)) {
		return Capabilities{}, fmt.Errorf("not a terminal")
	}
	oldState, err := term.MakeRaw(uintptr(fd))
	if err != nil {
		return Capabilities{}, err
	}
	defer func() { _ = term.Restore(uintptr(fd), oldState) }()

	var caps Capabilities
	// Drain anything already queued so stale bytes cannot be
	// mistaken for a fresh reply. Never waits: a poll(0)
	// read of pending data only.
	drain(tty)

	queries := deviceAttrsQuery + kittyQuery + syncQuery
	if _, err := tty.WriteString(queries); err != nil {
		return caps, err
	}

	reply := readReplies(tty, timeout)
	caps.KittyKeyboard = kittySupported(reply)
	caps.SyncOutput = syncSupported(reply)
	return caps, nil
}

// readReplies collects tty output for up to timeout.
// It waits with poll(2) in the calling goroutine: a
// timeout must never leave a reader behind, because a
// goroutine still blocked on the tty would race the
// TUI's own input reader for keystrokes after startup.
// The wait ends early once every query has been
// answered, or after a short quiet window following
// the first byte — a terminal that does not answer
// every query must not burn the whole budget.
func readReplies(tty *os.File, timeout time.Duration) []byte {
	var reply []byte
	buf := make([]byte, 512)
	deadline := time.Now().Add(timeout)
	for {
		remaining := time.Until(deadline)
		if remaining <= 0 {
			return reply
		}
		wait := remaining
		if len(reply) > 0 {
			if quiet := 25 * time.Millisecond; quiet < remaining {
				wait = quiet
			}
		}
		if !pollRead(tty, wait) {
			return reply
		}
		n, err := tty.Read(buf)
		if n > 0 {
			reply = append(reply, buf[:n]...)
			if repliesComplete(reply) {
				return reply
			}
		}
		if err != nil {
			return reply
		}
	}
}

// repliesComplete reports whether the reply stream
// already answers all three queries: the
// device-attributes report (final byte c), the kitty
// keyboard reply (u) and the DECRQM reply ($y).
func repliesComplete(reply []byte) bool {
	var sawAttrs, sawKitty, sawSync bool
	for _, seq := range splitCSI(reply) {
		switch {
		case strings.HasSuffix(seq, "c"):
			sawAttrs = true
		case strings.HasPrefix(seq, "\x1b[?") && strings.HasSuffix(seq, "u"):
			sawKitty = true
		case strings.HasSuffix(seq, "$y"):
			sawSync = true
		}
	}
	return sawAttrs && sawKitty && sawSync
}

// kittySupported parses CSI ? flags u replies: any nonzero
// flags value means the protocol is available; ?0u means not.
func kittySupported(reply []byte) bool {
	for _, seq := range splitCSI(reply) {
		if !strings.HasPrefix(seq, "\x1b[?") || !strings.HasSuffix(seq, "u") {
			continue
		}
		flags := strings.TrimSuffix(strings.TrimPrefix(seq, "\x1b[?"), "u")
		if flags == "0" || flags == "" {
			return false
		}
		return true
	}
	return false
}

// syncSupported parses DECRQM replies for mode 2026:
// CSI ? 2026 ; mode $ y where mode 0 means unrecognized and
// 1 (set), 2 (reset) or 3 (permanently set) mean supported.
func syncSupported(reply []byte) bool {
	for _, seq := range splitCSI(reply) {
		if !strings.HasPrefix(seq, "\x1b[?2026;") || !strings.HasSuffix(seq, "$y") {
			continue
		}
		mode := strings.TrimSuffix(strings.TrimPrefix(seq, "\x1b[?2026;"), "$y")
		return mode != "0"
	}
	return false
}

// splitCSI splits a raw reply stream into CSI sequences,
// dropping intervening garbage. A CSI sequence is
// ESC [ parameter-bytes final-byte, where the final byte
// lies in 0x40–0x7E.
func splitCSI(reply []byte) []string {
	var seqs []string
	var cur []byte
	inSeq := false
	sawBracket := false
	for _, b := range reply {
		if !inSeq {
			if b == 0x1b {
				cur = []byte{b}
				inSeq = true
				sawBracket = false
			}
			continue
		}
		cur = append(cur, b)
		if !sawBracket {
			sawBracket = b == '['
			continue
		}
		if b >= 0x40 && b <= 0x7e {
			seqs = append(seqs, string(cur))
			inSeq = false
		}
	}
	return seqs
}

// Enable activates the negotiated modes on w.
func Enable(caps Capabilities, w io.Writer) {
	if caps.KittyKeyboard {
		_, _ = io.WriteString(w, kittyEnable)
	}
	if caps.SyncOutput {
		_, _ = io.WriteString(w, syncEnable)
	}
}

// Disable restores both modes. It writes unconditionally —
// unknown modes are ignored by terminals that never enabled
// them — so it is safe on every exit path.
func Disable(w io.Writer) {
	_, _ = io.WriteString(w, kittyDisable)
	_, _ = io.WriteString(w, syncDisable)
}

// NewSyncWriter returns a writer that brackets every
// Write with synchronized-output mode when the output
// is a terminal that supports it. Terminal files are
// wrapped in a writer that also conforms to
// term.File, so the TUI framework keeps querying the
// real descriptor for window size and terminal state.
// Any other output is returned unchanged: synchronized
// sequences would only corrupt a non-terminal stream.
func NewSyncWriter(w io.Writer, supported bool) io.Writer {
	f, ok := w.(*os.File)
	if !ok || !term.IsTerminal(f.Fd()) {
		return w
	}
	return &syncTTYWriter{w: w, file: f, supported: supported}
}

// syncTTYWriter brackets writes with synchronized
// output and preserves term.File conformance.
type syncTTYWriter struct {
	w         io.Writer
	file      *os.File
	supported bool
	mu        sync.Mutex
}

func (s *syncTTYWriter) Write(p []byte) (int, error) {
	if !s.supported {
		return s.w.Write(p)
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if _, err := io.WriteString(s.w, syncEnable); err != nil {
		return 0, err
	}
	n, err := s.w.Write(p)
	if _, werr := io.WriteString(s.w, syncDisable); werr != nil && err == nil {
		return n, werr
	}
	return n, err
}

// Read, Close and Fd satisfy term.File. Close is a
// no-op: the descriptor belongs to the process's own
// standard streams and must outlive the program.
func (s *syncTTYWriter) Read(p []byte) (int, error) { return s.file.Read(p) }
func (s *syncTTYWriter) Close() error               { return nil }
func (s *syncTTYWriter) Fd() uintptr                { return s.file.Fd() }

var openTTYFn = openTTY

// openTTY returns the controlling terminal. It prefers
// /dev/tty so detection works even when stdin is
// redirected, and never returns a descriptor the caller
// must not close: owns reports whether the caller should
// close the file.
func openTTY() (*os.File, bool, error) {
	// O_RDWR: detection writes queries to the terminal, so a
	// read-only descriptor would fail every write with EBADF
	// and silently disable all capability detection.
	if f, err := os.OpenFile("/dev/tty", os.O_RDWR, 0); err == nil {
		return f, true, nil
	}
	if term.IsTerminal(os.Stdin.Fd()) {
		return os.Stdin, false, nil
	}
	return nil, false, fmt.Errorf("no controlling terminal")
}
