package tools

import (
	"context"
	"strings"
	"testing"
)

// Binary target (NUL in the first 8 KiB): text tools refuse and the
// bytes stay untouched.
func TestEditRefusesBinaryFile(t *testing.T) {
	p := t.TempDir() + "/blob.bin"
	bin := append([]byte{0x7f, 'E', 'L', 'F'}, make([]byte, 100)...)
	bin = append(bin, 0x00, 0x01, 0x02)
	write(t, p, string(bin))
	before := readBytes(t, p)

	res := runEdit(t, p, "\x00\x01", "\x00\x02", false)
	if !res.IsError || !strings.Contains(res.Output, "binary") {
		t.Fatalf("[E6] binary edit not refused: %+v", res)
	}
	if string(readBytes(t, p)) != string(before) {
		t.Fatalf("[E6] binary file was modified")
	}
	t.Logf("[E6 binary] edit refused: %s", res.Output)
}

// UTF-16 target: refused as non-text, bytes untouched.
func TestEditRefusesUTF16File(t *testing.T) {
	p := t.TempDir() + "/u16.txt"
	u16 := []byte{0xFF, 0xFE, 'h', 0x00, 'i', 0x00, 0x00, 0x00}
	writeBytes(t, p, u16)

	res := runEdit(t, p, "hi", "ho", false)
	if !res.IsError || !strings.Contains(res.Output, "binary") {
		t.Fatalf("[E6] UTF-16 edit not refused: %+v", res)
	}
	if string(readBytes(t, p)) != string(u16) {
		t.Fatalf("[E6] UTF-16 file bytes changed")
	}
	t.Logf("[E6 utf16] edit refused: %s", res.Output)
}

// apply_patch on a binary target: refused, bytes untouched.
func TestApplyPatchRefusesBinaryFile(t *testing.T) {
	p := t.TempDir() + "/b.bin"
	bin := make([]byte, 64)
	bin[10] = 0x00
	writeBytes(t, p, bin)
	before := readBytes(t, p)

	tool := NewApplyPatchTool()
	args, _ := jsonMarshal(map[string]any{"path": p, "patch": "--- a/x\n+++ b/x\n@@ -1,1 +1,1 @@\n-a\n+b\n"})
	res, err := tool.Run(context.Background(), args)
	if err != nil {
		t.Fatal(err)
	}
	if !res.IsError || !strings.Contains(res.Output, "binary") {
		t.Fatalf("[E6] binary patch not refused: %+v", res)
	}
	if string(readBytes(t, p)) != string(before) {
		t.Fatalf("[E6] binary file was modified by patch")
	}
	t.Logf("[E6 binary-patch] %s", res.Output)
}

// Large text file beyond the cap: refused.
func TestEditRefusesOversizeFile(t *testing.T) {
	p := t.TempDir() + "/big.txt"
	write(t, p, strings.Repeat("a", maxTextEditBytes+1))
	res := runEdit(t, p, "aaa", "bbb", false)
	if !res.IsError || !strings.Contains(res.Output, "too large") {
		t.Fatalf("[E6] oversize edit not refused: %+v", res)
	}
	t.Logf("[E6 oversize] %s", res.Output)
}

// CRLF round-trip: an edit applies and every \r\n is preserved.
func TestEditCRLFRoundTrip(t *testing.T) {
	p := t.TempDir() + "/crlf.txt"
	orig := "first\r\nsecond\r\nthird\r\n"
	write(t, p, orig)

	res := runEdit(t, p, "second\r\nthird", "SECOND\r\nthird", false)
	if res.IsError {
		t.Fatalf("[E6] CRLF edit failed: %s", res.Output)
	}
	data := read(t, p)
	// Every \n must be preceded by \r: the count check below (bare
	// LF would make \n outnumber \r\n) is the real assertion.
	if got, want := strings.Count(data, "\r\n"), strings.Count(orig, "\r\n"); got != want {
		t.Fatalf("[E6] CRLF count changed: got %d want %d (%q)", got, want, data)
	}
	if strings.Count(data, "\n") != strings.Count(data, "\r\n") {
		t.Fatalf("[E6] bare LF introduced: %q", data)
	}
	if !strings.Contains(data, "SECOND") {
		t.Fatalf("[E6] CRLF edit not applied: %q", data)
	}
	t.Logf("[E6 crlf] %d CRLF pairs preserved, no bare LF", strings.Count(data, "\r\n"))
}

// UTF-16 round-trip via write_file over an existing text file is fine,
// but over an existing UTF-16/binary file the text tool refuses.
func TestWriteFileRefusesOverwriteBinary(t *testing.T) {
	p := t.TempDir() + "/img.png"
	bin := []byte{0x89, 'P', 'N', 'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00}
	writeBytes(t, p, bin)
	before := readBytes(t, p)

	tool := NewWriteFileTool()
	args, _ := jsonMarshal(map[string]any{"path": p, "content": "not a png"})
	res, err := tool.Run(context.Background(), args)
	if err != nil {
		t.Fatal(err)
	}
	if !res.IsError || !strings.Contains(res.Output, "binary") {
		t.Fatalf("[E6] write over binary not refused: %+v", res)
	}
	if string(readBytes(t, p)) != string(before) {
		t.Fatalf("[E6] binary target clobbered")
	}
	t.Logf("[E6 write-binary] %s", res.Output)
}
