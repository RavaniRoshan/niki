package tui

// HistoryCell is a semantic unit of the transcript.
type HistoryCell struct {
	Role string // user, assistant, system, tool, error
	Text string
}

// History accumulates transcript cells split at a
// committed boundary (U9). Cells before Committed
// are finalized history; cells from Committed onward
// are the live region that streams without
// re-rendering committed history.
type History struct {
	Cells     []HistoryCell
	Committed int
}

func (h *History) Append(role, text string) {
	h.Cells = append(h.Cells, HistoryCell{Role: role, Text: text})
}

// AppendDelta appends to the last assistant
// cell if it is still in the live region.
func (h *History) AppendDelta(text string) {
	if len(h.Cells) > h.Committed && h.Cells[len(h.Cells)-1].Role == "assistant" {
		h.Cells[len(h.Cells)-1].Text += text
		return
	}
	h.Cells = append(h.Cells, HistoryCell{Role: "assistant", Text: text})
}

// Live returns the cells after the committed
// boundary (the streaming region).
func (h *History) Live() []HistoryCell {
	if h.Committed > len(h.Cells) {
		return nil
	}
	return h.Cells[h.Committed:]
}

// Finalize marks every current cell as committed
// and returns them, so they can be flushed to
// native scrollback (U9).
func (h *History) Finalize() []HistoryCell {
	committed := h.Cells[h.Committed:]
	h.Committed = len(h.Cells)
	return committed
}
