package memory

import (
	"os"
	"path/filepath"
	"testing"
)

// C8: a user-owned memory file, written only by explicit
// `remember`, shown and editable. Nothing is saved silently.
func TestMemoryWrittenOnlyByExplicitRemember(t *testing.T) {
	dir := t.TempDir()
	st, err := NewStore(dir)
	if err != nil {
		t.Fatal(err)
	}
	// Read-only activity: retrieves, failed retrieves, listing.
	_, _ = st.Retrieve("anything", 3)
	_, _ = st.Retrieve("nothing here", 3)
	if err := st.Show(""); err != nil {
		t.Fatal(err)
	}
	// Nothing but the store skeleton may exist: no topic files.
	entries, err := os.ReadDir(filepath.Join(dir, "topics"))
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 0 {
		t.Fatalf("[C8] silent write: %v", entries)
	}
	t.Logf("[C8] reads never write (0 topic files)")

	// Explicit remember stores, shows, and forgets.
	if err := st.AddFact("prefs", "dark mode please"); err != nil {
		t.Fatal(err)
	}
	got, err := st.Retrieve("dark mode", 3)
	if err != nil || len(got) == 0 {
		t.Fatalf("[C8] stored fact not retrievable: %v %v", got, err)
	}
	if err := st.Forget("prefs"); err != nil {
		t.Fatal(err)
	}
	entries, _ = os.ReadDir(filepath.Join(dir, "topics"))
	if len(entries) != 0 {
		t.Fatalf("[C8] forget left residue: %v", entries)
	}
	t.Logf("[C8] explicit remember stores + retrieves; forget removes")
}
