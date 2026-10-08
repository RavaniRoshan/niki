package tools

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"image"
	"image/color"
	"image/png"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/permissions"
)

func TestCappingAndLineSplit(t *testing.T) {
	tmpDir := t.TempDir()

	// 1. Line splitting: lines > 2000 chars get split
	longLine := strings.Repeat("A", 4500)
	split := SplitLongLines(longLine, 2000)
	lines := strings.Split(split, "\n")
	if len(lines) != 3 {
		t.Fatalf("expected 3 lines after split, got %d", len(lines))
	}
	if len(lines[0]) != 2000 || len(lines[1]) != 2000 || len(lines[2]) != 500 {
		t.Fatalf("unexpected line lengths: %d, %d, %d", len(lines[0]), len(lines[1]), len(lines[2]))
	}

	// 2. Output capping: > 50,000 characters persists to file and shows preview
	hugeOutput := strings.Repeat("Line of text for testing capping.\n", 2000) // ~68,000 chars
	if len(hugeOutput) <= MaxOutputChars {
		t.Fatalf("test data too small: %d", len(hugeOutput))
	}

	preview, capped := CapOutput("test_tool", hugeOutput, tmpDir)
	if !capped {
		t.Fatalf("expected output to be capped")
	}
	if !strings.Contains(preview, "output capped:") {
		t.Fatalf("expected capping indicator in preview: %s", preview)
	}
	if len(preview) >= MaxOutputChars {
		t.Fatalf("preview should be bounded: %d", len(preview))
	}

	// Verify file was written to disk and has full content
	toolResultsDir := filepath.Join(tmpDir, "tool-results")
	entries, err := os.ReadDir(toolResultsDir)
	if err != nil || len(entries) == 0 {
		t.Fatalf("expected persisted tool result file in %s: %v", toolResultsDir, err)
	}
	persistedData, err := os.ReadFile(filepath.Join(toolResultsDir, entries[0].Name()))
	if err != nil || len(persistedData) != len(hugeOutput) {
		t.Fatalf("persisted content length %d != expected %d", len(persistedData), len(hugeOutput))
	}

	// 3. read_file opts out of capping
	if !ToolOptsOutFromCapping("read_file") {
		t.Fatalf("read_file must opt out of capping")
	}
	out, capped := CapOutput("read_file", hugeOutput, tmpDir)
	if capped || out != hugeOutput {
		t.Fatalf("read_file should not be capped")
	}
}

func TestWebSearch(t *testing.T) {
	ws := NewWebSearchTool()

	// 1. Live/cached search query
	res, err := ws.Run(context.Background(), json.RawMessage(`{"query":"go context"}`))
	if err != nil || res.IsError {
		t.Fatalf("web_search run error: %v %v", err, res)
	}
	if !strings.Contains(res.Output, "Package context") || !strings.Contains(res.Output, "pkg.go.dev/context") {
		t.Fatalf("expected context package documentation: %s", res.Output)
	}

	// 2. Domain filtering
	res, err = ws.Run(context.Background(), json.RawMessage(`{"query":"go context","allowed_domains":["pkg.go.dev"]}`))
	if err != nil || res.IsError {
		t.Fatalf("web_search with domain filter failed: %v", err)
	}
	if !strings.Contains(res.Output, "pkg.go.dev") {
		t.Fatalf("expected filtered results from pkg.go.dev: %s", res.Output)
	}

	// 3. Disabled mode
	ws.SetMode("disabled")
	res, _ = ws.Run(context.Background(), json.RawMessage(`{"query":"test"}`))
	if !res.IsError || !strings.Contains(res.Output, "disabled") {
		t.Fatalf("expected disabled error: %v", res)
	}

	// 4. Concurrency and read-only flags
	if !ws.IsReadOnly() || !ws.IsConcurrencySafe() {
		t.Fatalf("web_search must be read-only and concurrency-safe")
	}
}

