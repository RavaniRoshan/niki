package tui

// HistoryCell is a semantic unit of the transcript.
type HistoryCell struct {
	Role string // user, assistant, system, tool, error
	Text string
}

// History accumulates transcript cells.
type History struct {
	Cells []HistoryCell
}

func (h *History) Append(role, text string) {
	h.Cells = append(h.Cells, HistoryCell{Role: role, Text: text})
}

// AppendDelta appends to the last assistant cell if streaming.
func (h *History) AppendDelta(text string) {
	if len(h.Cells) > 0 && h.Cells[len(h.Cells)-1].Role == "assistant" {
		h.Cells[len(h.Cells)-1].Text += text
		return
	}
	h.Cells = append(h.Cells, HistoryCell{Role: "assistant", Text: text})
}
