package tui

import (
	"fmt"
	"math"
	"strings"
	"time"
)

// BrailleLevels defines the 7 progressive vertical Braille fill glyphs.
var BrailleLevels = []string{
	"⣀", // Level 1 (bottom dots)
	"⣄", // Level 2
	"⣤", // Level 3
	"⣦", // Level 4
	"⣶", // Level 5
	"⣷", // Level 6
	"⣿", // Level 7 (full 8-dot cell)
}

const (
	PhaseOrchestrating = "Orchestrating…"
	PhasePrompting     = "Prompting…"
	PhaseWorking       = "Working…"
	PhaseCompleted     = "Completed."
	PhaseRateLimited   = "Rate limited…"
)

// RenderBrailleBar produces a progressive 7-level Braille progress bar.
func RenderBrailleBar(width int, fraction float64, ascii bool) string {
	if width <= 0 {
		width = 10
	}
	if fraction < 0 {
		fraction = 0
	}
	if fraction > 1 {
		fraction = 1
	}

	if ascii {
		filled := int(math.Round(fraction * float64(width)))
		if filled > width {
			filled = width
		}
		var sb strings.Builder
		sb.WriteString("[")
		sb.WriteString(strings.Repeat("=", filled))
		sb.WriteString(strings.Repeat(" ", width-filled))
		sb.WriteString("]")
		return sb.String()
	}

	totalSteps := width * 7
	currentStep := int(math.Round(fraction * float64(totalSteps)))
	if currentStep > totalSteps {
		currentStep = totalSteps
	}

	var sb strings.Builder
	sb.WriteString("[")
	for i := 0; i < width; i++ {
		cellStep := currentStep - (i * 7)
		if cellStep >= 7 {
			sb.WriteString(BrailleLevels[6]) // ⣿ full cell
		} else if cellStep <= 0 {
			sb.WriteString(" ")
		} else {
			sb.WriteString(BrailleLevels[cellStep-1])
		}
	}
	sb.WriteString("]")
	return sb.String()
}

// SwarmAgent represents the active state of a background worker or subagent.
type SwarmAgent struct {
	ID       string
	Name     string
	Progress float64
	Phase    string
	Duration time.Duration
}

// RenderSwarmProgress renders a hierarchical task card tree for active subagents.
func RenderSwarmProgress(agents []SwarmAgent, th Theme, width int, ascii bool) string {
	if len(agents) == 0 {
		return ""
	}
	var sb strings.Builder
	barWidth := 8
	if width < 60 {
		barWidth = 5
	}

	for i, ag := range agents {
		connector := "├"
		if i == 0 && len(agents) == 1 {
			connector = "─"
		} else if i == 0 {
			connector = "┌"
		} else if i == len(agents)-1 {
			connector = "└"
		}
		if ascii {
			if connector == "┌" || connector == "└" || connector == "├" {
				connector = "+"
			} else {
				connector = "-"
			}
		}

		bar := RenderBrailleBar(barWidth, ag.Progress, ascii)
		pct := int(ag.Progress * 100)
		phase := ag.Phase
		if phase == "" {
			phase = PhaseWorking
		}

		durStr := ""
		if ag.Duration > 0 {
			durStr = fmt.Sprintf(" %s", ag.Duration.Round(time.Millisecond*100))
		}

		styledConnector := th.ToolTree.Render(connector)
		styledBar := th.Accent.Render(bar)
		styledPhase := th.Muted.Render(phase + durStr)
		line := fmt.Sprintf("%s sub-agent: %s %s %d%% · %s", styledConnector, ag.Name, styledBar, pct, styledPhase)
		sb.WriteString(line)
		if i < len(agents)-1 {
			sb.WriteString("\n")
		}
	}
	return sb.String()
}
