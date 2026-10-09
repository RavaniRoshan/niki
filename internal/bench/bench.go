// Package bench is the reproducible benchmark harness behind
// `nikicode bench` (G4): stdlib only (os/exec, time), CSV/JSON output,
// no framework. Every number in docs/BENCH.md is reproducible from a
// stored command under docs/bench/raw/.
package bench

import (
	"fmt"
	"os"
	"os/exec"
	"sort"
	"time"
)

// Sample is one measurement in milliseconds (or MB, or bytes — the
// metric defines the unit; stats are unit-agnostic).
type Sample struct {
	Value float64
}

// Stats summarizes N samples.
type Stats struct {
	N    int
	Min  float64
	Max  float64
	Mean float64
	P50  float64
	P95  float64
}

// Summarize sorts a copy and reports min/max/mean/p50/p95. Empty input
// is an error, never a zero row.
func Summarize(values []float64) (Stats, error) {
	if len(values) == 0 {
		return Stats{}, fmt.Errorf("no samples")
	}
	sorted := append([]float64(nil), values...)
	sort.Float64s(sorted)
	sum := 0.0
	for _, v := range sorted {
		sum += v
	}
	pick := func(p float64) float64 {
		i := int(p * float64(len(sorted)-1))
		return sorted[i]
	}
	return Stats{
		N: len(sorted), Min: sorted[0], Max: sorted[len(sorted)-1],
		Mean: sum / float64(len(sorted)), P50: pick(0.50), P95: pick(0.95),
	}, nil
}

// RunVersion times `--version` N times (fresh process every run).
func RunVersion(bin string, args []string, n int) ([]float64, error) {
	var out []float64
	for i := 0; i < n; i++ {
		start := time.Now()
		cmd := exec.Command(bin, args...)
		cmd.Env = os.Environ()
		if err := cmd.Run(); err != nil {
			return nil, fmt.Errorf("run %d: %w", i, err)
		}
		out = append(out, float64(time.Since(start).Microseconds())/1000)
	}
	return out, nil
}

// ProcRSS reads VmRSS (MB) for a pid from /proc. 0 with no error when
// unavailable (non-Linux); callers record the platform.
func ProcRSS(pid int) float64 {
	data, err := os.ReadFile(fmt.Sprintf("/proc/%d/status", pid))
	if err != nil {
		return 0
	}
	for _, line := range splitLines(string(data)) {
		var kb int64
		if _, err := fmt.Sscanf(line, "VmRSS: %d kB", &kb); err == nil {
			return float64(kb) / 1024
		}
	}
	return 0
}

func splitLines(s string) []string {
	var out []string
	start := 0
	for i := 0; i < len(s); i++ {
		if s[i] == '\n' {
			out = append(out, s[start:i])
			start = i + 1
		}
	}
	out = append(out, s[start:])
	return out
}

// FileBytes returns the size of a binary in bytes, or -1.
func FileBytes(path string) int64 {
	st, err := os.Stat(path)
	if err != nil {
		return -1
	}
	return st.Size()
}
