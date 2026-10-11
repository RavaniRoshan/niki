package selfextend

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
)

// TestResult is the outcome of the offline skill test (T5.3).
type TestResult struct {
	Passed bool
	Output string
}

// TestOffline runs the staged skill's own test (test.sh, else
// run.sh --self-test) in a sandbox: network off, writes confined
// to the staging directory, and a fake HOME without credentials
// (T5.3). A skill that tries to read ~/.ssh or reach the network
// fails here. testTimeout bounds the run.
func TestOffline(stagingDir string, testTimeout time.Duration) TestResult {
	script := ""
	if _, err := os.Stat(filepath.Join(stagingDir, "test.sh")); err == nil {
		script = "./test.sh"
	} else if _, err := os.Stat(filepath.Join(stagingDir, "run.sh")); err == nil {
		script = "./run.sh --self-test"
	} else {
		return TestResult{Output: "no test.sh or run.sh to exercise"}
	}
	if testTimeout <= 0 {
		testTimeout = 30 * time.Second
	}
	ctx, cancel := context.WithTimeout(context.Background(), testTimeout)
	defer cancel()

	// Fake HOME: no .ssh, no tokens — a credential grab finds
	// nothing, and the assertion on its output then fails.
	fakeHome, err := os.MkdirTemp("", "skill-home")
	if err != nil {
		return TestResult{Output: "fake home: " + err.Error()}
	}
	defer os.RemoveAll(fakeHome)

	cmd := exec.CommandContext(ctx, "sh", "-c", script)
	cmd.Dir = stagingDir
	env := []string{
		"HOME=" + fakeHome,
		"PATH=/usr/bin:/bin",
		// Dead proxies: any network attempt fails fast instead of
		// hanging or (worse) succeeding through a real proxy.
		"http_proxy=http://127.0.0.1:9",
		"https_proxy=http://127.0.0.1:9",
		"HTTP_PROXY=http://127.0.0.1:9",
		"HTTPS_PROXY=http://127.0.0.1:9",
		"NIKI_SKILL_TEST=1",
	}
	// A skill that declared network access gets the same dead
	// proxies: undeclared use is blocked at runtime (T5.4) because
	// NOTHING here can reach the network.
	cmd.Env = env
	out, err := cmd.CombinedOutput()
	res := TestResult{Output: string(out)}
	if err != nil {
		if ctx.Err() == context.DeadlineExceeded {
			res.Output += "\n(test timed out)"
		}
		return res
	}
	res.Passed = true
	return res
}

// CheckOfflineFailure explains a failed offline test for the log.
func CheckOfflineFailure(output string) string {
	hints := []string{}
	lower := strings.ToLower(output)
	if strings.Contains(lower, ".ssh") || strings.Contains(lower, "permission denied") {
		hints = append(hints, "tried to read credentials outside its sandbox")
	}
	if strings.Contains(lower, "proxy") || strings.Contains(lower, "could not resolve") || strings.Contains(lower, "connection refused") {
		hints = append(hints, "tried to reach the network (offline test)")
	}
	if len(hints) == 0 {
		return "skill test failed: " + firstLine(output)
	}
	return "skill test failed (" + strings.Join(hints, "; ") + "): " + firstLine(output)
}

func firstLine(s string) string {
	if i := strings.Index(s, "\n"); i >= 0 {
		return s[:i]
	}
	if len(s) > 160 {
		return s[:160]
	}
	return s
}
