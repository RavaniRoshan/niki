package tools

import (
	"bytes"
	"fmt"
)

const (
	// maxTextEditBytes caps what a text tool will edit (E6): larger
	// files are left to dedicated tools, never rewritten by text edits.
	maxTextEditBytes = 2 << 20 // 2 MiB
	// sniffBytes is the prefix scanned for NUL bytes / encoding BOMs.
	sniffBytes = 8192
)

var (
	utf32LEBOM = []byte{0xFF, 0xFE, 0x00, 0x00}
	utf32BEBOM = []byte{0x00, 0x00, 0xFE, 0xFF}
)

// nonTextReason reports why data must not be edited by a text tool
// (binary content or oversize), or "" when the target is safe.
func nonTextReason(data []byte) string {
	if len(data) > maxTextEditBytes {
		return fmt.Sprintf("file is too large for text tools (%d bytes, cap %d)", len(data), maxTextEditBytes)
	}
	if len(data) >= 4 && (bytes.Equal(data[:4], utf32LEBOM) || bytes.Equal(data[:4], utf32BEBOM)) {
		return "binary content (UTF-32 BOM)"
	}
	if len(data) >= 2 && ((data[0] == 0xFF && data[1] == 0xFE) || (data[0] == 0xFE && data[1] == 0xFF)) {
		return "binary content (UTF-16 BOM)"
	}
	n := len(data)
	if n > sniffBytes {
		n = sniffBytes
	}
	if bytes.IndexByte(data[:n], 0) >= 0 {
		return "binary content (NUL byte in first 8 KiB)"
	}
	return ""
}
