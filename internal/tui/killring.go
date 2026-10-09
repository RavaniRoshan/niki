package tui

// KillRing implements an Emacs-style kill/yank ring buffer for
// cut/copy/deleted text chunks in the composer.
type KillRing struct {
	entries []string
	index   int
}

func NewKillRing(capacity int) *KillRing {
	if capacity <= 0 {
		capacity = 32
	}
	return &KillRing{entries: make([]string, 0, capacity)}
}

// Push adds a killed string to the ring.
// If accumulate is true, consecutive kills accumulate into the latest entry:
// if prepend is true (backward kill), it prepends; otherwise (forward kill), it appends.
func (k *KillRing) Push(text string, prepend, accumulate bool) {
	if text == "" {
		return
	}
	if accumulate && len(k.entries) > 0 {
		if prepend {
			k.entries[len(k.entries)-1] = text + k.entries[len(k.entries)-1]
		} else {
			k.entries[len(k.entries)-1] = k.entries[len(k.entries)-1] + text
		}
		k.index = len(k.entries) - 1
		return
	}
	k.entries = append(k.entries, text)
	k.index = len(k.entries) - 1
}

// Yank returns the latest killed string.
func (k *KillRing) Yank() string {
	if len(k.entries) == 0 {
		return ""
	}
	if k.index < 0 || k.index >= len(k.entries) {
		k.index = len(k.entries) - 1
	}
	return k.entries[k.index]
}

// YankPop rotates backwards through the kill ring.
func (k *KillRing) YankPop() string {
	if len(k.entries) <= 1 {
		return k.Yank()
	}
	k.index--
	if k.index < 0 {
		k.index = len(k.entries) - 1
	}
	return k.entries[k.index]
}

// Len returns the number of entries in the ring.
func (k *KillRing) Len() int {
	return len(k.entries)
}
