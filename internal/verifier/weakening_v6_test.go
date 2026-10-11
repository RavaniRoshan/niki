package verifier

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// V6: test-weakening detection — removing assertions, adding skips, or
// commenting out assertions blocks T2 even when every check passes.
func TestWeakenedSuiteNeverVerifies(t *testing.T) {
	dir := goFixtureDir(t)
	testFile := filepath.Join(dir, "calc_test.go")
	strong := "package flaketest\n\n" +
		"import \"testing\"\n\n" +
		"func TestAdd(t *testing.T) {\n" +
		"\tassert(t, 1+1 == 2)\n" +
		"\tassert(t, 2+2 == 4)\n" +
		"\tif 3+3 != 6 { t.Error(\"bad\") }\n" +
		"}\n\nfunc assert(t *testing.T, ok bool) {\n\tif !ok { t.Fatal(\"assertion failed\") }\n}\n"
	if err := os.WriteFile(testFile, []byte(strong), 0o644); err != nil {
		t.Fatal(err)
	}

	// Measure directly: strong vs weakened variants.
	before := MeasureTests(dir)
	if before["calc_test.go"].Assertions < 3 {
		t.Fatalf("[V6] fixture should have >=3 assertions, got %+v", before["calc_test.go"])
	}

	// Variant A: assertions removed.
	weakenedA := strings.Replace(strong, "\tassert(t, 2+2 == 4)\n", "", 1)
	afterA := MeasureTests(writeCopy(t, dir, testFile, weakenedA))
	evA := DetectWeakening(before, afterA)
	if len(evA) == 0 {
		t.Fatalf("[V6] removed assertion not detected")
	}
	t.Logf("[V6] removed-assertions: %s", evA[0])

	// Variant B: a new skip added.
	weakenedB := strings.Replace(strong, "func TestAdd(t *testing.T) {", "func TestAdd(t *testing.T) {\n\tt.Skip(\"flaky on CI\")", 1)
	afterB := MeasureTests(writeCopy(t, dir, testFile, weakenedB))
	evB := DetectWeakening(before, afterB)
	if !hasSignal(evB, "new-skips") {
		t.Fatalf("[V6] new skip not detected: %v", evB)
	}
	t.Logf("[V6] new-skips: %s", evB[0])

	// Variant C: assertion commented out.
	weakenedC := strings.Replace(strong, "\tassert(t, 1+1 == 2)", "\t// assert(t, 1+1 == 2)", 1)
	afterC := MeasureTests(writeCopy(t, dir, testFile, weakenedC))
	evC := DetectWeakening(before, afterC)
	if len(evC) == 0 {
		t.Fatalf("[V6] commented-out assertion not detected")
	}
	t.Logf("[V6] commented-out: %s", evC[0])

	// End-to-end: the runner weakens the suite mid-run, then every
	// check passes. The verdict must NOT be Verified.
	s := Standard{Dir: dir, Runner: func(_ context.Context, c Check) CheckResult {
		_ = os.WriteFile(testFile, []byte(weakenedA), 0o644)
		return CheckResult{Check: c, Passed: true}
	}}
	v, err := s.Verify(context.Background(), Request{Prompt: "make it green", Claim: "done"})
	if err != nil {
		t.Fatal(err)
	}
	if v.Result == Verified || !strings.Contains(v.Reason, "test weakening") {
		t.Fatalf("[V6] weakened suite must not verify: %+v", v)
	}
	t.Logf("[V6] all checks pass but suite weakened -> %s (%s)", v.Result, v.Reason)

	// Strengthening (adding an assertion) is never flagged.
	stronger := strong + "\nfunc TestMore(t *testing.T) { assert(t, true) }\n"
	afterD := MeasureTests(writeCopy(t, dir, testFile, stronger))
	if ev := DetectWeakening(before, afterD); len(ev) != 0 {
		t.Fatalf("[V6] strengthening must not be flagged: %v", ev)
	}
	t.Logf("[V6] strengthening the suite is never flagged")
}

func writeCopy(t *testing.T, dir, name, content string) string {
	t.Helper()
	d := t.TempDir()
	_ = os.WriteFile(filepath.Join(d, "go.mod"), []byte("module flaketest\n\ngo 1.21\n"), 0o644)
	if err := os.WriteFile(filepath.Join(d, filepath.Base(name)), []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
	return d
}

func hasSignal(evs []WeakeningEvent, signal string) bool {
	for _, e := range evs {
		if e.Signal == signal {
			return true
		}
	}
	return false
}
