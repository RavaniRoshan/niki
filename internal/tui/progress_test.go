package tui

import (
	"strings"
	"testing"
	"time"
)

func TestRenderBrailleBar(t *testing.T) {
	// Zero progress
	empty := RenderBrailleBar(6, 0.0, false)
	if empty != "[      ]" {
		t.Fatalf("expected empty bar '[      ]', got %q", empty)
	}

	// 100% progress
	full := RenderBrailleBar(6, 1.0, false)
	if full != "[⣿⣿⣿⣿⣿⣿]" {
		t.Fatalf("expected full braille bar '[⣿⣿⣿⣿⣿⣿]', got %q", full)
	}

	// Mid progress
	mid := RenderBrailleBar(6, 0.5, false)
	if !strings.HasPrefix(mid, "[⣿⣿⣿") {
		t.Fatalf("expected mid bar to have leading full cells, got %q", mid)
	}

	// ASCII fallback
	asciiFull := RenderBrailleBar(6, 1.0, true)
	if asciiFull != "[======]" {
		t.Fatalf("expected ascii full bar '[======]', got %q", asciiFull)
	}

	asciiEmpty := RenderBrailleBar(6, 0.0, true)
	if asciiEmpty != "[      ]" {
		t.Fatalf("expected ascii empty bar '[      ]', got %q", asciiEmpty)
	}
}

func TestRenderSwarmProgress(t *testing.T) {
	agents := []SwarmAgent{
		{ID: "ag-1", Name: "worker-1", Progress: 0.65, Phase: PhaseWorking, Duration: 2 * time.Second},
		{ID: "ag-2", Name: "worker-2", Progress: 1.0, Phase: PhaseCompleted, Duration: 5 * time.Second},
	}
	th := NewDefaultTheme()
	out := RenderSwarmProgress(agents, th, 80, false)
	if !strings.Contains(out, "worker-1") || !strings.Contains(out, "worker-2") {
		t.Fatalf("swarm output missing worker names: %s", out)
	}
	if !strings.Contains(out, "65%") || !strings.Contains(out, "100%") {
		t.Fatalf("swarm output missing percentage values: %s", out)
	}
}
