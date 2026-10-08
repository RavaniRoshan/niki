package main

import (
	"bytes"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
	"time"

	"github.com/creack/pty"
)

type Measurement struct {
	Command           string  `json:"command"`
	FirstPaintMs      float64 `json:"first_paint_ms"`
	InputReadyMs      float64 `json:"input_ready_ms"`
	IdleRSSMB         float64 `json:"idle_rss_mb"`
	BytesBeforePrompt int     `json:"bytes_before_prompt"`
	Error             string  `json:"error,omitempty"`
}

func readRSS(pid int) float64 {
	// Linux /proc/<pid>/statm: second field is RSS in pages
	data, err := os.ReadFile(fmt.Sprintf("/proc/%d/statm", pid))
	if err != nil {
		return 0
	}
	fields := strings.Fields(string(data))
	if len(fields) >= 2 {
		pages, err := strconv.ParseUint(fields[1], 10, 64)
		if err == nil {
			pageSize := uint64(os.Getpagesize())
			rssBytes := pages * pageSize
			return float64(rssBytes) / (1024.0 * 1024.0)
		}
	}
	return 0
}

func measure(cmdArgs []string, idleWait time.Duration) Measurement {
	m := Measurement{Command: strings.Join(cmdArgs, " ")}
	if len(cmdArgs) == 0 {
		m.Error = "no command provided"
		return m
	}

	cmd := exec.Command(cmdArgs[0], cmdArgs[1:]...)

	start := time.Now()
	f, err := pty.Start(cmd)
	if err != nil {
		m.Error = fmt.Sprintf("pty start: %v", err)
		return m
	}
	defer f.Close()

	_ = pty.Setsize(f, &pty.Winsize{Rows: 24, Cols: 80})

	var (
		firstPaintDuration time.Duration
		inputReadyDuration time.Duration
		bytesBeforePrompt  int
		sawFirstPaint      bool
		sawInputReady      bool
		buf                = make([]byte, 8192)
		accumulated        bytes.Buffer
	)

	readDone := make(chan struct{})

	go func() {
		defer close(readDone)
		for {
			n, err := f.Read(buf)
			now := time.Now()
			if n > 0 {
				chunk := buf[:n]
				if !sawFirstPaint {
					sawFirstPaint = true
					firstPaintDuration = now.Sub(start)
				}
				if !sawInputReady {
					bytesBeforePrompt += n
					accumulated.Write(chunk)
					// Heuristic for interactive prompt:
					// prompt marker like ">", or cursor placement escape like "\x1b[?25h", or clear screen
					text := accumulated.String()
					if strings.Contains(text, ">") || strings.Contains(text, "?") || strings.Contains(text, "\x1b[H") || strings.Contains(text, "Niki") || strings.Contains(text, "Codex") {
						sawInputReady = true
						inputReadyDuration = now.Sub(start)
					}
				}
			}
			if err != nil {
				return
			}
		}
	}()

	// Wait for prompt or timeout up to 3s
	deadline := time.After(3 * time.Second)
waitLoop:
	for {
		if sawInputReady {
			break waitLoop
		}
		select {
		case <-deadline:
			break waitLoop
		case <-time.After(50 * time.Millisecond):
		}
	}

	if !sawFirstPaint && sawInputReady {
		firstPaintDuration = inputReadyDuration
	}
	if !sawInputReady && sawFirstPaint {
		inputReadyDuration = firstPaintDuration
	}

	// Now wait idleWait for RSS measurement
	time.Sleep(idleWait)
	if cmd.Process != nil {
		m.IdleRSSMB = readRSS(cmd.Process.Pid)
	}

	m.FirstPaintMs = float64(firstPaintDuration.Microseconds()) / 1000.0
	m.InputReadyMs = float64(inputReadyDuration.Microseconds()) / 1000.0
	m.BytesBeforePrompt = bytesBeforePrompt

	// Clean terminate: send SIGINT (Ctrl+C) then SIGKILL
	if cmd.Process != nil {
		_ = cmd.Process.Signal(syscall.SIGINT)
		time.Sleep(100 * time.Millisecond)
		_ = cmd.Process.Kill()
		_ = cmd.Wait()
	}

	return m
}

func main() {
	jsonOut := flag.Bool("json", false, "Output results as JSON")
	idleSec := flag.Float64("idle", 2.0, "Idle wait in seconds before RSS measurement")
	flag.Parse()

	args := flag.Args()
	if len(args) == 0 {
		fmt.Fprintf(os.Stderr, "Usage: %s [flags] -- <command> [args...]\n", filepath.Base(os.Args[0]))
		os.Exit(1)
	}

	idleDuration := time.Duration(*idleSec * float64(time.Second))
	res := measure(args, idleDuration)

	if *jsonOut {
		enc := json.NewEncoder(os.Stdout)
		enc.SetIndent("", "  ")
		_ = enc.Encode(res)
		return
	}

	fmt.Printf("%-35s TTFP: %6.1f ms  InputReady: %6.1f ms  RSS: %5.1f MB  Bytes: %5d\n",
		res.Command, res.FirstPaintMs, res.InputReadyMs, res.IdleRSSMB, res.BytesBeforePrompt)
}
