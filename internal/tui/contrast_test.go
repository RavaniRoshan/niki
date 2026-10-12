package tui

import (
	"image/color"
	"math"
	"testing"

	"charm.land/lipgloss/v2"
)

// WCAG 2.x contrast (PACK visual spec: text at least 4.5:1, glyphs
// at least 3:1, proven in a unit test). The theme module is the only
// place color literals may live, so this test pins the actual
// palette against the backgrounds it renders on.

// lum8 is the WCAG relative luminance of an sRGB triple.
func lum8(r, g, b float64) float64 {
	lin := func(c float64) float64 {
		c /= 255
		if c <= 0.03928 {
			return c / 12.92
		}
		return math.Pow((c+0.055)/1.055, 2.4)
	}
	return 0.2126*lin(r) + 0.7152*lin(g) + 0.0722*lin(b)
}

// luminance reads a color's 16-bit sRGB components.
func luminance(c color.Color) float64 {
	r, g, b, _ := c.RGBA()
	return lum8(float64(r>>8), float64(g>>8), float64(b>>8))
}

// contrastRatio is the WCAG contrast ratio between two colors.
func contrastRatio(fg, bg color.Color) float64 {
	l1, l2 := luminance(fg), luminance(bg)
	if l1 < l2 {
		l1, l2 = l2, l1
	}
	return (l1 + 0.05) / (l2 + 0.05)
}

// The backgrounds the palette actually renders on: the terminal's
// dark default, and the raised row's own painted background.
var (
	darkBG  color.Color = lipgloss.Color("#14161a")
	panelBG color.Color = lipgloss.Color("#1e293b")
)

func TestThemeContrast(t *testing.T) {
	th := NewDefaultTheme()

	// Text roles must clear 4.5:1 on the dark background.
	text := []struct {
		name string
		c    color.Color
	}{
		{"assistant", th.Assistant.GetForeground()},
		{"muted", th.Muted.GetForeground()},
		{"tool detail", th.ToolDetail.GetForeground()},
		{"status hints", th.StatusHints.GetForeground()},
		{"status dir", th.StatusDir.GetForeground()},
		{"badge model", th.BadgeModel.GetForeground()},
		{"card value", th.CardValue.GetForeground()},
		{"placeholder", th.Placeholder.GetForeground()},
		{"user text", th.UserText.GetForeground()},
		{"tool name", th.ToolName.GetForeground()},
		{"card title", th.CardTitle.GetForeground()},
	}
	for _, tc := range text {
		if ratio := contrastRatio(tc.c, darkBG); ratio < 4.5 {
			t.Errorf("[contrast] text %s: %.2f:1, want >= 4.5:1", tc.name, ratio)
		} else {
			t.Logf("[contrast] text %-12s %.2f:1", tc.name, ratio)
		}
	}

	// Glyph and status roles must clear 3:1.
	glyph := []struct {
		name string
		c    color.Color
	}{
		{"accent", th.Accent.GetForeground()},
		{"success", th.Success.GetForeground()},
		{"error", th.Error.GetForeground()},
		{"activity glyph", th.ActivityGlyph.GetForeground()},
		{"header", th.Header.GetForeground()},
		{"user", th.User.GetForeground()},
		{"badge perm", th.BadgePerm.GetForeground()},
		{"composer border", th.ComposerBorder.GetForeground()},
	}
	for _, gc := range glyph {
		if ratio := contrastRatio(gc.c, darkBG); ratio < 3.0 {
			t.Errorf("[contrast] glyph %s: %.2f:1, want >= 3:1", gc.name, ratio)
		} else {
			t.Logf("[contrast] glyph %-13s %.2f:1", gc.name, ratio)
		}
	}

	// The raised user row paints its own background.
	if ratio := contrastRatio(th.RaisedUser.GetForeground(), th.RaisedUser.GetBackground()); ratio < 4.5 {
		t.Errorf("[contrast] raised user row: %.2f:1, want >= 4.5:1", ratio)
	} else {
		t.Logf("[contrast] raised row     %.2f:1", ratio)
	}
	// Its background must also separate from the terminal default so
	// the row reads as raised.
	if ratio := contrastRatio(panelBG, darkBG); ratio < 1.2 {
		t.Errorf("[contrast] raised background separation: %.2f:1, want >= 1.2:1", ratio)
	}
}