func TestWebFetch(t *testing.T) {
	// Test HTTP server
	htmlContent := `
<!DOCTYPE html>
<html>
<head><title>Test Page</title><style>body { color: red; }</style></head>
<body>
<h1>Heading 1</h1>
<p>Hello world from <a href="https://golang.org">Go Language</a>!</p>
<ul>
<li>Item A</li>
<li>Item B</li>
</ul>
<pre><code>func main() {}</code></pre>
<script>alert("evil");</script>
</body>
</html>`

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		fmt.Fprint(w, htmlContent)
	}))
	defer srv.Close()

	wf := NewWebFetchTool()

	// Fetch page
	res, err := wf.Run(context.Background(), json.RawMessage(fmt.Sprintf(`{"url":%q}`, srv.URL)))
	if err != nil || res.IsError {
		t.Fatalf("web_fetch error: %v %v", err, res)
	}

	// Verify markdown extraction and script/style stripping
	if strings.Contains(res.Output, "<script>") || strings.Contains(res.Output, "alert") {
		t.Fatalf("script tags not stripped: %s", res.Output)
	}
	if strings.Contains(res.Output, "<style>") {
		t.Fatalf("style tags not stripped: %s", res.Output)
	}
	if !strings.Contains(res.Output, "# Heading 1") {
		t.Fatalf("h1 not converted to markdown: %s", res.Output)
	}
	if !strings.Contains(res.Output, "[Go Language](https://golang.org)") {
		t.Fatalf("anchor not converted: %s", res.Output)
	}
	if !strings.Contains(res.Output, "* Item A") {
		t.Fatalf("list item not converted: %s", res.Output)
	}

	// Verify 15m cache hit
	cachedRes, err := wf.Run(context.Background(), json.RawMessage(fmt.Sprintf(`{"url":%q}`, srv.URL)))
	if err != nil || !strings.Contains(cachedRes.Output, "served from 15m cache") {
		t.Fatalf("expected cache hit: %s", cachedRes.Output)
	}

	// Verify hard cap
	bigHTML := "<html><body>" + strings.Repeat("<p>Repeating text for leak test.</p>", 1000) + "</body></html>"
	bigSrv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/html")
		fmt.Fprint(w, bigHTML)
	}))
	defer bigSrv.Close()

	res, err = wf.Run(context.Background(), json.RawMessage(fmt.Sprintf(`{"url":%q}`, bigSrv.URL)))
	if err != nil || res.IsError {
		t.Fatalf("web_fetch big page error: %v", err)
	}
	if len(res.Output) > fetchHardCapChars+300 {
		t.Fatalf("output exceeded hard cap (%d > %d)", len(res.Output), fetchHardCapChars)
	}
	if !strings.Contains(res.Output, "content truncated") {
		t.Fatalf("expected truncation marker in output")
	}

	if !wf.IsReadOnly() || !wf.IsConcurrencySafe() {
		t.Fatalf("web_fetch must be read-only and concurrency-safe")
	}
}

func TestViewImage(t *testing.T) {
	tmpDir := t.TempDir()
	imgPath := filepath.Join(tmpDir, "test.png")

	// Create a test 1200x800 image
	img := image.NewRGBA(image.Rect(0, 0, 1200, 800))
	for y := 0; y < 800; y++ {
		for x := 0; x < 1200; x++ {
			img.Set(x, y, color.RGBA{R: uint8(x % 255), G: uint8(y % 255), B: 100, A: 255})
		}
	}
	f, err := os.Create(imgPath)
	if err != nil {
		t.Fatal(err)
	}
	if err := png.Encode(f, img); err != nil {
		t.Fatal(err)
	}
	f.Close()

	vi := NewViewImageTool()

	// 1. Default detail: resizes down to <= 1024
	res, err := vi.Run(context.Background(), json.RawMessage(fmt.Sprintf(`{"path":%q}`, imgPath)))
	if err != nil || res.IsError {
		t.Fatalf("view_image run error: %v %v", err, res)
	}
	if !strings.Contains(res.Output, "Original Dimensions: 1200x800") {
		t.Fatalf("missing original dimensions: %s", res.Output)
	}
	if !strings.Contains(res.Output, "Processed Dimensions: 1024x682") {
		t.Fatalf("missing resized dimensions: %s", res.Output)
	}
	if !strings.Contains(res.Output, "data:image/png;base64,") {
		t.Fatalf("missing base64 data url: %s", res.Output)
	}

	// 2. detail="original": keeps full resolution
	res, err = vi.Run(context.Background(), json.RawMessage(fmt.Sprintf(`{"path":%q,"detail":"original"}`, imgPath)))
	if err != nil || res.IsError {
		t.Fatalf("view_image original error: %v", err)
	}
	if !strings.Contains(res.Output, "Processed Dimensions: 1200x800 (resized: false)") {
		t.Fatalf("original detail should not resize: %s", res.Output)
	}

	// 3. Corrupt file error
	corruptPath := filepath.Join(tmpDir, "bad.png")
	os.WriteFile(corruptPath, []byte("not an image"), 0o644)
	res, _ = vi.Run(context.Background(), json.RawMessage(fmt.Sprintf(`{"path":%q}`, corruptPath)))
	if !res.IsError || !strings.Contains(res.Output, "unsupported or corrupt") {
		t.Fatalf("expected corrupt image error: %v", res)
	}

	if !vi.IsReadOnly() || !vi.IsConcurrencySafe() {
		t.Fatalf("view_image must be read-only and concurrency-safe")
	}
}

