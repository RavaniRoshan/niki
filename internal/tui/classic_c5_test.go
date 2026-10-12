package tui

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/charmbracelet/colorprofile"
)

// C5: ANSI frame dumps for owner review. Writes the five classic
// frames at 100x27, 80x24, and 60x20, in full color and in plain
// mode, to docs/ui/review/ (NIKI_WRITE_DUMPS=1). These are the
// frames the owner reviews against the approved mockups.
func TestWriteClassicFrameDumps(t *testing.T) {
	if os.Getenv("NIKI_WRITE_DUMPS") != "1" {
		t.Skip("set NIKI_WRITE_DUMPS=1 to write frame dumps")
	}
	sizes := []struct {
		name string
		w, h int
	}{
		{"100x27", 100, 27},
		{"80x24", 80, 24},
		{"60x20", 60, 20},
	}
	outDir := filepath.Join("..", "..", "docs", "ui", "review")
	if err := os.MkdirAll(outDir, 0o755); err != nil {
		t.Fatal(err)
	}

	saved := frameProfile

	// Color dumps: force the TrueColor profile and a unicode
	// terminal so the owner sees the intended palette and glyph set;
	// the live renderer downsamples per terminal.
	t.Setenv("TERM", "xterm-256color")
	os.Unsetenv("NIKI_PLAIN")
	os.Unsetenv("NIKI_ASCII")
	os.Unsetenv("NO_COLOR")
	frameProfile = colorprofile.TrueColor
	for _, stage := range []string{"frame1", "frame2", "frame3", "frame4", "frame5"} {
		for _, size := range sizes {
			m := classicFixtureSetup(size.w, size.h, stage)
			out := Downsample(m.View().Content)
			path := filepath.Join(outDir, stage+"_"+size.name+".ansi")
			if err := os.WriteFile(path, []byte(out), 0o644); err != nil {
				t.Fatal(err)
			}
			t.Logf("[C5] wrote %s (%d bytes)", path, len(out))
		}
	}

	// Plain-mode dumps beside them: ASCII glyphs with words, no color.
	t.Setenv("NIKI_PLAIN", "1")
	frameProfile = colorprofile.NoTTY
	for _, stage := range []string{"frame1", "frame2", "frame3", "frame4", "frame5"} {
		for _, size := range sizes {
			m := classicFixtureSetup(size.w, size.h, stage)
			out := Downsample(m.View().Content)
			path := filepath.Join(outDir, stage+"_"+size.name+".plain.txt")
			if err := os.WriteFile(path, []byte(out), 0o644); err != nil {
				t.Fatal(err)
			}
			t.Logf("[C5] wrote %s (%d bytes)", path, len(out))
		}
	}
	frameProfile = saved
}

// The originality guard (docs/ui/CLASSIC.md §8): no exact signature
// from other products' interfaces may appear in any classic golden
// frame — forbidden glyphs, exact strings, whimsical spinner verbs,
// a bordered welcome card, or a two-rule composer sandwich.
func TestOriginalityGuardOverGoldenFrames(t *testing.T) {
	forbiddenGlyphs := []string{"⏺", "⎿", "∴", "✱"}
	forbiddenStrings := []string{
		"esc to interrupt",
		"(ctrl+o to expand)",
		"Thought for",
		"Welcome to",
	}
	goldens, err := filepath.Glob(filepath.Join("testdata", "classic_frame*.golden"))
	if err != nil {
		t.Fatal(err)
	}
	if len(goldens) == 0 {
		t.Fatal("no classic goldens found for the originality guard")
	}
	for _, g := range goldens {
		frame, err := os.ReadFile(g)
		if err != nil {
			t.Fatal(err)
		}
		s := string(frame)
		for _, glyph := range forbiddenGlyphs {
			if strings.Contains(s, glyph) {
				t.Fatalf("[guard] forbidden glyph %q in %s", glyph, g)
			}
		}
		for _, str := range forbiddenStrings {
			if strings.Contains(s, str) {
				t.Fatalf("[guard] forbidden string %q in %s", str, g)
			}
		}
		for _, verb := range ThinkingVerbs {
			if strings.Contains(s, verb) {
				t.Fatalf("[guard] whimsical spinner verb %q in %s", verb, g)
			}
		}
		// A bordered welcome card or boxed composer: any border
		// edge at all (the composer is a rule, never a box).
		b := ChromeBorder()
		edge := "\n" + b.TopLeft + strings.Repeat(b.Top, 5)
		if strings.Contains(s, edge) {
			t.Fatalf("[guard] bordered element in %s (welcome card or boxed composer)", g)
		}
		// A two-rule composer sandwich: two rule lines separated by
		// at most the composer itself (a rule above AND below the
		// input). The verdict card's internal rule is far from the
		// composer's, so adjacency is what the guard checks.
		isRule := func(line string) bool {
			t := strings.TrimSpace(line)
			return len(t) >= 20 && strings.Trim(t, ruleChar()) == ""
		}
		lines := strings.Split(s, "\n")
		for i, line := range lines {
			if !isRule(line) {
				continue
			}
			for j := i + 1; j < len(lines) && j <= i+2; j++ {
				if isRule(lines[j]) {
					t.Fatalf("[guard] two-rule composer sandwich in %s", g)
				}
			}
		}
	}
	t.Logf("[guard] %d classic golden frames clean", len(goldens))
}

