package tools

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"strings"
)

type NotebookEditTool struct {
	Base
}

func NewNotebookEditTool() *NotebookEditTool {
	return &NotebookEditTool{
		Base: Base{
			SchemaStr: `{"required":["path","operation","cell_index"],"fields":{"path":"string","operation":"string","cell_index":"number","cell_type":"string","source":"string"}}`,
		},
	}
}

func (t *NotebookEditTool) Name() string { return "notebook_edit" }
func (t *NotebookEditTool) Description() string {
	return "Edit a Jupyter Notebook (.ipynb) cell by index (replace, insert, delete) while preserving outputs and metadata integrity"
}

type notebookEditArgs struct {
	Path      string `json:"path"`
	Operation string `json:"operation"` // "replace" | "insert" | "delete"
	CellIndex int    `json:"cell_index"`
	CellType  string `json:"cell_type,omitempty"` // "code" | "markdown"
	Source    string `json:"source,omitempty"`
}

type NotebookCell struct {
	CellType       string                 `json:"cell_type"`
	ExecutionCount *int                   `json:"execution_count"`
	Metadata       map[string]interface{} `json:"metadata"`
	Outputs        []interface{}          `json:"outputs"`
	Source         interface{}            `json:"source"`
}

func (t *NotebookEditTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a notebookEditArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}

	path := strings.TrimSpace(a.Path)
	if path == "" {
		return ToolResult{Output: "path cannot be empty", IsError: true}, nil
	}

	data, err := os.ReadFile(path)
	if err != nil {
		return ToolResult{Output: fmt.Sprintf("failed to read notebook %s: %v", path, err), IsError: true}, nil
	}

	// Read generic notebook preserving all top-level keys
	var notebook map[string]interface{}
	if err := json.Unmarshal(data, &notebook); err != nil {
		return ToolResult{Output: fmt.Sprintf("invalid notebook JSON: %v", err), IsError: true}, nil
	}

	rawCells, ok := notebook["cells"].([]interface{})
	if !ok {
		return ToolResult{Output: "notebook missing 'cells' array", IsError: true}, nil
	}

	cellType := a.CellType
	if cellType == "" {
		cellType = "code"
	}

	var sourceLines []string
	if a.Source != "" {
		lines := strings.Split(a.Source, "\n")
		for i, l := range lines {
			if i < len(lines)-1 {
				sourceLines = append(sourceLines, l+"\n")
			} else if l != "" {
				sourceLines = append(sourceLines, l)
			}
		}
	}

	newCell := map[string]interface{}{
		"cell_type": cellType,
		"metadata":  map[string]interface{}{},
		"source":    sourceLines,
	}
	if cellType == "code" {
		newCell["execution_count"] = nil
		newCell["outputs"] = []interface{}{}
	}

	switch strings.ToLower(a.Operation) {
	case "replace":
		if a.CellIndex < 0 || a.CellIndex >= len(rawCells) {
			return ToolResult{Output: fmt.Sprintf("cell_index %d out of bounds (0..%d)", a.CellIndex, len(rawCells)-1), IsError: true}, nil
		}
		rawCells[a.CellIndex] = newCell

	case "insert":
		if a.CellIndex < 0 || a.CellIndex > len(rawCells) {
			return ToolResult{Output: fmt.Sprintf("cell_index %d out of bounds for insert (0..%d)", a.CellIndex, len(rawCells)), IsError: true}, nil
		}
		newCells := make([]interface{}, 0, len(rawCells)+1)
		newCells = append(newCells, rawCells[:a.CellIndex]...)
		newCells = append(newCells, newCell)
		newCells = append(newCells, rawCells[a.CellIndex:]...)
		rawCells = newCells

	case "delete":
		if a.CellIndex < 0 || a.CellIndex >= len(rawCells) {
			return ToolResult{Output: fmt.Sprintf("cell_index %d out of bounds (0..%d)", a.CellIndex, len(rawCells)-1), IsError: true}, nil
		}
		rawCells = append(rawCells[:a.CellIndex], rawCells[a.CellIndex+1:]...)

	default:
		return ToolResult{Output: fmt.Sprintf("unsupported operation %q: must be replace, insert, or delete", a.Operation), IsError: true}, nil
	}

	notebook["cells"] = rawCells

	updatedData, err := json.MarshalIndent(notebook, "", " ")
	if err != nil {
		return ToolResult{Output: fmt.Sprintf("failed formatting notebook JSON: %v", err), IsError: true}, nil
	}

	if err := os.WriteFile(path, append(updatedData, '\n'), 0o644); err != nil {
		return ToolResult{Output: fmt.Sprintf("failed writing notebook %s: %v", path, err), IsError: true}, nil
	}

	return ToolResult{Output: fmt.Sprintf("Successfully executed %s on cell %d of %s (total cells: %d)", a.Operation, a.CellIndex, path, len(rawCells))}, nil
}