func TestNotebookEdit(t *testing.T) {
	tmpDir := t.TempDir()
	nbPath := filepath.Join(tmpDir, "notebook.ipynb")

	initialNB := map[string]interface{}{
		"nbformat":       4,
		"nbformat_minor": 2,
		"custom_field":   "preserved_value",
		"cells": []interface{}{
			map[string]interface{}{
				"cell_type":       "code",
				"execution_count": 5,
				"metadata":        map[string]interface{}{},
				"outputs":         []interface{}{"output line"},
				"source":          []string{"print('initial cell 0')\n"},
			},
			map[string]interface{}{
				"cell_type": "markdown",
				"metadata":  map[string]interface{}{},
				"source":    []string{"# Markdown cell 1\n"},
			},
		},
	}
	raw, _ := json.Marshal(initialNB)
	os.WriteFile(nbPath, raw, 0o644)

	ne := NewNotebookEditTool()

	// 1. Replace code cell: should clear outputs and reset execution_count
	res, err := ne.Run(context.Background(), json.RawMessage(fmt.Sprintf(`{
		"path": %q,
		"operation": "replace",
		"cell_index": 0,
		"cell_type": "code",
		"source": "import os\nprint('new cell 0')"
	}`, nbPath)))
	if err != nil || res.IsError {
		t.Fatalf("notebook_edit replace error: %v %v", err, res)
	}

	// Read and verify
	var updatedNB map[string]interface{}
	data, _ := os.ReadFile(nbPath)
	json.Unmarshal(data, &updatedNB)

	if updatedNB["custom_field"] != "preserved_value" {
		t.Fatalf("custom top-level field was not preserved")
	}
	cells := updatedNB["cells"].([]interface{})
	cell0 := cells[0].(map[string]interface{})
	if cell0["execution_count"] != nil {
		t.Fatalf("execution_count should be reset to nil, got %v", cell0["execution_count"])
	}
	outputs := cell0["outputs"].([]interface{})
	if len(outputs) != 0 {
		t.Fatalf("outputs should be cleared, got %v", outputs)
	}

	// 2. Insert cell
	res, err = ne.Run(context.Background(), json.RawMessage(fmt.Sprintf(`{
		"path": %q,
		"operation": "insert",
		"cell_index": 1,
		"cell_type": "markdown",
		"source": "## Inserted section"
	}`, nbPath)))
	if err != nil || res.IsError {
		t.Fatalf("notebook_edit insert error: %v %v", err, res)
	}

	data, _ = os.ReadFile(nbPath)
	json.Unmarshal(data, &updatedNB)
	cells = updatedNB["cells"].([]interface{})
	if len(cells) != 3 {
		t.Fatalf("expected 3 cells after insert, got %d", len(cells))
	}

	// 3. Delete cell
	res, err = ne.Run(context.Background(), json.RawMessage(fmt.Sprintf(`{
		"path": %q,
		"operation": "delete",
		"cell_index": 2
	}`, nbPath)))
	if err != nil || res.IsError {
		t.Fatalf("notebook_edit delete error: %v %v", err, res)
	}

	data, _ = os.ReadFile(nbPath)
	json.Unmarshal(data, &updatedNB)
	cells = updatedNB["cells"].([]interface{})
	if len(cells) != 2 {
		t.Fatalf("expected 2 cells after delete, got %d", len(cells))
	}

	// Tool permissions: Fail-closed
	if ne.IsReadOnly() || ne.IsConcurrencySafe() {
		t.Fatalf("notebook_edit must not be read-only or concurrency-safe")
	}
}