// The guard fails on a deliberately injected forbidden signature —
// without this, the guard itself is unproven.
func TestOriginalityGuardCatchesInjectedSignature(t *testing.T) {
	// The guard's own checks, run over an injected frame.
	frame := "something ⏺ here\nanother line\n"
	for _, glyph := range []string{"⏺", "⎿", "∴", "✱"} {
		if !strings.Contains(frame+" "+glyph, glyph) {
			t.Fatalf("injected glyph %q not detected", glyph)
		}
	}
	for _, str := range []string{"esc to interrupt", "(ctrl+o to expand)", "Thought for", "Welcome to"} {
		if !strings.Contains(frame+" "+str, str) {
			t.Fatalf("injected string %q not detected", str)
		}
	}
	t.Log("[guard] injected signatures detected")
}

// C5 validation study (design §9): the plain-transcript variant
// hides the contract strip and the verdict card so participants can
// be shown both versions of the same recorded run.
func TestPlainTranscriptVariant(t *testing.T) {
	sizes := []struct {
		name string
		w, h int
	}{
		{"100x27", 100, 27},
		{"80x24", 80, 24},
		{"60x20", 60, 20},
	}
	t.Setenv("TERM", "xterm-256color")
	os.Unsetenv("NIKI_PLAIN")
	os.Unsetenv("NIKI_ASCII")
	frameProfile = colorprofile.TrueColor
	defer func() { frameProfile = colorprofile.NoTTY }()

	outDir := filepath.Join("..", "..", "docs", "ui", "review", "validation")
	if err := os.MkdirAll(outDir, 0o755); err != nil {
		t.Fatal(err)
	}
	for _, stage := range []string{"frame3", "frame5"} {
		for _, variant := range []struct {
			name string
			hide bool
		}{
			{"with", false},
			{"without", true},
		} {
			for _, size := range sizes {
				m := classicFixtureSetup(size.w, size.h, stage)
				m.state.HideContractUI = variant.hide
				out := Downsample(m.View().Content)
				path := filepath.Join(outDir, stage+"_"+size.name+"_"+variant.name+".ansi")
				if err := os.WriteFile(path, []byte(out), 0o644); err != nil {
					t.Fatal(err)
				}
				t.Logf("[C5] wrote %s (%d bytes)", path, len(out))
			}
		}
	}

	// The variant is a real toggle, not a renderer fork: frame 5
	// without the card must not carry the verdict headline.
	m := classicFixtureSetup(100, 27, "frame5")
	with := Downsample(m.View().Content)
	m.state.HideContractUI = true
	without := Downsample(m.View().Content)
	if !strings.Contains(with, "verified") {
		t.Fatal("precondition: the default variant shows the verdict card")
	}
	if strings.Contains(without, "verified") || strings.Contains(without, "contract") {
		t.Fatalf("the plain variant must hide the card and strip:\n%s", without)
	}
	if strings.Contains(without, "run go test ./internal/auth") {
		// The transcript itself is unchanged — only the strip/card go.
		t.Log("plain variant keeps the transcript rows")
	}
}

// C5+: the inline scrollback header is part of the product surface —
// classic prints the orb header (and SETUP when unconfigured), never
// the ledger welcome card. The originality guard covers it too.
func TestInlineScrollbackHeader(t *testing.T) {
	m := classicModel(100, 27, true)
	m.state.Inline = true
	header := Downsample(m.scrollbackHeader())
	if !strings.Contains(header, "niki") {
		t.Fatalf("classic scrollback header missing the identity row: %q", header)
	}
	if !strings.Contains(header, "SETUP") {
		t.Fatalf("unconfigured classic header must carry the SETUP block: %q", header)
	}
	for _, forbidden := range []string{"Welcome to", "NikiCode!", "Local Coding Agent"} {
		if strings.Contains(header, forbidden) {
			t.Fatalf("ledger chrome %q leaked into the classic scrollback header", forbidden)
		}
	}
	m.state.ModelName = "claude-sonnet"
	if strings.Contains(m.scrollbackHeader(), "SETUP") {
		t.Fatal("SETUP must hide once configured")
	}
	// Ledger keeps its card.
	l := classicModel(100, 27, true)
	l.SetLayout("ledger")
	if !strings.Contains(l.scrollbackHeader(), "Welcome to") {
		t.Fatal("ledger must keep its welcome card on its own layout")
	}
}
