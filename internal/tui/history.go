package tui

import "github.com/RavaniRoshan/niki/internal/tools"

// HistoryCell is a semantic unit of the transcript.
type HistoryCell struct {
	Role string // user, assistant, system, tool, error
	Text string
	// Kind further types a cell: "tool" (one-line classic tool
	// row), "tool_excerpt" (failure excerpt under a tool row),
	// or "loop" (retry/repetition row). Empty means the role
	// decides the rendering (C2).
	Kind string
	// CallID links tool cells to their engine events so a
	// completion updates its own row (C2).
	CallID string
	// State is the row's real state: "running", "done", "failed"
	// (set only by events; the glyph is rendered from it).
	State string
	// Verb is the fixed-list action verb (C2).
	Verb string
	// Target is the tool's real argument (path, command, pattern).
	Target string
	// Result is the real outcome summary, right-aligned on the row.
	Result string
	// Excerpt holds the failing output lines for a tool_excerpt cell.
	Excerpt []string
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
	// X3: the visible transcript never retains credential text.
	h.Cells = append(h.Cells, HistoryCell{Role: role, Text: tools.Redact(text)})
}

// AppendDelta appends to the last assistant
// cell if it is still in the live region.
func (h *History) AppendDelta(text string) {
	// X3: streamed text is scrubbed too (deltas can carry secrets).
	text = tools.Redact(text)
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

// OpenToolCell finds the running tool cell for a
// call id, searching from the end (a call id maps to
// one row). Returns nil when no open row exists.
func (h *History) OpenToolCell(callID string) *HistoryCell {
	if callID == "" {
		return nil
	}
	for i := len(h.Cells) - 1; i >= h.Committed; i-- {
		c := &h.Cells[i]
		if c.Kind == "tool" && c.CallID == callID && c.State == stateRunning {
			return c
		}
	}
	return nil
}
