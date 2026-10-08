package main

// `nikicode bench` (G4): reproducible benchmark harness. Every metric
// writes raw samples (JSON) under --out (default docs/bench/raw) plus a
// summary line; docs/BENCH.md transcribes the summaries and never
// hand-holds a number.

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/RavaniRoshan/niki/internal/bench"
	"github.com/RavaniRoshan/niki/internal/engine"
	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/skills"
	"github.com/RavaniRoshan/niki/internal/tools"
)

type benchRow struct {
	Metric string             `json:"metric"`
	Binary string             `json:"binary"`
	Unit   string             `json:"unit"`
	N      int                `json:"n"`
	Stats  bench.Stats        `json:"stats"`
	Extra  map[string]float64 `json:"extra,omitempty"`
}

// benchHome overrides HOME for PTY runs (references needing auth run
// with their real HOME; default is a fresh temp dir per run).
var benchHome string

func writeBenchRaw(outDir, metric string, rows []benchRow, samples map[string][]float64) (string, error) {
	if err := os.MkdirAll(outDir, 0o755); err != nil {
		return "", err
	}
	stamp := time.Now().UTC().Format("20060102-150405")
	path := filepath.Join(outDir, fmt.Sprintf("%s-%s.json", metric, stamp))
	payload := map[string]any{"rows": rows, "samples": samples}
	data, err := json.MarshalIndent(payload, "", "  ")
	if err != nil {
		return "", err
	}
	if err := os.WriteFile(path, data, 0o644); err != nil {
		return "", err
	}
	return path, nil
}

func benchVersion(bin string, n int) (benchRow, []float64, error) {
	vals, err := bench.RunVersion(bin, []string{"--version"}, n)
	if err != nil {
		return benchRow{}, nil, err
	}
	st, err := bench.Summarize(vals)
	if err != nil {
		return benchRow{}, nil, err
	}
	return benchRow{Metric: "version", Binary: bin, Unit: "ms", N: n, Stats: st}, vals, nil
}

func benchTTFF(bin, keyword string, n int, idle time.Duration) ([]benchRow, map[string][]float64, error) {
	var paint, header, rss []float64
	var idleBytes float64
	for i := 0; i < n; i++ {
		home := benchHome
		var cleanup func()
		if home == "" {
			var err error
			home, err = os.MkdirTemp("", "bench-home-")
			if err != nil {
				return nil, nil, err
			}
			cleanup = func() { os.RemoveAll(home) }
		}
		res, err := bench.RunPTY(bin, nil, keyword, home, idle)
		if cleanup != nil {
			cleanup()
		}
		if err != nil {
			return nil, nil, fmt.Errorf("run %d: %w", i, err)
		}
		paint = append(paint, res.FirstPaintMs)
		if res.HeaderMs >= 0 {
			header = append(header, res.HeaderMs)
		}
		rss = append(rss, res.IdleRSSMB)
		idleBytes += float64(res.IdleBytes)
	}
	rows := []benchRow{}
	samples := map[string][]float64{"first_paint_ms": paint, "idle_rss_mb": rss}
	addRow := func(metric, unit string, vals []float64) error {
		st, err := bench.Summarize(vals)
		if err != nil {
			return err
		}
		rows = append(rows, benchRow{Metric: "ttff." + metric, Binary: bin, Unit: unit, N: len(vals), Stats: st})
		return nil
	}
	if err := addRow("first_paint", "ms", paint); err != nil {
		return nil, nil, err
	}
	if err := addRow("idle_rss", "MB", rss); err != nil {
		return nil, nil, err
	}
	if len(header) > 0 {
		st, _ := bench.Summarize(header)
		rows = append(rows, benchRow{Metric: "ttff.input_ready", Binary: bin, Unit: "ms", N: len(header), Stats: st})
		samples["input_ready_ms"] = header
	}
	rows = append(rows, benchRow{Metric: "ttff.idle_bytes", Binary: bin, Unit: "bytes", N: n, Stats: bench.Stats{N: n, Mean: idleBytes / float64(n)}})
	return rows, samples, nil
}

