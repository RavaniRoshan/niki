package tui

// SpinnerStyle defines the animation aesthetic.
type SpinnerStyle int

const (
	// SpinnerBloom uses flower/bloom petal glyphs (✻ ✼ ✽ ✾ ✿ ❀).
	SpinnerBloom SpinnerStyle = iota
	// SpinnerBraille uses rotating 10-frame Braille dots (⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏).
	SpinnerBraille
	// SpinnerSweep uses rotating orb quadrants (◐ ◓ ◑ ◒).
	SpinnerSweep
	// SpinnerPulse uses audio wave blocks (  ▃ ▄ ▅ ▆ ▇ █ ▇ ▆ ▅ ▄ ▃).
	SpinnerPulse
)

var SpinnerFrames = map[SpinnerStyle][]string{
	SpinnerBloom:   {"✻", "✼", "✽", "✾", "✿", "❀"},
	SpinnerBraille: {"⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"},
	SpinnerSweep:   {"◐", "◓", "◑", "◒"},
	SpinnerPulse:   {" ", "▃", "▄", "▅", "▆", "▇", "█", "▇", "▆", "▅", "▄", "▃"},
}

// ThinkingVerbs rotate during thinking to give live visual progress without polling.
var ThinkingVerbs = []string{
	"thinking…",
	"pondering codebase…",
	"analyzing context…",
	"consulting archives…",
	"formulating plan…",
}

// SpinGlyphWithStyle returns the glyph for the given frame, style, and terminal mode.
func SpinGlyphWithStyle(style SpinnerStyle, frame int, reduced, ascii bool) string {
	if ascii {
		frames := []string{"-", "\\", "|", "/"}
		if reduced {
			return "-"
		}
		return frames[frame%len(frames)]
	}
	if reduced {
		return "◐"
	}
	frames, ok := SpinnerFrames[style]
	if !ok || len(frames) == 0 {
		frames = SpinnerFrames[SpinnerBloom]
	}
	return frames[frame%len(frames)]
}

// ParseSpinnerStyle converts a name string to SpinnerStyle.
func ParseSpinnerStyle(name string) (SpinnerStyle, bool) {
	switch name {
	case "bloom", "flower":
		return SpinnerBloom, true
	case "braille", "dots":
		return SpinnerBraille, true
	case "sweep", "orb":
		return SpinnerSweep, true
	case "pulse", "wave":
		return SpinnerPulse, true
	default:
		return SpinnerBloom, false
	}
}

// StyleName returns the human-readable name of a SpinnerStyle.
func StyleName(s SpinnerStyle) string {
	switch s {
	case SpinnerBloom:
		return "bloom"
	case SpinnerBraille:
		return "braille"
	case SpinnerSweep:
		return "sweep"
	case SpinnerPulse:
		return "pulse"
	default:
		return "bloom"
	}
}
