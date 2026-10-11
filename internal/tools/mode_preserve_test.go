package tools

import (
	"context"
	"os"
	"testing"
)

// N: text tools must not clobber permission bits (edit_file used to
// turn a 0600 secret into 0644; write_file created 0600 files).
func TestEditFilePreservesPermissions(t *testing.T) {
	dir := t.TempDir()
	p := dir + "/secret.txt"
	write(t, p, "before-content")
	if err := os.Chmod(p, 0o600); err != nil {
		t.Fatal(err)
	}
	args, jerr := jsonMarshal(map[string]any{
		"path": p, "old_string": "before-content", "new_string": "after-content",
	})
	if jerr != nil {
		t.Fatal(jerr)
	}
	if _, err := NewEditFileTool().Run(context.Background(), args); err != nil {
		t.Fatal(err)
	}
	fi, err := os.Stat(p)
	if err != nil {
		t.Fatal(err)
	}
	if got := fi.Mode().Perm(); got != 0o600 {
		t.Fatalf("mode after edit = %o, want 600", got)
	}
	t.Logf("[N] edit_file preserved mode 600")
}

func TestWriteFileModes(t *testing.T) {
	dir := t.TempDir()
	p := dir + "/cfg.txt"
	write(t, p, "v1")
	if err := os.Chmod(p, 0o755); err != nil {
		t.Fatal(err)
	}
	w := NewWriteFileTool()
	av, jerr := jsonMarshal(map[string]any{"path": p, "content": "v2"})
	if jerr != nil {
		t.Fatal(jerr)
	}
	if _, err := w.Run(context.Background(), av); err != nil {
		t.Fatal(err)
	}
	fi, err := os.Stat(p)
	if err != nil {
		t.Fatal(err)
	}
	if got := fi.Mode().Perm(); got != 0o755 {
		t.Fatalf("overwrite mode = %o, want 755", got)
	}

	n := dir + "/fresh.txt"
	nv, jerr := jsonMarshal(map[string]any{"path": n, "content": "hi"})
	if jerr != nil {
		t.Fatal(jerr)
	}
	if _, err := w.Run(context.Background(), nv); err != nil {
		t.Fatal(err)
	}
	fi, err = os.Stat(n)
	if err != nil {
		t.Fatal(err)
	}
	if got := fi.Mode().Perm(); got != 0o644 {
		t.Fatalf("new-file mode = %o, want 644", got)
	}
	t.Logf("[N] write_file preserved 755 on overwrite, 644 for new file")
}
