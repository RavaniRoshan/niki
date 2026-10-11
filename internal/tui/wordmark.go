package tui

import (
	"io"
	"os"
	"strings"

	"github.com/charmbracelet/lipgloss"
	"github.com/muesli/termenv"
)

// Wordmark renders the original NikiCode brand lockup: the orb motif
// (single eye, designed for NikiCode) beside the name. Nothing is
// borrowed: no competitor name, tagline, logo, or asset.
//
//	width >= 80: three-row lockup with tagline.
//	50 <= width < 80: compact one-row form.
//	width < 50: name only (always fits a narrow viewport).
//
// ascii selects the pure-ASCII fallback (TERM=dumb terminals): the
// half-block orb becomes bracket art and the eye becomes "o".
func Wordmark(width int, ascii bool) string {
	const name = "NikiCode"
	const tag = "personal coding agent"
	if width < 50 {
		return name
	}
	if width < 80 {
		if ascii {
			return "(o) " + name
		}
		return "◐ " + name
	}
	if ascii {
		return strings.Join([]string{
			"  _____",
			" /  o  \\   " + name,
			" \\_____/   " + tag,
		}, "\n")
	}
	return strings.Join([]string{
		"  ▄███▄",
		" ███◐███   " + name,
		"  ▀███▀   " + tag,
	}, "\n")
}

// CompactMark is the one-row header form: orb + name, or the bare
// name below 50 columns. Headers always use this, never the full lockup.
func CompactMark(width int, ascii bool) string {
	if width < 50 {
		return "NikiCode"
	}
	if ascii {
		return "(o) NikiCode"
	}
	return "◐ NikiCode"
}

// BrandLine is the in-TUI brand line shown under the header.
func BrandLine() string {
	if UseASCII() {
		return "NikiCode - personal coding agent"
	}
	return "NikiCode — personal coding agent"
}

// DisplayMode names the rendering contract (P1): normal, nocolor
// (NO_COLOR), ascii (TERM=dumb), or plain (NIKI_PLAIN=1: ASCII +
// no color + no animation, for logs and screen readers).
type DisplayMode string

const (
	ModeNormal  DisplayMode = "normal"
	ModeNoColor DisplayMode = "nocolor"
	ModeASCII   DisplayMode = "ascii"
	ModePlain   DisplayMode = "plain"
)

// CurrentDisplayMode resolves the mode from the environment.
func CurrentDisplayMode() DisplayMode {
	if os.Getenv("NIKI_PLAIN") == "1" {
		return ModePlain
	}
	if UseASCII() {
		return ModeASCII
	}
	if os.Getenv("NO_COLOR") != "" {
		return ModeNoColor
	}
	return ModeNormal
}

// UseASCII reports whether the terminal needs the ASCII fallback:
// TERM=dumb has no unicode block/orb glyphs worth emitting, and
// NIKI_PLAIN=1 requests plain output for logs/screen readers.
func UseASCII() bool {
	return os.Getenv("TERM") == "dumb" || os.Getenv("NIKI_PLAIN") == "1"
}

// ChromeBorder returns the frame border for the current display
// mode (P1): rounded unicode normally, pure-ASCII (+-|) under
// ASCII/plain so dumb terminals and logs stay clean.
// asciiBorder is the pure-ASCII frame border (+, -, |).
var asciiBorder = lipgloss.Border{
	Top: "-", Bottom: "-", Left: "|", Right: "|",
	TopLeft: "+", TopRight: "+", BottomLeft: "+", BottomRight: "+",
	MiddleLeft: "+", MiddleRight: "+", Middle: "+",
	MiddleTop: "+", MiddleBottom: "+",
}

func ChromeBorder() lipgloss.Border {
	if UseASCII() {
		return asciiBorder
	}
	return lipgloss.RoundedBorder()
}

// Ellipsis is "…" normally, "..." under ASCII/plain.
func Ellipsis() string {
	if UseASCII() {
		return "..."
	}
	return "…"
}

// ApplyDisplayEnv refreshes lipgloss's default renderer from the
// environment so NO_COLOR and NIKI_PLAIN take effect even when the
// process started without them (a fresh renderer re-reads the env;
// the stale default would keep a color profile). Call once at TUI
// startup and in tests that change display env vars. The writer is
// a parameter — the render path itself performs no process-output I/O.
func ApplyDisplayEnv(w io.Writer) {
	r := lipgloss.NewRenderer(w)
	if CurrentDisplayMode() == ModePlain || CurrentDisplayMode() == ModeASCII {
		r = lipgloss.NewRenderer(w, termenv.WithProfile(termenv.Ascii))
	}
	lipgloss.SetDefaultRenderer(r)
}
