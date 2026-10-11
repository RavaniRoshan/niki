package tools

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/RavaniRoshan/niki/internal/format"
)

type EditFileTool struct {
	Base
}

func NewEditFileTool() *EditFileTool {
	return &EditFileTool{
		Base: Base{
			SchemaStr: `{"required":["path","old_string","new_string"],"fields":{"path":"string","old_string":"string","new_string":"string","expected_hash":"string","replace_all":"boolean"}}`,
		},
	}
}

func (t *EditFileTool) Name() string { return "edit_file" }
func (t *EditFileTool) Description() string {
	return "Replace exact string occurrences in a file with read-before-edit hash check and unified diff preview"
}

type editFileArgs struct {
	Path         string `json:"path"`
	OldString    string `json:"old_string"`
	NewString    string `json:"new_string"`
	ExpectedHash string `json:"expected_hash,omitempty"`
	ReplaceAll   bool   `json:"replace_all,omitempty"`
}

func generateSimpleDiff(path, oldText, newText string) string {
	var sb strings.Builder
	fmt.Fprintf(&sb, "--- a/%s\n+++ b/%s\n@@ -1 +1 @@\n", path, path)
	for _, l := range strings.Split(oldText, "\n") {
		sb.WriteString("-" + l + "\n")
	}
	for _, l := range strings.Split(newText, "\n") {
		sb.WriteString("+" + l + "\n")
	}
	return strings.TrimRight(sb.String(), "\n")
}

func (t *EditFileTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a editFileArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}

	data, err := os.ReadFile(a.Path)
	if err != nil {
		return ToolResult{Output: err.Error(), IsError: true}, nil
	}
	if reason := nonTextReason(data); reason != "" {
		return ToolResult{Output: "refusing text edit: " + reason, IsError: true}, nil
	}

	currentHash := sha256.Sum256(data)
	hexHash := hex.EncodeToString(currentHash[:])

	if a.ExpectedHash != "" && !strings.EqualFold(a.ExpectedHash, hexHash) && !strings.HasPrefix(hexHash, strings.ToLower(a.ExpectedHash)) {
		return ToolResult{
			Output:  fmt.Sprintf("read-before-edit hash mismatch: file %s hash is %s, expected %s", a.Path, hexHash, a.ExpectedHash),
			IsError: true,
		}, nil
	}

	fileContent := string(data)
	n := strings.Count(fileContent, a.OldString)
	if n > 1 && !a.ReplaceAll {
		return ToolResult{
			Output:  fmt.Sprintf("expected exactly 1 occurrence, found %d (set replace_all: true to replace all instances)", n),
			IsError: true,
		}, nil
	}

	var updated, fuzzyNote string
	if n == 0 {
		// E1: exact text absent — allow a whitespace-normalized
		// fallback only when it is unique; ambiguous or missing
		// matches are rejected, never guessed. The confidence of a
		// fuzzy hit is logged in the output.
		fz, lineNo, conf, applied, reason := fuzzyReplace(fileContent, a.OldString, a.NewString)
		if !applied {
			if reason != "" {
				return ToolResult{Output: fmt.Sprintf("edit rejected: %s", reason), IsError: true}, nil
			}
			return ToolResult{Output: fmt.Sprintf("target old_string not found in %s", a.Path), IsError: true}, nil
		}
		updated = fz
		fuzzyNote = fmt.Sprintf(", fuzzy match confidence=%.2f at line %d", conf, lineNo)
	} else if a.ReplaceAll {
		updated = strings.ReplaceAll(fileContent, a.OldString, a.NewString)
	} else {
		updated = strings.Replace(fileContent, a.OldString, a.NewString, 1)
	}

	// Preserve the target's permission bits across the atomic
	// replace (N: an edit must not turn a 0600 secret into 0644).
	origMode := os.FileMode(0o644)
	if fi, serr := os.Stat(a.Path); serr == nil {
		origMode = fi.Mode().Perm()
	}
	// Atomic replace: write a temp file and rename, so an interrupt
	// mid-write can never leave a torn file (L5).
	tmp, err := os.CreateTemp(filepath.Dir(a.Path), ".nikicode-edit-*")
	if err != nil {
		return ToolResult{Output: err.Error(), IsError: true}, nil
	}
	defer os.Remove(tmp.Name())
	if _, err := tmp.WriteString(updated); err != nil {
		tmp.Close()
		return ToolResult{Output: err.Error(), IsError: true}, nil
	}
	if err := tmp.Close(); err != nil {
		return ToolResult{Output: err.Error(), IsError: true}, nil
	}
	if err := os.Chmod(tmp.Name(), origMode); err != nil {
		return ToolResult{Output: err.Error(), IsError: true}, nil
	}
	if err := os.Rename(tmp.Name(), a.Path); err != nil {
		return ToolResult{Output: err.Error(), IsError: true}, nil
	}

	_ = format.FormatFile(ctx, a.Path)

	diffPreview := generateSimpleDiff(a.Path, a.OldString, a.NewString)
	replacements := n
	if replacements == 0 {
		replacements = 1
	}
	output := fmt.Sprintf("Successfully edited %s (%d replacement(s)%s, hash: %s):\n\n%s",
		a.Path, replacements, fuzzyNote, hexHash[:12], diffPreview)

	return ToolResult{Output: output}, nil
}

// normalizeLine collapses every whitespace run to a single space and
// trims the ends, so "  x  =  1  " and "x = 1" normalize alike.
func normalizeLine(l string) string {
	return strings.Join(strings.Fields(l), " ")
}

// fuzzyReplace looks for a unique line window in content whose
// normalized form equals the normalized old text. Returns applied=true
// with the rewritten content and a line number/confidence for logging.
// If several windows match it returns applied=false and a non-empty
// reason (ambiguous...); if none match, reason is empty. A unique
// normalized match logs confidence 0.90 (line-normalized heuristic).
func fuzzyReplace(content, oldText, newText string) (out string, line int, conf float64, applied bool, reason string) {
	oldLines := strings.Split(oldText, "\n")
	lines := strings.Split(content, "\n")
	normOld := make([]string, len(oldLines))
	for i, l := range oldLines {
		normOld[i] = normalizeLine(l)
	}
	var hits []int
	for start := 0; start+len(oldLines) <= len(lines); start++ {
		match := true
		for k, no := range normOld {
			if normalizeLine(lines[start+k]) != no {
				match = false
				break
			}
		}
		if match {
			hits = append(hits, start)
		}
	}
	if len(hits) == 0 {
		return "", 0, 0, false, ""
	}
	if len(hits) > 1 {
		return "", 0, 0, false, fmt.Sprintf("ambiguous fuzzy match: %d candidate positions (line %s) for old_string; refusing to guess", len(hits), joinLineNos(hits))
	}
	start := hits[0]
	newLines := strings.Split(newText, "\n")
	merged := make([]string, 0, len(lines)-len(oldLines)+len(newLines))
	merged = append(merged, lines[:start]...)
	merged = append(merged, newLines...)
	merged = append(merged, lines[start+len(oldLines):]...)
	return strings.Join(merged, "\n"), start + 1, 0.90, true, ""
}

func joinLineNos(idxs []int) string {
	parts := make([]string, len(idxs))
	for i, v := range idxs {
		parts[i] = fmt.Sprintf("%d", v+1)
	}
	return strings.Join(parts, ", ")
}
