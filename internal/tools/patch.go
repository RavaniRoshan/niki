package tools

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"strconv"
	"strings"
)

// DiffHunk represents a unified diff hunk.
type DiffHunk struct {
	OldStart  int
	OldLength int
	NewStart  int
	NewLength int
	Lines     []string
}

// FilePatch represents changes to a single file.
type FilePatch struct {
	OldPath string
	NewPath string
	Hunks   []DiffHunk
}

// ParsePatch parses unified diff text into FilePatch structures.
func ParsePatch(diffText string) ([]FilePatch, error) {
	var patches []FilePatch
	var curPatch *FilePatch
	var curHunk *DiffHunk

	sc := bufio.NewScanner(strings.NewReader(diffText))
	for sc.Scan() {
		line := sc.Text()

		if strings.HasPrefix(line, "--- ") {
			if curHunk != nil && curPatch != nil {
				curPatch.Hunks = append(curPatch.Hunks, *curHunk)
				curHunk = nil
			}
			if curPatch != nil {
				patches = append(patches, *curPatch)
			}
			curPatch = &FilePatch{
				OldPath: strings.TrimSpace(strings.TrimPrefix(line, "--- ")),
			}
			continue
		}

		if strings.HasPrefix(line, "+++ ") {
			if curPatch == nil {
				curPatch = &FilePatch{}
			}
			curPatch.NewPath = strings.TrimSpace(strings.TrimPrefix(line, "+++ "))
			continue
		}

		if strings.HasPrefix(line, "@@") {
			if curPatch == nil {
				curPatch = &FilePatch{OldPath: "unknown", NewPath: "unknown"}
			}
			if curHunk != nil {
				curPatch.Hunks = append(curPatch.Hunks, *curHunk)
			}
			hunk, err := parseHunkHeader(line)
			if err != nil {
				return nil, err
			}
			curHunk = hunk
			continue
		}

		if curHunk != nil {
			if strings.HasPrefix(line, " ") || strings.HasPrefix(line, "+") || strings.HasPrefix(line, "-") {
				curHunk.Lines = append(curHunk.Lines, line)
			} else if line == "" {
				// Blank line treated as empty context line
				curHunk.Lines = append(curHunk.Lines, " ")
			}
		}
	}

	if curHunk != nil && curPatch != nil {
		curPatch.Hunks = append(curPatch.Hunks, *curHunk)
	}
	if curPatch != nil {
		patches = append(patches, *curPatch)
	}

	if err := sc.Err(); err != nil {
		return nil, err
	}
	return patches, nil
}

func parseHunkHeader(line string) (*DiffHunk, error) {
	// Format: @@ -oldStart,oldLen +newStart,newLen @@
	parts := strings.Split(line, "@@")
	if len(parts) < 3 {
		return nil, fmt.Errorf("malformed hunk header: %s", line)
	}
	rangeStr := strings.TrimSpace(parts[1])
	tokens := strings.Fields(rangeStr)
	if len(tokens) < 2 {
		return nil, fmt.Errorf("malformed hunk ranges: %s", rangeStr)
	}

	oldStart, oldLen, err := parseRange(tokens[0], "-")
	if err != nil {
		return nil, err
	}
	newStart, newLen, err := parseRange(tokens[1], "+")
	if err != nil {
		return nil, err
	}

	return &DiffHunk{
		OldStart:  oldStart,
		OldLength: oldLen,
		NewStart:  newStart,
		NewLength: newLen,
	}, nil
}

func parseRange(token, prefix string) (int, int, error) {
	if !strings.HasPrefix(token, prefix) {
		return 0, 0, fmt.Errorf("expected range prefix %q in %q", prefix, token)
	}
	val := strings.TrimPrefix(token, prefix)
	sub := strings.Split(val, ",")
	start, err := strconv.Atoi(sub[0])
	if err != nil {
		return 0, 0, fmt.Errorf("invalid range start %q: %w", sub[0], err)
	}
	length := 1
	if len(sub) > 1 {
		length, err = strconv.Atoi(sub[1])
		if err != nil {
			return 0, 0, fmt.Errorf("invalid range length %q: %w", sub[1], err)
		}
	}
	return start, length, nil
}

// ApplyPatch applies hunks to original file content and returns the updated text.
func ApplyPatch(content string, hunks []DiffHunk) (string, error) {
	lines := strings.Split(content, "\n")
	// If the file is empty, lines is [""]
	if len(lines) == 1 && lines[0] == "" {
		lines = []string{}
	}

	var result []string
	srcIdx := 0

	for _, hunk := range hunks {
		targetIdx := hunk.OldStart - 1
		if targetIdx < 0 {
			targetIdx = 0
		}

		// Copy unchanged lines prior to hunk
		for srcIdx < targetIdx && srcIdx < len(lines) {
			result = append(result, lines[srcIdx])
			srcIdx++
		}

		for _, hLine := range hunk.Lines {
			if len(hLine) == 0 {
				continue
			}
			indicator := hLine[0]
			text := hLine[1:]

			switch indicator {
			case ' ':
				if srcIdx < len(lines) {
					result = append(result, lines[srcIdx])
					srcIdx++
				} else {
					result = append(result, text)
				}
			case '-':
				if srcIdx < len(lines) {
					srcIdx++ // skip/delete from source
				}
			case '+':
				result = append(result, text)
			}
		}
	}

	// Append any remaining lines
	for srcIdx < len(lines) {
		result = append(result, lines[srcIdx])
		srcIdx++
	}

	return strings.Join(result, "\n"), nil
}

// ApplyPatchTool exposes patch parsing and application as a built-in tool.
type ApplyPatchTool struct {
	Base
}

func NewApplyPatchTool() *ApplyPatchTool {
	return &ApplyPatchTool{
		Base: Base{SchemaStr: `{"required":["path","patch"],"fields":{"path":"string","patch":"string"}}`},
	}
}

func (t *ApplyPatchTool) Name() string        { return "apply_patch" }
func (t *ApplyPatchTool) Description() string { return "Apply a unified diff patch to a file" }

type applyPatchArgs struct {
	Path  string `json:"path"`
	Patch string `json:"patch"`
}

func (t *ApplyPatchTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a applyPatchArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}

	patches, err := ParsePatch(a.Patch)
	if err != nil {
		return ToolResult{Output: fmt.Sprintf("invalid patch: %v", err), IsError: true}, nil
	}
	if len(patches) == 0 || len(patches[0].Hunks) == 0 {
		return ToolResult{Output: "no hunks found in patch", IsError: true}, nil
	}

	data, err := os.ReadFile(a.Path)
	if err != nil {
		return ToolResult{Output: err.Error(), IsError: true}, nil
	}

	updated, err := ApplyPatch(string(data), patches[0].Hunks)
	if err != nil {
		return ToolResult{Output: fmt.Sprintf("failed applying patch: %v", err), IsError: true}, nil
	}

	if err := os.WriteFile(a.Path, []byte(updated), 0o644); err != nil {
		return ToolResult{Output: err.Error(), IsError: true}, nil
	}

	return ToolResult{Output: fmt.Sprintf("applied %d hunks to %s", len(patches[0].Hunks), a.Path)}, nil
}
