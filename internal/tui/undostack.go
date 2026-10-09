package tui

// UndoSnapshot captures the state of the composer input at an edit boundary.
type UndoSnapshot struct {
	Value  string
	Cursor int
}

// UndoStack manages undo and redo histories for the composer.
type UndoStack struct {
	past   []UndoSnapshot
	future []UndoSnapshot
}

func NewUndoStack() *UndoStack {
	return &UndoStack{
		past:   make([]UndoSnapshot, 0, 50),
		future: make([]UndoSnapshot, 0, 50),
	}
}

// Push records a new snapshot. Clears the redo future.
func (u *UndoStack) Push(val string, cursor int) {
	if len(u.past) > 0 {
		last := u.past[len(u.past)-1]
		if last.Value == val {
			return
		}
	}
	u.past = append(u.past, UndoSnapshot{Value: val, Cursor: cursor})
	u.future = nil
}

// Undo steps back to the previous snapshot, returning it.
func (u *UndoStack) Undo(currentVal string, currentCursor int) (UndoSnapshot, bool) {
	if len(u.past) == 0 {
		return UndoSnapshot{}, false
	}
	snap := u.past[len(u.past)-1]
	u.past = u.past[:len(u.past)-1]
	u.future = append(u.future, UndoSnapshot{Value: currentVal, Cursor: currentCursor})
	return snap, true
}

// Redo steps forward to a reverted snapshot, returning it.
func (u *UndoStack) Redo(currentVal string, currentCursor int) (UndoSnapshot, bool) {
	if len(u.future) == 0 {
		return UndoSnapshot{}, false
	}
	snap := u.future[len(u.future)-1]
	u.future = u.future[:len(u.future)-1]
	u.past = append(u.past, UndoSnapshot{Value: currentVal, Cursor: currentCursor})
	return snap, true
}
