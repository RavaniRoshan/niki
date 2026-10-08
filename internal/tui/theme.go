package tui

import "github.com/charmbracelet/lipgloss"

type Theme struct {
	Header         lipgloss.Style
	Assistant      lipgloss.Style
	Success        lipgloss.Style
	Error          lipgloss.Style
	Muted          lipgloss.Style
	User           lipgloss.Style
	Accent         lipgloss.Style
	CardBorder     lipgloss.Style
	ComposerBorder lipgloss.Style
	BadgePerm      lipgloss.Style
	BadgeMode      lipgloss.Style
	BadgeModel     lipgloss.Style
	ToolBullet     lipgloss.Style
	ToolTree       lipgloss.Style
	ToolName       lipgloss.Style
	ToolDetail     lipgloss.Style
	Thinking       lipgloss.Style
	UserPrompt     lipgloss.Style
	UserText       lipgloss.Style
	CardTitle      lipgloss.Style
	CardSubtitle   lipgloss.Style
	CardDesc       lipgloss.Style
	CardLabel      lipgloss.Style
	CardValue      lipgloss.Style
	AnnounceIcon   lipgloss.Style
	AnnounceTitle  lipgloss.Style
	AnnounceDesc   lipgloss.Style
	AnnounceLink   lipgloss.Style
	MascotTop      lipgloss.Style
	MascotMid      lipgloss.Style
	MascotBot      lipgloss.Style
	PromptPrefix   lipgloss.Style
	InputText      lipgloss.Style
	Placeholder    lipgloss.Style
	ActivityGlyph  lipgloss.Style
	ActivityText   lipgloss.Style
	ActivityHint   lipgloss.Style
	StatusThinking lipgloss.Style
	StatusDir      lipgloss.Style
	StatusGit      lipgloss.Style
	StatusHints    lipgloss.Style
	StatusMeter    lipgloss.Style
}

func NewDefaultTheme() Theme {
	cyan := lipgloss.Color("#38bdf8")
	dark := lipgloss.Color("#0f172a")
	white := lipgloss.Color("#ffffff")
	brightWhite := lipgloss.Color("#f8fafc")
	textWhite := lipgloss.Color("#f1f5f9")
	slateLight := lipgloss.Color("#cbd5e1")
	slateDim := lipgloss.Color("#94a3b8")
	slateMuted := lipgloss.Color("#64748b")
	borderGray := lipgloss.Color("#475569")
	green := lipgloss.Color("#22c55e")
	red := lipgloss.Color("#ef4444")
	amber := lipgloss.Color("#f59e0b")
	gold := lipgloss.Color("#fbbf24")
	blue := lipgloss.Color("#60a5fa")
	orange := lipgloss.Color("#f97316")

	return Theme{
		Header:         lipgloss.NewStyle().Bold(true).Foreground(cyan),
		Assistant:      lipgloss.NewStyle().Foreground(textWhite),
		Success:        lipgloss.NewStyle().Foreground(green),
		Error:          lipgloss.NewStyle().Foreground(red),
		Muted:          lipgloss.NewStyle().Foreground(slateMuted),
		User:           lipgloss.NewStyle().Bold(true).Foreground(gold),
		Accent:         lipgloss.NewStyle().Foreground(cyan),
		CardBorder:     lipgloss.NewStyle().Foreground(cyan),
		ComposerBorder: lipgloss.NewStyle().Foreground(borderGray),
		BadgePerm:      lipgloss.NewStyle().Bold(true).Foreground(amber),
		BadgeMode:      lipgloss.NewStyle().Bold(true).Foreground(blue),
		BadgeModel:     lipgloss.NewStyle().Foreground(slateLight),
		ToolBullet:     lipgloss.NewStyle().Foreground(green),
		ToolTree:       lipgloss.NewStyle().Foreground(slateMuted),
		ToolName:       lipgloss.NewStyle().Bold(true).Foreground(textWhite),
		ToolDetail:     lipgloss.NewStyle().Foreground(slateDim),
		Thinking:       lipgloss.NewStyle().Italic(true).Foreground(slateDim),
		UserPrompt:     lipgloss.NewStyle().Bold(true).Foreground(gold),
		UserText:       lipgloss.NewStyle().Bold(true).Foreground(brightWhite),
		CardTitle:      lipgloss.NewStyle().Bold(true).Foreground(cyan),
		CardSubtitle:   lipgloss.NewStyle().Foreground(slateDim),
		CardDesc:       lipgloss.NewStyle().Foreground(slateMuted),
		CardLabel:      lipgloss.NewStyle().Foreground(slateMuted),
		CardValue:      lipgloss.NewStyle().Foreground(textWhite),
		AnnounceIcon:   lipgloss.NewStyle().Foreground(cyan),
		AnnounceTitle:  lipgloss.NewStyle().Bold(true).Foreground(white),
		AnnounceDesc:   lipgloss.NewStyle().Foreground(slateLight),
		AnnounceLink:   lipgloss.NewStyle().Foreground(slateMuted),
		MascotTop:      lipgloss.NewStyle().Background(cyan),
		MascotMid:      lipgloss.NewStyle().Background(cyan).Foreground(dark).Bold(true),
		MascotBot:      lipgloss.NewStyle().Background(cyan),
		PromptPrefix:   lipgloss.NewStyle().Bold(true).Foreground(textWhite),
		InputText:      lipgloss.NewStyle().Foreground(brightWhite),
		Placeholder:    lipgloss.NewStyle().Foreground(borderGray),
		ActivityGlyph:  lipgloss.NewStyle().Bold(true).Foreground(orange),
		ActivityText:   lipgloss.NewStyle().Foreground(orange),
		ActivityHint:   lipgloss.NewStyle().Foreground(slateMuted),
		StatusThinking: lipgloss.NewStyle().Foreground(slateDim),
		StatusDir:      lipgloss.NewStyle().Foreground(slateMuted),
		StatusGit:      lipgloss.NewStyle().Foreground(slateMuted),
		StatusHints:    lipgloss.NewStyle().Foreground(slateMuted),
	}
}

// SelectTheme returns a Theme based on the requested name (default, dark, light, monochrome).
func SelectTheme(name string) Theme {
	th := NewDefaultTheme()
	switch name {
	case "monochrome":
		mono := lipgloss.NewStyle()
		th.Header = mono.Bold(true)
		th.Accent = mono
		th.User = mono.Bold(true)
		th.ToolBullet = mono
		th.ActivityGlyph = mono
	case "light", "dark", "default":
		// Standard palette
	}
	return th
}
