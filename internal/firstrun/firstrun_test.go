package firstrun

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

// I2 acceptance: the first-run flow detects, in order — (1) an API key in
// the environment, (2) a running local model server on its default port,
// (3) neither, in which case it offers a recorded no-key demo. One test
// per branch, each printing its output.

// TestFirstRunBranch1EnvKey: an env API key wins; the probe is never made
// (detection order is enforced, not just preferred).
func TestFirstRunBranch1EnvKey(t *testing.T) {
	getenv := func(k string) string {
		if k == "ANTHROPIC_API_KEY" {
			return "sk-test-planted-key"
		}
		return ""
	}
	probeCalled := false
	probe := func(ctx context.Context, url string) error {
		probeCalled = true
		return nil
	}

	d := Detect(getenv, probe)

	if probeCalled {
		t.Fatal("probe ran even though an env API key was found — detection order violated")
	}
	if d.Branch != BranchEnvKey {
		t.Fatalf("expected branch %q, got %q", BranchEnvKey, d.Branch)
	}
	if d.EnvKey != "ANTHROPIC_API_KEY" {
		t.Fatalf("expected EnvKey ANTHROPIC_API_KEY, got %q", d.EnvKey)
	}
	if strings.Contains(d.Offer, "sk-test-planted-key") {
		t.Fatal("detection output leaked the key value; only the variable name may appear")
	}
	t.Logf("[branch 1] env key: %s | offer: %s", d.EnvKey, d.Offer)
}

// TestFirstRunBranch2RunningOllama: no env key, but a local model server
// answers on its default port -> branch ollama with the exact next step.
func TestFirstRunBranch2RunningOllama(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/tags" {
			http.NotFound(w, r)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"models":[{"name":"qwen2.5-coder:7b-instruct-q4_K_M"}]}`))
	}))
	defer srv.Close()

	emptyEnv := func(string) string { return "" }
	d := DetectAt(emptyEnv, ProbeOllama, srv.URL)

	if d.Branch != BranchOllama {
		t.Fatalf("expected branch %q, got %q", BranchOllama, d.Branch)
	}
	if d.OllamaURL == "" {
		t.Fatal("OllamaURL not recorded")
	}
	if d.OllamaURL != srv.URL {
		t.Fatalf("OllamaURL must record the probed URL: got %q want %q", d.OllamaURL, srv.URL)
	}
	if !strings.Contains(d.Offer, d.OllamaURL) {
		t.Fatalf("offer must name the detected server URL: %q", d.Offer)
	}
	if !strings.Contains(d.Offer, "ollama") {
		t.Fatalf("offer must give the exact next step (provider name ollama): %q", d.Offer)
	}
	t.Logf("[branch 2] local server: %s | offer: %s", d.OllamaURL, d.Offer)
}

// TestFirstRunBranch2RunningOllamaRealProbe: the production probe against a
// live HTTP server (stands in for Ollama's default port) returns success,
// and against a dead port returns an error — so branch 3 is reachable.
func TestFirstRunBranch2RunningOllamaRealProbe(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/api/tags" {
			_, _ = w.Write([]byte(`{"models":[]}`))
			return
		}
		http.NotFound(w, r)
	}))
	url := srv.URL

	if err := ProbeOllama(context.Background(), url); err != nil {
		t.Fatalf("probe against live server failed: %v", err)
	}
	srv.Close()
	if err := ProbeOllama(context.Background(), url); err == nil {
		t.Fatal("probe against dead server succeeded")
	}
	t.Logf("[probe] live=ok dead=error (default URL would be %s)", DefaultOllamaURL)
}

// TestFirstRunBranch3RecordedDemo: no key, no local server -> the flow
// offers the no-key demo and the offer is a replay of a recorded session,
// explicitly labelled as recorded.
func TestFirstRunBranch3RecordedDemo(t *testing.T) {
	emptyEnv := func(string) string { return "" }
	dead := func(ctx context.Context, url string) error {
		return context.DeadlineExceeded
	}

	d := Detect(emptyEnv, dead)
	if d.Branch != BranchNoKeyDemo {
		t.Fatalf("expected branch %q, got %q", BranchNoKeyDemo, d.Branch)
	}
	if !strings.Contains(strings.ToLower(d.Offer), "recorded") {
		t.Fatalf("no-key offer must mention the recorded demo: %q", d.Offer)
	}

	demo, err := LoadRecordedDemo()
	if err != nil {
		t.Fatalf("LoadRecordedDemo: %v", err)
	}
	if !strings.Contains(strings.ToLower(demo.Label), "recorded") {
		t.Fatalf("demo label must say it is recorded: %q", demo.Label)
	}
	lowerLabel := strings.ToLower(demo.Label)
	if !strings.Contains(lowerLabel, "no model") && !strings.Contains(lowerLabel, "no network") {
		t.Fatalf("demo label must state it is not a live model/network run: %q", demo.Label)
	}
	if len(demo.Events) == 0 {
		t.Fatal("recorded session has no events to replay")
	}

	t.Logf("[branch 3] offer: %s", d.Offer)
	t.Logf("[branch 3] demo label: %s", demo.Label)
	for _, e := range demo.Events {
		t.Logf("[replay] %s> %s", e.Role, e.Text)
	}
}
