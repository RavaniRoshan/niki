package bench

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"time"

	"github.com/creack/pty"
)

// ptyReader wraps a PTY master with timeout reads. Go's fd deadlines
// are not reliable on PTY masters everywhere, so reads run on their
// own goroutine and callers select with a timer. Close unblocks a
// pending read; the channel is buffered so the reader never leaks.
type ptyReader struct {
	f  *os.File
	ch chan readRes
}

type readRes struct {
	data []byte
	err  error
}

func newPTYReader(f *os.File) *ptyReader {
	r := &ptyReader{f: f, ch: make(chan readRes, 64)}
	go func() {
		buf := make([]byte, 8192)
		for {
			n, err := f.Read(buf)
			r.ch <- readRes{data: append([]byte(nil), buf[:n]...), err: err}
			if err != nil {
				return
			}
		}
	}()
	return r
}

var errReadTimeout = fmt.Errorf("read timeout")

func (r *ptyReader) readTimeout(d time.Duration) ([]byte, error) {
	select {
	case res := <-r.ch:
		return res.data, res.err
	case <-time.After(d):
		return nil, errReadTimeout
	}
}

// queryReplies answers the terminal queries TUIs send, the way a real
// terminal would, so capability detection completes immediately.
func answerQueries(chunk []byte, f *os.File) {
	for _, qr := range []struct{ q, r []byte }{
		{[]byte("\x1b[c"), []byte("\x1b[?62;22c")},
		{[]byte("\x1b[?u"), []byte("\x1b[?1u")},
		{[]byte("\x1b[?2026$p"), []byte("\x1b[?2026;1$y")},
		{[]byte("\x1b[6n"), []byte("\x1b[1;1R")},
	} {
		if bytes.Contains(chunk, qr.q) {
			_, _ = f.Write(qr.r)
		}
	}
}

// PTYResult is one interactive-launch measurement.
type PTYResult struct {
	FirstPaintMs float64
	HeaderMs     float64 // -1 when the keyword never appeared
	IdleBytes    int
	IdleRSSMB    float64
}

// RunPTY launches bin under a PTY, answers terminal queries, records
// time to first output and to keyword, then idles to count bytes and
// sample RSS. home isolates all state. The child is killed afterwards.
func RunPTY(bin string, args []string, keyword, home string, idle time.Duration) (PTYResult, error) {
	var res PTYResult
	res.HeaderMs = -1
	start := time.Now()
	cmd := exec.Command(bin, args...)
	cmd.Env = append(os.Environ(), "HOME="+home, "TERM=xterm-256color")
	f, err := pty.Start(cmd)
	if err != nil {
		return res, err
	}
	defer func() {
		_ = cmd.Process.Kill()
		_ = cmd.Wait()
		_ = f.Close()
	}()
	_ = pty.Setsize(f, &pty.Winsize{Rows: 24, Cols: 80})
	rd := newPTYReader(f)
	deadline := time.Now().Add(30 * time.Second)
	var seen bytes.Buffer
	first := false
	for time.Now().Before(deadline) {
		chunk, rerr := rd.readTimeout(500 * time.Millisecond)
		if len(chunk) > 0 {
			if !first {
				first = true
				res.FirstPaintMs = float64(time.Since(start).Microseconds()) / 1000
				if keyword == "" {
					res.HeaderMs = res.FirstPaintMs
				}
			}
			answerQueries(chunk, f)
			seen.Write(chunk)
			if res.HeaderMs < 0 && keyword != "" && bytes.Contains(seen.Bytes(), []byte(keyword)) {
				res.HeaderMs = float64(time.Since(start).Microseconds()) / 1000
			}
			if first && (keyword == "" || res.HeaderMs >= 0) {
				break
			}
		}
		if rerr != nil && rerr != errReadTimeout {
			if first && (keyword == "" || res.HeaderMs >= 0) {
				break
			}
			if !first {
				continue
			}
			break
		}
	}
	if !first {
		return res, fmt.Errorf("no output from %s within 30s", bin)
	}
	if keyword != "" && res.HeaderMs < 0 {
		return res, fmt.Errorf("keyword %q never appeared", keyword)
	}
	// Idle window: drain-then-count bytes, sample RSS mid-window.
	for {
		chunk, rerr := rd.readTimeout(200 * time.Millisecond)
		if len(chunk) > 0 {
			answerQueries(chunk, f)
			continue
		}
		if rerr != nil {
			break
		}
	}
	idleStart := time.Now()
	res.IdleRSSMB = ProcRSS(cmd.Process.Pid)
	for time.Since(idleStart) < idle {
		chunk, _ := rd.readTimeout(200 * time.Millisecond)
		if len(chunk) > 0 {
			res.IdleBytes += len(chunk)
			answerQueries(chunk, f)
		}
	}
	return res, nil
}

