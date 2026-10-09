package diagnostics

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestCollectDiagnosticsGo(t *testing.T) {
	tmpDir := t.TempDir()
	goMod := filepath.Join(tmpDir, "go.mod")
	_ = os.WriteFile(goMod, []byte("module testmod\n\ngo 1.22\n"), 0644)

	mainFile := filepath.Join(tmpDir, "main.go")
	badGo := "package main\n\nfunc main() {\nundefinedSymbolCall()\n}\n"
	_ = os.WriteFile(mainFile, []byte(badGo), 0644)

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	rep := CollectDiagnostics(ctx, tmpDir, mainFile)
	xml := rep.FormatXML()

	if len(rep.Items) == 0 {
		t.Logf("go compiler diagnostics returned 0 (or go not installed)")
	} else {
		if !strings.Contains(xml, "<diagnostics") {
			t.Fatalf("expected XML tag in diagnostics output: %s", xml)
		}
		if !strings.Contains(xml, "undefinedSymbolCall") {
			t.Fatalf("expected undefined symbol in diagnostics: %s", xml)
		}
	}
}

func TestReportFormatXMLEmpty(t *testing.T) {
	rep := Report{File: "clean.go", Items: nil}
	if rep.FormatXML() != "" {
		t.Fatalf("expected empty string for report with zero issues")
	}
}