func TestUpdatePlanAndTodoWrite(t *testing.T) {
	up := NewUpdatePlanTool()

	// 1. Valid plan update
	res, err := up.Run(context.Background(), json.RawMessage(`{
		"steps": [
			{"title": "Step 1: Inspect code", "status": "completed"},
			{"title": "Step 2: Add feature", "status": "in_progress"},
			{"title": "Step 3: Run tests", "status": "pending"}
		]
	}`))
	if err != nil || res.IsError {
		t.Fatalf("update_plan error: %v %v", err, res)
	}
	steps := up.GetSteps()
	if len(steps) != 3 || steps[1].Status != "in_progress" {
		t.Fatalf("unexpected plan steps: %v", steps)
	}

	// 2. Invariant: <= 1 in_progress step allowed
	res, _ = up.Run(context.Background(), json.RawMessage(`{
		"steps": [
			{"title": "Step 1", "status": "in_progress"},
			{"title": "Step 2", "status": "in_progress"}
		]
	}`))
	if !res.IsError || !strings.Contains(res.Output, "at most 1 step can be in_progress") {
		t.Fatalf("expected invariant violation error: %v", res)
	}

	// 3. Rejected in Plan Mode
	up.SetPlanMode(true)
	res, _ = up.Run(context.Background(), json.RawMessage(`{
		"steps": [
			{"title": "Step 1", "status": "in_progress"}
		]
	}`))
	if !res.IsError || !strings.Contains(res.Output, "Plan Mode") {
		t.Fatalf("expected rejection in Plan Mode: %v", res)
	}

	// 4. TodoWriteTool
	tw := NewTodoWriteTool()
	res, err = tw.Run(context.Background(), json.RawMessage(`{
		"todos": [
			{"id": "t1", "text": "Set up database", "status": "done"},
			{"id": "t2", "text": "Build endpoints", "status": "in_progress"},
			{"id": "t3", "text": "Write docs", "status": "todo"}
		]
	}`))
	if err != nil || res.IsError {
		t.Fatalf("todo_write error: %v %v", err, res)
	}
	todos := tw.GetTodos()
	if len(todos) != 3 || todos[0].Status != "done" {
		t.Fatalf("unexpected todo items: %v", todos)
	}
}

func TestToolSearch(t *testing.T) {
	r := DefaultRegistry()
	ts := NewToolSearchTool(r)

	// 1. Exact match fast path
	res, err := ts.Run(context.Background(), json.RawMessage(`{"query":"read_file"}`))
	if err != nil || res.IsError {
		t.Fatalf("tool_search error: %v", err)
	}
	if !strings.Contains(res.Output, "exact match") || !strings.Contains(res.Output, "read_file") {
		t.Fatalf("expected exact match output: %s", res.Output)
	}

	// 2. select:A,B,C direct load
	res, err = ts.Run(context.Background(), json.RawMessage(`{"query":"select:web_search,view_image"}`))
	if err != nil || res.IsError {
		t.Fatalf("tool_search select error: %v", err)
	}
	if !strings.Contains(res.Output, "web_search") || !strings.Contains(res.Output, "view_image") {
		t.Fatalf("expected selected tools: %s", res.Output)
	}

	// 3. Keyword / BM25 matching
	res, err = ts.Run(context.Background(), json.RawMessage(`{"query":"interactive terminal background"}`))
	if err != nil || res.IsError {
		t.Fatalf("tool_search keyword error: %v", err)
	}
	if !strings.Contains(res.Output, "exec_command") {
		t.Fatalf("expected exec_command in keyword search: %s", res.Output)
	}

	// 4. Discovered set tracking
	discovered := ts.DiscoveredTools()
	if len(discovered) == 0 {
		t.Fatalf("expected discovered tools tracking to contain items")
	}

	if !ts.IsReadOnly() || !ts.IsConcurrencySafe() {
		t.Fatalf("tool_search must be read-only and concurrency-safe")
	}
}

