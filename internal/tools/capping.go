package tools

import (
	"crypto/rand"
	"encoding/hex"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"
)

const (
	// MaxOutputChars is the threshold beyond which tool output is persisted to disk and previewed.
	MaxOutputChars = 50000
	// MaxLineLength is the maximum length of a single line before being split.
	MaxLineLength = 2000
	// PreviewByteCap is the maximum size of the inline preview.
	PreviewByteCap = 2048
)

// SplitLongLines splits any lines longer than maxLen with newlines.
func SplitLongLines(s string, maxLen int) string {
	if maxLen <= 0 {
		maxLen = MaxLineLength
	}
	lines := strings.Split(s, "\n")
	var result []string
	modified := false

	for _, line := range lines {
		if len(line) <= maxLen {
			result = append(result, line)
			continue
		}
		modified = true
		for len(line) > maxLen {
			result = append(result, line[:maxLen])
			line = line[maxLen:]
		}
		if len(line) > 0 {
			result = append(result, line)
		}
	}

	if !modified {
		return s
	}
	return strings.Join(result, "\n")
}

// ToolOptsOutFromCapping returns true for tools that manage their own output limits (e.g., read_file).
func ToolOptsOutFromCapping(toolName string) bool {
	return toolName == "read_file"
}

// CapOutput applies line splitting and bounds output to 50k characters.
// If output exceeds 50k characters, it persists the full output to disk
// and returns a ~2KB preview cut cleanly at a newline boundary.
func CapOutput(toolName string, output string, sessionDir string) (string, bool) {
	output = SplitLongLines(output, MaxLineLength)

	if ToolOptsOutFromCapping(toolName) || len(output) <= MaxOutputChars {
		return output, false
	}

	outDir := sessionDir
	if outDir == "" {
		home, err := os.UserHomeDir()
		if err == nil {
			outDir = filepath.Join(home, ".niki", "cache", "tool-results")
		} else {
			outDir = filepath.Join(os.TempDir(), "niki-tool-results")
		}
	} else {
		outDir = filepath.Join(outDir, "tool-results")
	}

	_ = os.MkdirAll(outDir, 0o700)

	var randomBytes [6]byte
	_, _ = rand.Read(randomBytes[:])
	fileName := fmt.Sprintf("%s-%d-%s.txt", toolName, time.Now().Unix(), hex.EncodeToString(randomBytes[:]))
	fullPath := filepath.Join(outDir, fileName)

	_ = os.WriteFile(fullPath, []byte(output), 0o600)

	previewLen := PreviewByteCap
	if previewLen > len(output) {
		previewLen = len(output)
	}

	preview := output[:previewLen]
	lastNL := strings.LastIndex(preview, "\n")
	if lastNL > 0 {
		preview = preview[:lastNL]
	}

	cappedMsg := fmt.Sprintf("\n... [output capped: %d characters exceed %d limit. Preview shown. Full output saved to: %s]",
		len(output), MaxOutputChars, fullPath)

	return preview + cappedMsg, true
}
