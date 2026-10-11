// Command perfbudget measures NikiCode's boot-time and binary-size budgets
// against a recorded baseline, failing the build when a regression exceeds the
// foundation-pack tolerance (Q3). It is the CI-facing measurement layer; the
// regression math lives in internal/perf.
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"time"

	"github.com/RavaniRoshan/niki/internal/perf"
)

type baseline struct {
	VersionMS float64 `json:"version_ms"`
	BinaryMB  float64 `json:"binary_mb"`
	Tolerance float64 `json:"tolerance"`
}

func main() {
	bin := flag.String("bin", "", "path to the nikicode binary (defaults to building one)")
	base := flag.String("baseline", "tools/perfbudget/baseline.json", "baseline budget file")
	runs := flag.Int("runs", 9, "how many --version timings to take (p50 reported)")
	flag.Parse()

	b, err := readBaseline(*base)
	if err != nil {
		fmt.Fprintf(os.Stderr, "[Q3] cannot read baseline: %v\n", err)
		os.Exit(2)
	}

	binaryPath, cleanup, err := resolveBinary(*bin)
	if err != nil {
		fmt.Fprintf(os.Stderr, "[Q3] build binary: %v\n", err)
		os.Exit(2)
	}
	if cleanup != nil {
		defer cleanup()
	}

	versionMS := measureVersion(binaryPath, *runs)
	binaryMB := binarySizeMB(tStat(binaryPath))

	budgets := []perf.Budget{
		{Name: "niki_version_ms", Baseline: b.VersionMS, Current: versionMS, Tolerance: b.Tolerance},
		{Name: "binary_mb", Baseline: b.BinaryMB, Current: binaryMB, Limit: 25.0, Tolerance: b.Tolerance},
	}

	report(budgets)
	violations := perf.Check(budgets)
	if len(violations) > 0 {
		fmt.Fprintln(os.Stderr, "[Q3] performance budget FAILED")
		sort.Slice(violations, func(i, j int) bool { return violations[i].Name < violations[j].Name })
		for _, v := range violations {
			fmt.Fprintln(os.Stderr, "  "+v.Error())
		}
		os.Exit(1)
	}
	fmt.Println("[Q3] all performance budgets green")
}

func report(budgets []perf.Budget) {
	fmt.Println("[Q3] measured budgets:")
	for _, b := range budgets {
		fmt.Printf("      %s: current=%.3f baseline=%.3f limit=%.1f\n", b.Name, b.Current, b.Baseline, b.Limit)
	}
}

func readBaseline(path string) (baseline, error) {
	var b baseline
	f, err := os.Open(path)
	if err != nil {
		return b, err
	}
	defer f.Close()
	if err := json.NewDecoder(f).Decode(&b); err != nil {
		return b, err
	}
	if b.Tolerance == 0 {
		b.Tolerance = perf.DefaultTolerance
	}
	return b, nil
}

func resolveBinary(path string) (string, func(), error) {
	if path != "" {
		return path, nil, nil
	}
	// Build into a temp dir.
	tmp, err := os.MkdirTemp("", "perfbudget-")
	if err != nil {
		return "", nil, err
	}
	out := filepath.Join(tmp, "nikicode")
	cmd := exec.Command("go", "build", "-o", out, "./cmd/nikicode")
	if err := cmd.Run(); err != nil {
		_ = os.RemoveAll(tmp)
		return "", nil, fmt.Errorf("go build: %w", err)
	}
	return out, func() { _ = os.RemoveAll(tmp) }, nil
}

func tStat(path string) os.FileInfo {
	fi, _ := os.Stat(path)
	return fi
}

func binarySizeMB(fi os.FileInfo) float64 {
	if fi == nil {
		return -1
	}
	return float64(fi.Size()) / (1024 * 1024)
}

// measureVersion runs `binary --version` `runs` times and returns p50 wall ms.
func measureVersion(binary string, runs int) float64 {
	if runs < 1 {
		runs = 1
	}
	var samples []time.Duration
	for i := 0; i < runs; i++ {
		s := time.Now()
		_ = exec.Command(binary, "--version").Run()
		samples = append(samples, time.Since(s))
	}
	sort.Slice(samples, func(i, j int) bool { return samples[i] < samples[j] })
	median := samples[len(samples)/2]
	return float64(median.Microseconds()) / 1000.0
}