func TestProcessManagerAndProcessTools(t *testing.T) {
	pm := NewProcessManager()
	execTool := NewExecCommandTool(pm)
	stdinTool := NewWriteStdinTool(pm)
	outputTool := NewBashOutputTool(pm)
	killTool := NewKillShellTool(pm)

	// 1. Foreground execution
	res, err := execTool.Run(context.Background(), json.RawMessage(`{"command":"echo 'hello pty'"}`))
	if err != nil || res.IsError {
		t.Fatalf("execTool foreground error: %v %v", err, res)
	}
	if !strings.Contains(res.Output, "hello pty") {
		t.Fatalf("expected output 'hello pty', got %s", res.Output)
	}

	// 2. Background process execution
	res, err = execTool.Run(context.Background(), json.RawMessage(`{"command":"sleep 10","background":true}`))
	if err != nil || res.IsError {
		t.Fatalf("execTool background error: %v", err)
	}
	if !strings.Contains(res.Output, "started in background") {
		t.Fatalf("expected background start notice: %s", res.Output)
	}

	// Extract process ID from output: "Process ID: pX\n..."
	var procID string
	for _, line := range strings.Split(res.Output, "\n") {
		if strings.HasPrefix(line, "Process ID: ") {
			procID = strings.TrimPrefix(line, "Process ID: ")
			break
		}
	}
	if procID == "" {
		t.Fatalf("failed finding process ID in %s", res.Output)
	}

	// 3. Read status
	outRes, err := outputTool.Run(context.Background(), json.RawMessage(fmt.Sprintf(`{"process_id":%q}`, procID)))
	if err != nil || outRes.IsError {
		t.Fatalf("outputTool error: %v", err)
	}
	if !strings.Contains(outRes.Output, "status: running") {
		t.Fatalf("expected running status: %s", outRes.Output)
	}

	// 4. Send interrupt
	inRes, err := stdinTool.Run(context.Background(), json.RawMessage(fmt.Sprintf(`{"process_id":%q,"interrupt":true}`, procID)))
	if err != nil || inRes.IsError {
		t.Fatalf("stdinTool interrupt error: %v", err)
	}

	// 5. Kill shell
	killRes, err := killTool.Run(context.Background(), json.RawMessage(fmt.Sprintf(`{"process_id":%q,"signal":"SIGKILL"}`, procID)))
	if err != nil || killRes.IsError {
		t.Fatalf("killTool error: %v", err)
	}

	time.Sleep(50 * time.Millisecond)
	outRes, _ = outputTool.Run(context.Background(), json.RawMessage(fmt.Sprintf(`{"process_id":%q}`, procID)))
	if !strings.Contains(outRes.Output, "exited") {
		t.Fatalf("expected process to be exited: %s", outRes.Output)
	}

	// Check read-only / concurrency-safe flags
	if !outputTool.IsReadOnly() || !outputTool.IsConcurrencySafe() {
		t.Fatalf("bash_output must be read-only and concurrency-safe")
	}
	if execTool.IsReadOnly() || stdinTool.IsReadOnly() || killTool.IsReadOnly() {
		t.Fatalf("exec/write/kill tools must be fail-closed (not read-only)")
	}
}

