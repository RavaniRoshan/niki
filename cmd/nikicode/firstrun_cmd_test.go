package main

import (
	"bytes"
	"strings"
	"testing"

	"github.com/RavaniRoshan/niki/internal/config"
	"github.com/RavaniRoshan/niki/internal/firstrun"
	"github.com/RavaniRoshan/niki/internal/provider"
)

// I2 product surface: `nikicode demo` replays the shipped recorded session,
// clearly labelled as recorded (no model, no network).
func TestDemoCommandReplaysRecordedSession(t *testing.T) {
	cmd := newDemoCommand()
	var out bytes.Buffer
	cmd.SetOut(&out)
	cmd.SetErr(&out)
	cmd.SetArgs([]string{})
	if err := cmd.Execute(); err != nil {
		t.Fatalf("demo command failed: %v\n%s", err, out.String())
	}
	got := out.String()
	lower := strings.ToLower(got)
	if !strings.Contains(lower, "recorded") {
		t.Fatalf("demo output must be labelled as recorded:\n%s", got)
	}
	if !strings.Contains(lower, "no model") && !strings.Contains(lower, "no network") {
		t.Fatalf("demo output must state no model/network involvement:\n%s", got)
	}
	// The replay must actually show transcript content, not just a label.
	if !strings.Contains(got, "read the main file") || !strings.Contains(got, "git_status") {
		t.Fatalf("demo output missing recorded transcript events:\n%s", got)
	}
	t.Logf("nikicode demo output:\n%s", got)
}

// I2 wiring: the unconfigured provider's first-turn guidance is built from
// the ordered detection result — one assertion per branch.
func TestFirstRunMessageCoversAllBranches(t *testing.T) {
	cases := []struct {
		name string
		d    firstrun.Detection
		want []string
	}{
		{
			name: "env-key",
			d: firstrun.Detection{
				Branch: firstrun.BranchEnvKey,
				EnvKey: "OPENAI_API_KEY",
				Offer:  "API key found in $OPENAI_API_KEY.",
			},
			want: []string{"no AI model configured", "OPENAI_API_KEY"},
		},
		{
			name: "ollama",
			d: firstrun.Detection{
				Branch:    firstrun.BranchOllama,
				OllamaURL: "http://localhost:11434",
				Offer:     `Local model server detected at http://localhost:11434 (Ollama default port). Next: set provider.name = "ollama" in nikicode.toml (or choose it in /connect) — no API key required, works fully offline.`,
			},
			want: []string{"no AI model configured", "localhost:11434", "ollama"},
		},
		{
			name: "no-key-demo",
			d: firstrun.Detection{
				Branch: firstrun.BranchNoKeyDemo,
				Offer:  "No API key and no local model server found. Next: run `nikicode demo` to watch a recorded session replay ...",
			},
			want: []string{"no AI model configured", "nikicode demo", "recorded"},
		},
	}
	for _, tc := range cases {
		msg := firstRunMessage(tc.d)
		for _, want := range tc.want {
			if !strings.Contains(msg, want) {
				t.Errorf("%s: message missing %q:\n%s", tc.name, want, msg)
			}
		}
		t.Logf("[%s] first-turn guidance: %s", tc.name, msg)
	}
}

// newDemoCommand must exist as a cobra command wired with a RunE (guard
// against a future refactor silently dropping the demo entry point).
func TestDemoCommandIsCobra(t *testing.T) {
	cmd := newDemoCommand()
	if cmd.Use == "" || cmd.RunE == nil {
		t.Fatal("demo command must have Use and RunE")
	}
	if !strings.HasPrefix(cmd.Use, "demo") {
		t.Fatalf("unexpected Use: %q", cmd.Use)
	}
}

// I4: `config show` must never print a key even when one is live in the
// config struct (legacy load or /connect session value).
func TestConfigShowNeverPrintsPlantedKey(t *testing.T) {
	cfg := config.ConfigWithSources{Config: config.Default(), Sources: map[string]string{}}
	cfg.Provider.Name = "anthropic"
	cfg.Provider.APIKey = "sk-planted-I4-show-canary-fedcba987654"
	cfg.Provider.EnvKey = "ANTHROPIC_API_KEY"
	cfg.Model.Name = "claude-3-5-sonnet"

	for _, showSrc := range []bool{false, true} {
		out := renderConfig(cfg, showSrc)
		if strings.Contains(out, cfg.Provider.APIKey) {
			t.Fatalf("planted key leaked into config show (showSources=%v):\n%s", showSrc, out)
		}
		if strings.Contains(out, "api_key") {
			t.Fatalf("api_key field rendered at all (showSources=%v):\n%s", showSrc, out)
		}
		if !strings.Contains(out, "provider.name = anthropic") {
			t.Fatalf("config show lost normal fields:\n%s", out)
		}
		t.Logf("config show (showSources=%v) clean:\n%s", showSrc, out)
	}
}

// I5 / bug B-01: provider.name = "ollama" must construct a working local
// provider (loopback base URL, no key) instead of falling through to the
// unconfigured provider.
func TestBuildProviderOllamaWiresLocalModel(t *testing.T) {
	cfg := config.Default()
	cfg.Provider.Name = "ollama"
	cfg.Model.Name = "qwen2.5-coder:7b-instruct-q4_K_M"

	p := buildProvider(cfg)
	if p.Name() == "none" {
		t.Fatal("ollama config produced the unconfigured provider (bug B-01)")
	}
	// The concrete client must target the local Ollama endpoint with no key.
	op, ok := p.(*provider.OpenAIProvider)
	if !ok {
		t.Fatalf("expected *provider.OpenAIProvider for ollama, got %T", p)
	}
	if !strings.Contains(op.BaseURL, "localhost") && !strings.Contains(op.BaseURL, "127.0.0.1") {
		t.Fatalf("ollama base URL must be loopback, got %q", op.BaseURL)
	}
	if !strings.Contains(op.BaseURL, "11434") {
		t.Fatalf("ollama base URL must use the default port, got %q", op.BaseURL)
	}
	if op.APIKey != "" {
		t.Fatalf("local model must not require an API key, got %q", op.APIKey)
	}
	if op.StartHint == "" || !strings.Contains(op.StartHint, "ollama serve") {
		t.Fatalf("local model must carry the exact start hint, got %q", op.StartHint)
	}
	t.Logf("[B-01 fixed] ollama -> %s (key=%q, hint=%q)", op.BaseURL, op.APIKey, op.StartHint)
}