// RunEcho measures keystroke-to-screen latency: after first paint it
// sends beacon bytes and times each one's reappearance. Returns per-key
// samples in ms.
func RunEcho(bin string, args []string, keyword, home string, keys int) ([]float64, error) {
	cmd := exec.Command(bin, args...)
	cmd.Env = append(os.Environ(), "HOME="+home, "TERM=xterm-256color")
	f, err := pty.Start(cmd)
	if err != nil {
		return nil, err
	}
	defer func() {
		_ = cmd.Process.Kill()
		_ = cmd.Wait()
		_ = f.Close()
	}()
	_ = pty.Setsize(f, &pty.Winsize{Rows: 24, Cols: 80})
	rd := newPTYReader(f)
	deadline := time.Now().Add(30 * time.Second)
	var seen bytes.Buffer
	ready := keyword == ""
	for !ready && time.Now().Before(deadline) {
		chunk, rerr := rd.readTimeout(500 * time.Millisecond)
		if len(chunk) > 0 {
			answerQueries(chunk, f)
			seen.Write(chunk)
			if !ready && bytes.Contains(seen.Bytes(), []byte(keyword)) {
				ready = true
				break
			}
		}
		if rerr != nil && rerr != errReadTimeout && ready {
			break
		}
	}
	if !ready {
		return nil, fmt.Errorf("never ready: %q", keyword)
	}
	seen.Reset()
	var samples []float64
	for i := 0; i < keys; i++ {
		beacon := []byte{byte('a' + i%26)}
		t0 := time.Now()
		if _, err := f.Write(beacon); err != nil {
			return nil, err
		}
		found := false
		// Poll until the beacon reappears in FRESH output (echo
		// via render). Startup bytes are discarded above.
		for time.Since(t0) < 2*time.Second {
			chunk, _ := rd.readTimeout(100 * time.Millisecond)
			if len(chunk) > 0 {
				answerQueries(chunk, f)
				if bytes.Contains(chunk, beacon) {
					samples = append(samples, float64(time.Since(t0).Microseconds())/1000)
					found = true
					break
				}
			}
		}
		if !found {
			return nil, fmt.Errorf("beacon %d never echoed", i)
		}
		time.Sleep(20 * time.Millisecond)
	}
	return samples, nil
}

// ExecPeak runs a non-interactive command, sampling RSS in a tight
// loop (sub-20ms turns would outrun a ticker), returning peak MB and
// wall ms.
func ExecPeak(bin string, args []string, home string) (peakMB, wallMs float64, err error) {
	start := time.Now()
	cmd := exec.Command(bin, args...)
	cmd.Env = append(os.Environ(), "HOME="+home)
	if err := cmd.Start(); err != nil {
		return 0, 0, err
	}
	done := make(chan error, 1)
	go func() { done <- cmd.Wait() }()
	for {
		if rss := ProcRSS(cmd.Process.Pid); rss > peakMB {
			peakMB = rss
		}
		select {
		case err := <-done:
			// One last sample: the peak often lands at exit.
			if rss := ProcRSS(cmd.Process.Pid); rss > peakMB {
				peakMB = rss
			}
			if err != nil {
				return peakMB, float64(time.Since(start).Microseconds()) / 1000, err
			}
			return peakMB, float64(time.Since(start).Microseconds()) / 1000, nil
		case <-time.After(120 * time.Second):
			_ = cmd.Process.Kill()
			return peakMB, 0, fmt.Errorf("timeout")
		default:
			time.Sleep(time.Millisecond)
		}
	}
}