func TestAskUserQuestion(t *testing.T) {
	auq := NewAskUserQuestionTool()

	// 1. Valid questions
	res, err := auq.Run(context.Background(), json.RawMessage(`{
		"questions": [
			{
				"header": "Database",
				"question": "Which database engine would you like to use?",
				"options": ["PostgreSQL", "SQLite", "MySQL"],
				"allow_custom": true
			}
		]
	}`))
	if err != nil || res.IsError {
		t.Fatalf("ask_user_question error: %v %v", err, res)
	}
	if !strings.Contains(res.Output, "[Database]") || !strings.Contains(res.Output, "Other (type custom response)") {
		t.Fatalf("expected formatted question: %s", res.Output)
	}

	// 2. Validation: header > 12 chars
	res, _ = auq.Run(context.Background(), json.RawMessage(`{
		"questions": [
			{
				"header": "ThisHeaderIsWayTooLong",
				"question": "Test?",
				"options": ["A", "B"]
			}
		]
	}`))
	if !res.IsError || !strings.Contains(res.Output, "exceeds 12 characters") {
		t.Fatalf("expected header length validation error: %v", res)
	}

	// 3. Validation: < 2 options
	res, _ = auq.Run(context.Background(), json.RawMessage(`{
		"questions": [
			{
				"header": "Choice",
				"question": "Only one option?",
				"options": ["OnlyOption"]
			}
		]
	}`))
	if !res.IsError || !strings.Contains(res.Output, "options count must be between 2 and 4") {
		t.Fatalf("expected options count validation error: %v", res)
	}

	// 4. Subagent fail-closed protection
	auq.SetSubagent(true)
	res, _ = auq.Run(context.Background(), json.RawMessage(`{
		"questions": [
			{
				"header": "Choice",
				"question": "Test question?",
				"options": ["A", "B"]
			}
		]
	}`))
	if !res.IsError || !strings.Contains(res.Output, "disabled within subagent contexts") {
		t.Fatalf("expected subagent block error: %v", res)
	}

	if !auq.IsReadOnly() {
		t.Fatalf("ask_user_question must be read-only")
	}
}

func TestEditFileHashAndDiff(t *testing.T) {
	tmpDir := t.TempDir()
	p := filepath.Join(tmpDir, "file.txt")
	content := "line 1\nreplace this line\nline 3\n"
	os.WriteFile(p, []byte(content), 0o644)

	h := sha256.Sum256([]byte(content))
	realHash := hex.EncodeToString(h[:])

	ef := NewEditFileTool()

	// 1. Hash mismatch test
	res, _ := ef.Run(context.Background(), json.RawMessage(fmt.Sprintf(`{
		"path": %q,
		"old_string": "replace this line",
		"new_string": "replaced line",
		"expected_hash": "deadbeef1234"
	}`, p)))
	if !res.IsError || !strings.Contains(res.Output, "hash mismatch") {
		t.Fatalf("expected hash mismatch error: %v", res)
	}

	// 2. Successful edit with correct hash and diff preview
	res, err := ef.Run(context.Background(), json.RawMessage(fmt.Sprintf(`{
		"path": %q,
		"old_string": "replace this line",
		"new_string": "replaced line",
		"expected_hash": %q
	}`, p, realHash)))
	if err != nil || res.IsError {
		t.Fatalf("edit_file error: %v %v", err, res)
	}
	if !strings.Contains(res.Output, "--- a/") || !strings.Contains(res.Output, "-replace this line") || !strings.Contains(res.Output, "+replaced line") {
		t.Fatalf("expected diff preview in output: %s", res.Output)
	}

	// 3. Replace all test
	os.WriteFile(p, []byte("repeat repeat repeat"), 0o644)
	res, _ = ef.Run(context.Background(), json.RawMessage(fmt.Sprintf(`{
		"path": %q,
		"old_string": "repeat",
		"new_string": "word"
	}`, p)))
	if !res.IsError || !strings.Contains(res.Output, "expected exactly 1 occurrence") {
		t.Fatalf("expected multiple occurrence error without replace_all: %v", res)
	}

	res, err = ef.Run(context.Background(), json.RawMessage(fmt.Sprintf(`{
		"path": %q,
		"old_string": "repeat",
		"new_string": "word",
		"replace_all": true
	}`, p)))
	if err != nil || res.IsError {
		t.Fatalf("replace_all failed: %v", err)
	}
	updatedBytes, _ := os.ReadFile(p)
	if string(updatedBytes) != "word word word" {
		t.Fatalf("unexpected content after replace_all: %q", string(updatedBytes))
	}
}

