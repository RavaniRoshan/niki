package bench

import (
	"os"
	"testing"
	"time"
)

func TestSummarize(t *testing.T) {
	s, err := Summarize([]float64{1, 2, 3, 4, 5, 6, 7, 8, 9, 10})
	if err != nil {
		t.Fatal(err)
	}
	if s.N != 10 || s.Min != 1 || s.Max != 10 || s.Mean != 5.5 {
		t.Fatalf("stats = %+v", s)
	}
	if s.P50 != 5 || s.P95 != 9 {
		t.Fatalf("percentiles = %+v", s)
	}
	if _, err := Summarize(nil); err == nil {
		t.Fatal("empty input should error, never produce a zero row")
	}
}

func TestRunVersionTiny(t *testing.T) {
	vals, err := RunVersion("/bin/echo", []string{"hi"}, 3)
	if err != nil {
		t.Fatal(err)
	}
	s, err := Summarize(vals)
	if err != nil {
		t.Fatal(err)
	}
	if s.N != 3 || s.P50 <= 0 || s.P50 > 5000 {
		t.Fatalf("version stats implausible: %+v", s)
	}
}

func TestPTYRig(t *testing.T) {
	home := t.TempDir()
	// echo prints immediately: validates first-paint capture.
	res, err := RunPTY("/bin/echo", []string{"hi"}, "", home, 300*time.Millisecond)
	if err != nil {
		t.Fatal(err)
	}
	if res.FirstPaintMs < 0 || res.FirstPaintMs > 10000 {
		t.Fatalf("first paint implausible: %+v", res)
	}
	if res.HeaderMs != res.FirstPaintMs {
		t.Fatalf("empty keyword should stamp header at first paint: %+v", res)
	}
	// Missing keyword refuses instead of hanging.
	if _, err := RunPTY("/bin/echo", []string{"hi"}, "zzz-never-prints", home, time.Millisecond); err == nil {
		t.Fatal("missing keyword should refuse")
	}
}

func TestPTYCatEcho(t *testing.T) {
	if _, err := os.Stat("/bin/cat"); err != nil {
		t.Skip("no /bin/cat")
	}
	home := t.TempDir()
	// cat echoes input bytes: validates the echo rig end to end.
	samples, err := RunEcho("/bin/cat", nil, "", home, 3)
	if err != nil {
		t.Fatal(err)
	}
	if len(samples) != 3 {
		t.Fatalf("samples = %v", samples)
	}
	for _, s := range samples {
		if s < 0 || s > 2000 {
			t.Fatalf("echo sample implausible: %v", samples)
		}
	}
}

func TestFileBytes(t *testing.T) {
	if FileBytes("/bin/echo") <= 0 {
		t.Fatal("expected positive size")
	}
	if FileBytes("/nonexistent-xyz") != -1 {
		t.Fatal("expected -1 for missing file")
	}
}
