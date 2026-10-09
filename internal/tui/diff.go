package tui

import (
	"fmt"
	"strings"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

// DiffHunk holds a single hunk in a unified diff.
type DiffHunk struct {
	Header string
	Lines  []string
}

// DiffFile represents a modified file with git-style stats and hunks.
type DiffFile struct {
	Path      string
	Additions int
	Deletions int
	Hunks     []DiffHunk
	RawDiff   string
}

// DiffViewerState manages the interactive diff viewer session.
type DiffViewerState struct {
	Active       bool
	Files        []DiffFile
	Selected     int
	SelectedHunk int
	ScrollOffset int
}

// NewDiffViewerState initializes a diff viewer state with parsed files.
func NewDiffViewerState(files []DiffFile) DiffViewerState {
	return DiffViewerState{
		Active:   len(files) > 0,
		Files:    files,
		Selected: 0,
	}
}

// ParseUnifiedDiff parses a raw unified diff output into structured DiffFiles.
func ParseUnifiedDiff(raw string) []DiffFile {
	var files []DiffFile
	lines := strings.Split(raw, "\n")
	var curFile *DiffFile
	var curHunk *DiffHunk

	for _, line := range lines {
		if strings.HasPrefix(line, "diff --git ") {
			if curFile != nil {
				if curHunk != nil {
					curFile.Hunks = append(curFile.Hunks, *curHunk)
					curHunk = nil
				}
				files = append(files, *curFile)
			}
			parts := strings.Fields(line)
			name := "file"
			if len(parts) >= 4 {
				name = strings.TrimPrefix(parts[3], "b/")
			}
			curFile = &DiffFile{Path: name, RawDiff: line}
		} else if strings.HasPrefix(line, "@@") {
			if curFile != nil {
				if curHunk != nil {
					curFile.Hunks = append(curFile.Hunks, *curHunk)
				}
				curHunk = &DiffHunk{Header: line}
			}
		} else if curHunk != nil {
			curHunk.Lines = append(curHunk.Lines, line)
			if curFile != nil {
				if strings.HasPrefix(line, "+") {
					curFile.Additions++
				} else if strings.HasPrefix(line, "-") {
					curFile.Deletions++
				}
			}
		}
	}

	if curFile != nil {
		if curHunk != nil {
			curFile.Hunks = append(curFile.Hunks, *curHunk)
		}
		files = append(files, *curFile)
	}

	return files
}

// HandleKey handles keyboard navigation in the diff viewer.
func (d *DiffViewerState) HandleKey(msg tea.KeyMsg) (closed bool) {
	if !d.Active || len(d.Files) == 0 {
		return false
	}

	switch msg.Type {
	case tea.KeyEsc:
		d.Active = false
		return true

	case tea.KeyUp, tea.KeyCtrlP:
		if d.Selected > 0 {
			d.Selected--
			d.SelectedHunk = 0
			d.ScrollOffset = 0
		}
		return false

	case tea.KeyDown, tea.KeyCtrlN:
		if d.Selected < len(d.Files)-1 {
			d.Selected++
			d.SelectedHunk = 0
			d.ScrollOffset = 0
		}
		return false

	case tea.KeyPgUp:
		if d.ScrollOffset > 5 {
			d.ScrollOffset -= 5
		} else {
			d.ScrollOffset = 0
		}
		return false

	case tea.KeyPgDown:
		d.ScrollOffset += 5
		return false

	default:
		switch msg.String() {
		case "q":
			d.Active = false
			return true
		case "j":
			if d.Selected < len(d.Files)-1 {
				d.Selected++
				d.SelectedHunk = 0
			}
		case "k":
			if d.Selected > 0 {
				d.Selected--
				d.SelectedHunk = 0
			}
		case "n":
			cur := d.Files[d.Selected]
			if d.SelectedHunk < len(cur.Hunks)-1 {
				d.SelectedHunk++
			}
		case "p":
			if d.SelectedHunk > 0 {
				d.SelectedHunk--
			}
		}
	}
	return false
}

// Render renders the interactive diff viewer layout.
func (d *DiffViewerState) Render(th Theme, width, height int) string {
	if !d.Active || len(d.Files) == 0 {
		return ""
	}

	boxWidth := width - 4
	if boxWidth < 50 {
		boxWidth = 50
	}

	cur := d.Files[d.Selected]

	// Left sidebar: files list
	sidebarWidth := 24
	if sidebarWidth > boxWidth/3 {
		sidebarWidth = boxWidth / 3
	}
	var fileList strings.Builder
	fileList.WriteString(th.CardTitle.Render("Modified Files") + "\n\n")
	for i, f := range d.Files {
		cursor := "  "
		if i == d.Selected {
			cursor = "❯ "
		}
		stat := fmt.Sprintf("+%d -%d", f.Additions, f.Deletions)
		name := f.Path
		if len(name) > sidebarWidth-10 {
			name = name[len(name)-(sidebarWidth-10):]
		}
		line := fmt.Sprintf("%s%s %s", cursor, name, stat)
		if i == d.Selected {
			fileList.WriteString(th.Accent.Render(line) + "\n")
		} else {
			fileList.WriteString(th.Muted.Render(line) + "\n")
		}
	}

	diffWidth := boxWidth - sidebarWidth - 4
	if diffWidth < 20 {
		diffWidth = 20
	}

	var diffContent strings.Builder
	diffContent.WriteString(th.CardTitle.Render(fmt.Sprintf("%s (+%d -%d)", cur.Path, cur.Additions, cur.Deletions)) + "\n\n")

	// Check if terminal width > 120 for split mode
	if width > 120 && diffWidth >= 80 {
		colWidth := (diffWidth - 3) / 2
		var leftCol, rightCol strings.Builder
		leftCol.WriteString(th.Muted.Render("--- Original") + "\n")
		rightCol.WriteString(th.Muted.Render("+++ Modified") + "\n")

		for _, h := range cur.Hunks {
			for _, l := range h.Lines {
				if strings.HasPrefix(l, "-") {
					leftCol.WriteString(th.Error.Render(truncate(l, colWidth)) + "\n")
				} else if strings.HasPrefix(l, "+") {
					rightCol.WriteString(th.Success.Render(truncate(l, colWidth)) + "\n")
				} else {
					leftCol.WriteString(th.Muted.Render(truncate(l, colWidth)) + "\n")
					rightCol.WriteString(th.Muted.Render(truncate(l, colWidth)) + "\n")
				}
			}
		}
		splitView := lipgloss.JoinHorizontal(lipgloss.Top,
			th.CardBorder.Width(colWidth).Render(leftCol.String()),
			" ",
			th.CardBorder.Width(colWidth).Render(rightCol.String()),
		)
		diffContent.WriteString(splitView)
	} else {
		// Unified single-column diff
		for _, h := range cur.Hunks {
			diffContent.WriteString(th.Accent.Render(h.Header) + "\n")
			for _, l := range h.Lines {
				if strings.HasPrefix(l, "+") {
					diffContent.WriteString(th.Success.Render(truncate(l, diffWidth)) + "\n")
				} else if strings.HasPrefix(l, "-") {
					diffContent.WriteString(th.Error.Render(truncate(l, diffWidth)) + "\n")
				} else {
					diffContent.WriteString(th.Muted.Render(truncate(l, diffWidth)) + "\n")
				}
			}
		}
	}

	diffContent.WriteString("\n" + th.Muted.Render("↑/↓/j/k file · n/p hunk · q/Esc return to chat"))

	mainLayout := lipgloss.JoinHorizontal(lipgloss.Top,
		th.CardBorder.Width(sidebarWidth).Render(fileList.String()),
		" ",
		th.CardBorder.Width(diffWidth).Render(diffContent.String()),
	)

	return lipgloss.PlaceHorizontal(width, lipgloss.Center, mainLayout)
}

func truncate(s string, max int) string {
	if len(s) > max && max > 3 {
		return s[:max-3] + "…"
	}
	return s
}
