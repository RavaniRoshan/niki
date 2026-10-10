// Package firstrun implements the ordered first-run detection flow:
// (1) an API key in the environment, (2) a running local model server on
// its default port, (3) neither — in which case the flow offers a no-key
// demo that replays a recorded session, labelled as recorded.
package firstrun

import (
	"context"
	"embed"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"strings"
	"time"

	"github.com/RavaniRoshan/niki/internal/config"
)

//go:embed recorded_session.json
var recordedFS embed.FS

// DefaultOllamaURL is Ollama's default local port.
const DefaultOllamaURL = "http://localhost:11434"

// Branch names the first-run path that applies, in detection priority order.
type Branch string

const (
	BranchEnvKey    Branch = "env-key"
	BranchOllama    Branch = "ollama"
	BranchNoKeyDemo Branch = "no-key-demo"
)

// Detection is the ordered first-run detection result.
type Detection struct {
	Branch Branch
	// EnvKey is the NAME of the environment variable holding a key.
	// The value is never stored or printed here.
	EnvKey string
	// OllamaURL records the local server URL that answered (branch 2).
	OllamaURL string
	// Offer is the exact guidance shown for this branch.
	Offer string
}

// Detect runs ordered detection against the default local-server URL.
func Detect(getenv func(string) string, probe func(context.Context, string) error) Detection {
	return DetectAt(getenv, probe, DefaultOllamaURL)
}

// DetectAt is Detect with an explicit local-server URL (tests, alt ports).
func DetectAt(getenv func(string) string, probe func(context.Context, string) error, ollamaURL string) Detection {
	// Branch 1: an API key in the environment (order: first match wins).
	for _, key := range config.EnvKeysInOrder() {
		if getenv(key) != "" {
			return Detection{
				Branch: BranchEnvKey,
				EnvKey: key,
				Offer: fmt.Sprintf(
					"API key found in $%s. Next: niki starts with that provider, or run /connect to pick one explicitly. (Key values are never printed, logged, or written to config files.)",
					key,
				),
			}
		}
	}
	// Branch 2: a local model server answering on its default port.
	if err := probe(context.Background(), ollamaURL); err == nil {
		return Detection{
			Branch:    BranchOllama,
			OllamaURL: ollamaURL,
			Offer: fmt.Sprintf(
				"Local model server detected at %s (Ollama default port). Next: set provider.name = \"ollama\" in nikicode.toml (or choose it in /connect) — no API key required, works fully offline.",
				ollamaURL,
			),
		}
	}
	// Branch 3: neither — offer the recorded no-key demo.
	return Detection{
		Branch: BranchNoKeyDemo,
		Offer:  "No API key and no local model server found. Next: run `nikicode demo` to watch a recorded session replay (pre-captured transcript, clearly labelled — no model, no network, no spend), or set ANTHROPIC_API_KEY / OPENAI_API_KEY, or start Ollama.",
	}
}

// DetectDefaults is the production entry point: process environment plus
// an HTTP probe of the Ollama default port with a bounded timeout.
func DetectDefaults() Detection {
	return Detect(os.Getenv, ProbeOllama)
}

// ProbeOllama reports whether a local model server answers at url.
// Any HTTP response counts as "running"; only connection-level
// failures/timeouts count as absent.
func ProbeOllama(ctx context.Context, url string) error {
	ctx, cancel := context.WithTimeout(ctx, 500*time.Millisecond)
	defer cancel()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, strings.TrimSuffix(url, "/")+"/api/tags", nil)
	if err != nil {
		return err
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	return nil
}

// Event is one line of the recorded session.
type Event struct {
	Role string `json:"role"`
	Text string `json:"text"`
}

// RecordedDemo is the shipped no-key demo: a pre-captured transcript.
type RecordedDemo struct {
	Label  string  `json:"label"`
	Events []Event `json:"events"`
}

// LoadRecordedDemo loads the shipped recorded session fixture.
func LoadRecordedDemo() (RecordedDemo, error) {
	var demo RecordedDemo
	raw, err := recordedFS.ReadFile("recorded_session.json")
	if err != nil {
		return demo, fmt.Errorf("recorded session fixture missing: %w", err)
	}
	if err := json.Unmarshal(raw, &demo); err != nil {
		return demo, fmt.Errorf("recorded session fixture corrupt: %w", err)
	}
	if !strings.Contains(strings.ToLower(demo.Label), "recorded") {
		return demo, fmt.Errorf("recorded session fixture must be labelled as recorded, got %q", demo.Label)
	}
	return demo, nil
}
