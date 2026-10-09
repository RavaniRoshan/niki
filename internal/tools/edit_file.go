package tools

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
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

func (t *EditFileTool) Name() string        { return "edit_file" }
func (t *EditFileTool) Description() string { return "Replace exact string occurrences in a file with read-before-edit hash check and unified diff preview" }

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
	if n == 0 {
		return ToolResult{Output: fmt.Sprintf("target old_string not found in %s", a.Path), IsError: true}, nil
	}

	if !a.ReplaceAll && n > 1 {
		return ToolResult{
			Output:  fmt.Sprintf("expected exactly 1 occurrence, found %d (set replace_all: true to replace all instances)", n),
			IsError: true,
		}, nil
	}

	var updated string
	if a.ReplaceAll {
		updated = strings.ReplaceAll(fileContent, a.OldString, a.NewString)
	} else {
		updated = strings.Replace(fileContent, a.OldString, a.NewString, 1)
	}

	if err := os.WriteFile(a.Path, []byte(updated), 0o644); err != nil {
		return ToolResult{Output: err.Error(), IsError: true}, nil
	}

	_ = format.FormatFile(ctx, a.Path)

	diffPreview := generateSimpleDiff(a.Path, a.OldString, a.NewString)
	output := fmt.Sprintf("Successfully edited %s (%d replacement(s), hash: %s):\n\n%s",
		a.Path, n, hexHash[:12], diffPreview)

	return ToolResult{Output: output}, nil
}
