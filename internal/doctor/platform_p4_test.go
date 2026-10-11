package doctor

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

// P4: native Windows is explicitly labelled unsupported in the
// README and in `niki doctor`'s platform probe file; WSL is the
// documented path. (No Windows CI exists; the label is the
// accepted alternative.)
func TestWindowsUnsupportedLabelled(t *testing.T) {
	_, this, _, _ := runtime.Caller(0)
	root := filepath.Dir(filepath.Dir(filepath.Dir(this)))
	read := func(rel string) string {
		data, err := os.ReadFile(filepath.Join(root, rel))
		if err != nil {
			t.Fatalf("read %s: %v", rel, err)
		}
		return string(data)
	}
	readme := read("README.md")
	if !strings.Contains(readme, "Native Windows is explicitly unsupported") || !strings.Contains(readme, "WSL") {
		t.Fatalf("[P4] README must label native Windows unsupported with the WSL path")
	}
	ps1 := read("install.ps1")
	if !strings.Contains(strings.ToUpper(ps1), "UNSUPPORTED") {
		t.Fatalf("[P4] install.ps1 must carry the unsupported label")
	}
	probe := read("internal/doctor/statfs_windows.go")
	if !strings.Contains(strings.ToLower(probe), "unsupported platform") {
		t.Fatalf("[P4] windows probe must name the unsupported platform")
	}
	t.Logf("[P4] native Windows labelled unsupported (README + install.ps1 + doctor probe); WSL documented")
}