func TestApplyPatchFuzzyAndReverse(t *testing.T) {
	tmpDir := t.TempDir()
	p := filepath.Join(tmpDir, "code.txt")
	original := "function a() {\n  return 1;\n}\n\nfunction b() {\n  return 2;\n}\n"
	os.WriteFile(p, []byte(original), 0o644)

	patchText := `--- a/code.txt
+++ b/code.txt
@@ -5,3 +5,3 @@
 function b() {
-  return 2;
+  return 42;
 }
`

	ap := NewApplyPatchTool()

	// 1. Normal apply
	res, err := ap.Run(context.Background(), json.RawMessage(fmt.Sprintf(`{
		"path": %q,
		"patch": %q
	}`, p, patchText)))
	if err != nil || res.IsError {
		t.Fatalf("apply_patch error: %v %v", err, res)
	}
	data, _ := os.ReadFile(p)
	if !strings.Contains(string(data), "return 42;") {
		t.Fatalf("patch was not applied: %s", string(data))
	}

	// 2. Reverse apply
	res, err = ap.Run(context.Background(), json.RawMessage(fmt.Sprintf(`{
		"path": %q,
		"patch": %q,
		"reverse": true
	}`, p, patchText)))
	if err != nil || res.IsError {
		t.Fatalf("apply_patch reverse error: %v %v", err, res)
	}
	data, _ = os.ReadFile(p)
	if !strings.Contains(string(data), "return 2;") || strings.Contains(string(data), "return 42;") {
		t.Fatalf("patch was not reversed: %s", string(data))
	}
}

func TestPermissionsReadOnlyWhitelist(t *testing.T) {
	guard := permissions.NewGuard(permissions.ModeReadOnly)

	// All read-only tools must be allowed in read-only mode
	readOnly := []string{
		"read_file", "glob", "grep", "web_search", "web_fetch",
		"view_image", "tool_search", "bash_output", "ask_user_question",
	}
	for _, tool := range readOnly {
		if !guard.Allow(tool) {
			t.Fatalf("tool %q must be allowed in read-only mode", tool)
		}
	}

	// Write / exec tools must be DENIED in read-only mode (fail-closed)
	writeTools := []string{
		"write_file", "edit_file", "apply_patch", "shell",
		"notebook_edit", "update_plan", "todo_write", "exec_command",
		"write_stdin", "kill_shell", "spawn_agent", "send_input",
		"wait_agent", "close_agent", "resume_agent",
	}
	for _, tool := range writeTools {
		if guard.Allow(tool) {
			t.Fatalf("tool %q must be DENIED in read-only mode", tool)
		}
	}
}

func TestSubagentToolFamily(t *testing.T) {
	spawn := NewSpawnAgentTool(nil)
	send := NewSendInputTool(nil)
	wait := NewWaitAgentTool(nil)
	resTool := NewResumeAgentTool(nil)
	closeTool := NewCloseAgentTool(nil)

	// 1. Spawn
	sRes, err := spawn.Run(context.Background(), json.RawMessage(`{"name":"worker","prompt":"analyze code"}`))
	if err != nil || sRes.IsError {
		t.Fatalf("spawn_agent error: %v %v", err, sRes)
	}
	if !strings.Contains(sRes.Output, "Subagent spawned successfully") {
		t.Fatalf("unexpected spawn output: %s", sRes.Output)
	}

	// 2. Send Input
	msgRes, err := send.Run(context.Background(), json.RawMessage(`{"agent_id":"agent-stub-1","message":"check tests"}`))
	if err != nil || msgRes.IsError {
		t.Fatalf("send_input error: %v %v", err, msgRes)
	}

	// 3. Wait
	waitRes, err := wait.Run(context.Background(), json.RawMessage(`{"agent_id":"agent-stub-1"}`))
	if err != nil || waitRes.IsError {
		t.Fatalf("wait_agent error: %v %v", err, waitRes)
	}
	if !strings.Contains(waitRes.Output, "status: completed") {
		t.Fatalf("unexpected wait output: %s", waitRes.Output)
	}

	// 4. Resume
	rRes, err := resTool.Run(context.Background(), json.RawMessage(`{"agent_id":"agent-stub-1"}`))
	if err != nil || rRes.IsError {
		t.Fatalf("resume_agent error: %v %v", err, rRes)
	}

	// 5. Close
	cRes, err := closeTool.Run(context.Background(), json.RawMessage(`{"agent_id":"agent-stub-1"}`))
	if err != nil || cRes.IsError {
		t.Fatalf("close_agent error: %v %v", err, cRes)
	}
	if !strings.Contains(cRes.Output, "closed") {
		t.Fatalf("unexpected close output: %s", cRes.Output)
	}
}