func benchEcho(bin, keyword string, keys int) (benchRow, []float64, error) {
	home, err := os.MkdirTemp("", "bench-home-")
	if err != nil {
		return benchRow{}, nil, err
	}
	defer os.RemoveAll(home)
	vals, err := bench.RunEcho(bin, nil, keyword, home, keys)
	if err != nil {
		return benchRow{}, nil, err
	}
	st, err := bench.Summarize(vals)
	if err != nil {
		return benchRow{}, nil, err
	}
	return benchRow{Metric: "echo", Binary: bin, Unit: "ms", N: len(vals), Stats: st}, vals, nil
}

func benchPeak(bin, prompt string, n int) (benchRow, []float64, error) {
	var peaks, walls []float64
	for i := 0; i < n; i++ {
		home, err := os.MkdirTemp("", "bench-home-")
		if err != nil {
			return benchRow{}, nil, err
		}
		peak, wall, err := bench.ExecPeak(bin, []string{"exec", prompt}, home)
		os.RemoveAll(home)
		if err != nil {
			return benchRow{}, nil, fmt.Errorf("run %d: %w", i, err)
		}
		peaks = append(peaks, peak)
		walls = append(walls, wall)
	}
	st, err := bench.Summarize(peaks)
	if err != nil {
		return benchRow{}, nil, err
	}
	wst, _ := bench.Summarize(walls)
	return benchRow{Metric: "peak_rss", Binary: bin, Unit: "MB", N: n, Stats: st, Extra: map[string]float64{"exec_wall_p50_ms": wst.P50}}, peaks, nil
}

func benchFootprint(bin string) (benchRow, error) {
	size := bench.FileBytes(bin)
	if size < 0 {
		return benchRow{}, fmt.Errorf("cannot stat %s", bin)
	}
	mb := float64(size) / (1024 * 1024)
	return benchRow{Metric: "footprint", Binary: bin, Unit: "MB", N: 1, Stats: bench.Stats{N: 1, Min: mb, Max: mb, Mean: mb, P50: mb, P95: mb}}, nil
}

func benchSkillsWarm() (benchRow, []float64, error) {
	var cold, warm []float64
	for i := 0; i < 5; i++ {
		start := time.Now()
		found, err := skills.Discover(".")
		if err != nil {
			return benchRow{}, nil, err
		}
		cold = append(cold, float64(time.Since(start).Microseconds())/1000)
		_ = found
		start = time.Now()
		_, err = skills.Discover(".")
		if err != nil {
			return benchRow{}, nil, err
		}
		warm = append(warm, float64(time.Since(start).Microseconds())/1000)
	}
	st, _ := bench.Summarize(cold)
	wst, _ := bench.Summarize(warm)
	return benchRow{Metric: "skills_discover", Binary: "in-process", Unit: "ms", N: 5, Stats: st, Extra: map[string]float64{"warm_p50_ms": wst.P50}}, cold, nil
}

// benchTurnOverhead times mock-provider turns: harness cost excluding
// model time (the mock responds instantly).
func benchTurnOverhead(n int) (benchRow, []float64, error) {
	reg := tools.DefaultRegistry()
	guard := permissions.NewGuard(permissions.ModeFullAccess)
	var vals []float64
	for i := 0; i < n; i++ {
		eng, cmdChan, eventChan := engine.NewEngine(100, provider.NewMockProvider(), reg, guard)
		go func() { _ = eng.Run() }()
		start := time.Now()
		cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "hi"}
		deadline := time.After(30 * time.Second)
	loop:
		for {
			select {
			case evt := <-eventChan:
				if evt.Type == protocol.EventTurnCompleted {
					break loop
				}
			case <-deadline:
				eng.Stop()
				return benchRow{}, nil, fmt.Errorf("turn %d timed out", i)
			}
		}
		vals = append(vals, float64(time.Since(start).Microseconds())/1000)
		eng.Stop()
	}
	st, err := bench.Summarize(vals)
	if err != nil {
		return benchRow{}, nil, err
	}
	return benchRow{Metric: "turn_overhead", Binary: "in-process", Unit: "ms", N: n, Stats: st}, vals, nil
}
