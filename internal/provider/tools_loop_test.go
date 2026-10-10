package provider

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

var testToolSpecs = []ToolSpec{
	{
		Name:        "read_file",
		Description: "Read a file with optional line offset/limit",
		Parameters:  json.RawMessage(`{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}`),
	},
	{
		Name:        "shell",
		Description: "Run a shell command",
		Parameters:  json.RawMessage(`{"type":"object","properties":{"command":{"type":"string"}},"required":["command"]}`),
	},
}

// toolConversation is the engine history shape: an assistant tool call turn
// followed by the tool result, then the next user turn.
func toolConversation() []Message {
	return []Message{
		{Role: "system", Content: "You are NikiCode."},
		{Role: "user", Content: "list files"},
		{Role: "assistant", Content: "Running ls", ToolCalls: []ToolCall{{ID: "call_prev", Tool: "shell", Args: `{"command":"ls"}`}}},
		{Role: "tool", Content: "a.txt\nb.txt", Name: "shell", ToolCallID: "call_prev"},
		{Role: "user", Content: "now read a.txt"},
	}
}

func TestOpenAIToolsRequestAndToolCallStream(t *testing.T) {
	var captured []byte
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		b, _ := io.ReadAll(r.Body)
		captured = b
		if !strings.Contains(r.URL.Path, "/chat/completions") {
			t.Errorf("unexpected path %s", r.URL.Path)
		}
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(http.StatusOK)
		_, _ = io.WriteString(w, strings.Join([]string{
			`data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"shell","arguments":""}}]}}]}`,
			`data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"comm"}}]}}]}`,
			`data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"and\":\"echo hi\"}"}}]}}]}`,
			`data: {"choices":[{"delta":{"tool_calls":[{"index":1,"id":"call_2","function":{"name":"read_file","arguments":"{\"path\":\"a.txt\"}"}}]}}]}`,
			`data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}`,
			`data: [DONE]`,
			"",
		}, "\n"))
	}))
	defer srv.Close()

	o := NewOpenAIProvider(srv.URL, "k", "gpt-4o")
	deltas, errs := o.StreamWithTools(context.Background(), toolConversation(), testToolSpecs)

	var calls []Delta
	for d := range deltas {
		if d.Kind == DeltaToolCall {
			calls = append(calls, d)
		}
	}
	if err := <-errs; err != nil {
		t.Fatalf("stream error: %v", err)
	}
	if len(calls) != 2 {
		t.Fatalf("expected 2 tool calls, got %d: %+v", len(calls), calls)
	}
	if calls[0].ToolName != "shell" || calls[0].ToolArgs != `{"command":"echo hi"}` || calls[0].CallID != "call_1" {
		t.Fatalf("unexpected first call: %+v", calls[0])
	}
	if calls[1].ToolName != "read_file" || calls[1].CallID != "call_2" {
		t.Fatalf("unexpected second call: %+v", calls[1])
	}

	// Request assertions: tools array + repaired conversation shape.
	var req struct {
		Tools []struct {
			Type     string `json:"type"`
			Function struct {
				Name        string          `json:"name"`
				Description string          `json:"description"`
				Parameters  json.RawMessage `json:"parameters"`
			} `json:"function"`
		} `json:"tools"`
		Messages []struct {
			Role       string `json:"role"`
			Content    string `json:"content"`
			ToolCallID string `json:"tool_call_id"`
			ToolCalls  []struct {
				ID       string `json:"id"`
				Function struct {
					Name      string `json:"name"`
					Arguments string `json:"arguments"`
				} `json:"function"`
			} `json:"tool_calls"`
		} `json:"messages"`
	}
	if err := json.Unmarshal(captured, &req); err != nil {
		t.Fatalf("capture unmarshal: %v", err)
	}
	if len(req.Tools) != 2 {
		t.Fatalf("expected 2 tools in request, got %d", len(req.Tools))
	}
	if req.Tools[0].Type != "function" || req.Tools[0].Function.Name != "read_file" {
		t.Fatalf("unexpected tool entry: %+v", req.Tools[0])
	}
	var props map[string]interface{}
	if err := json.Unmarshal(req.Tools[0].Function.Parameters, &props); err != nil || props["properties"] == nil {
		t.Fatalf("parameters not a JSON schema: %s", req.Tools[0].Function.Parameters)
	}
	var sawAssistantCalls, sawToolResult bool
	for _, m := range req.Messages {
		if m.Role == "assistant" && len(m.ToolCalls) > 0 && m.ToolCalls[0].Function.Name == "shell" {
			sawAssistantCalls = true
		}
		if m.Role == "tool" && m.ToolCallID == "call_prev" && strings.Contains(m.Content, "a.txt") {
			sawToolResult = true
		}
	}
	if !sawAssistantCalls {
		t.Fatal("assistant message with tool_calls missing from request")
	}
	if !sawToolResult {
		t.Fatal("tool result message with tool_call_id missing from request")
	}
}

func TestAnthropicToolsRequestAndToolUseStream(t *testing.T) {
	var captured []byte
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		b, _ := io.ReadAll(r.Body)
		captured = b
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(http.StatusOK)
		_, _ = io.WriteString(w, strings.Join([]string{
			`data: {"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_1","name":"shell"}}`,
			`data: {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"command\":"}}`,
			`data: {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":" \"pwd\"}"}}`,
			`data: {"type":"content_block_stop","index":0}`,
			`data: {"type":"message_delta","usage":{"input_tokens":9,"output_tokens":5}}`,
			`data: {"type":"message_stop"}`,
			`data: [DONE]`,
			"",
		}, "\n"))
	}))
	defer srv.Close()

	a := NewAnthropicProvider(srv.URL, "k", "claude-3-5-sonnet")
	deltas, errs := a.StreamWithTools(context.Background(), toolConversation(), testToolSpecs)

	var calls []Delta
	var usage *Usage
	for d := range deltas {
		switch d.Kind {
		case DeltaToolCall:
			calls = append(calls, d)
		case DeltaUsage:
			usage = d.Usage
		}
	}
	if err := <-errs; err != nil {
		t.Fatalf("stream error: %v", err)
	}
	if len(calls) != 1 || calls[0].ToolName != "shell" || calls[0].CallID != "toolu_1" || calls[0].ToolArgs != `{"command": "pwd"}` {
		t.Fatalf("unexpected tool calls: %+v", calls)
	}
	if usage == nil || usage.PromptTokens != 9 {
		t.Fatalf("usage not parsed: %+v", usage)
	}

	var req struct {
		Tools []struct {
			Name        string          `json:"name"`
			Description string          `json:"description"`
			InputSchema json.RawMessage `json:"input_schema"`
		} `json:"tools"`
		System   string `json:"system"`
		Messages []struct {
			Role    string          `json:"role"`
			Content json.RawMessage `json:"content"`
		} `json:"messages"`
		MaxTokens int `json:"max_tokens"`
	}
	if err := json.Unmarshal(captured, &req); err != nil {
		t.Fatalf("capture unmarshal: %v", err)
	}
	if len(req.Tools) != 2 || req.Tools[0].Name != "read_file" || req.Tools[1].Name != "shell" {
		t.Fatalf("unexpected tools: %+v", req.Tools)
	}
	if req.System == "" || req.MaxTokens == 0 {
		t.Fatalf("system=%q max_tokens=%d", req.System, req.MaxTokens)
	}
	// Conversation must be: user, assistant(text+tool_use), user(tool_result + next text).
	type block struct {
		Type    string `json:"type"`
		Text    string `json:"text,omitempty"`
		ID      string `json:"id,omitempty"`
		Name    string `json:"name,omitempty"`
		Input   any    `json:"input,omitempty"`
		ToolUse string `json:"tool_use_id,omitempty"`
		Content string `json:"content,omitempty"`
	}
	if len(req.Messages) != 3 {
		t.Fatalf("expected 3 mapped messages, got %d: %s", len(req.Messages), captured)
	}
	var assistantBlocks []block
	if err := json.Unmarshal(req.Messages[1].Content, &assistantBlocks); err != nil {
		t.Fatalf("assistant content: %v", err)
	}
	if len(assistantBlocks) != 2 || assistantBlocks[1].Type != "tool_use" || assistantBlocks[1].ID != "call_prev" || assistantBlocks[1].Name != "shell" {
		t.Fatalf("assistant blocks: %+v", assistantBlocks)
	}
	var userBlocks []block
	if err := json.Unmarshal(req.Messages[2].Content, &userBlocks); err != nil {
		t.Fatalf("tool_result content: %v", err)
	}
	if len(userBlocks) != 2 || userBlocks[0].Type != "tool_result" || userBlocks[0].ToolUse != "call_prev" || userBlocks[1].Type != "text" {
		t.Fatalf("tool_result blocks: %+v", userBlocks)
	}
	if userBlocks[0].Content != "a.txt\nb.txt" || userBlocks[1].Text != "now read a.txt" {
		t.Fatalf("block payloads wrong: %+v", userBlocks)
	}
	if req.Messages[2].Role != "user" {
		t.Fatalf("tool results must map to user role, got %s", req.Messages[2].Role)
	}
}

func TestResponsesToolsRequestAndFunctionCallStream(t *testing.T) {
	var captured []byte
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		b, _ := io.ReadAll(r.Body)
		captured = b
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(http.StatusOK)
		_, _ = io.WriteString(w, strings.Join([]string{
			`data: {"type":"response.output_text.delta","delta":"Reading "}`,
			`data: {"type":"response.output_text.delta","delta":"now"}`,
			`data: {"type":"response.function_call_arguments.delta","item_id":"fc_1","delta":"{\"path\":"}`,
			`data: {"type":"response.output_item.done","item":{"type":"function_call","id":"fc_1","call_id":"call_9","name":"read_file","arguments":"{\"path\":\"a.txt\"}"}}`,
			`data: {"type":"response.completed","response":{"usage":{"input_tokens":3,"output_tokens":4,"total_tokens":7}}}`,
			`data: [DONE]`,
			"",
		}, "\n"))
	}))
	defer srv.Close()

	r := NewResponsesProvider(srv.URL, "k", "gpt-4o")
	deltas, errs := r.StreamWithTools(context.Background(), toolConversation(), testToolSpecs)

	var text strings.Builder
	var calls []Delta
	var usage *Usage
	for d := range deltas {
		switch d.Kind {
		case DeltaText:
			text.WriteString(d.Text)
		case DeltaToolCall:
			calls = append(calls, d)
		case DeltaUsage:
			usage = d.Usage
		}
	}
	if err := <-errs; err != nil {
		t.Fatalf("stream error: %v", err)
	}
	if text.String() != "Reading now" {
		t.Fatalf("text=%q", text.String())
	}
	if len(calls) != 1 || calls[0].ToolName != "read_file" || calls[0].CallID != "call_9" || calls[0].ToolArgs != `{"path":"a.txt"}` {
		t.Fatalf("tool calls: %+v", calls)
	}
	if usage == nil || usage.TotalTokens != 7 {
		t.Fatalf("usage: %+v", usage)
	}

	var req struct {
		Tools []struct {
			Type        string          `json:"type"`
			Name        string          `json:"name"`
			Description string          `json:"description"`
			Parameters  json.RawMessage `json:"parameters"`
		} `json:"tools"`
		Input []struct {
			Type    string `json:"type"`
			Role    string `json:"role"`
			Content string `json:"content,omitempty"`
			CallID  string `json:"call_id,omitempty"`
			Name    string `json:"name,omitempty"`
			Args    string `json:"arguments,omitempty"`
			Output  string `json:"output,omitempty"`
		} `json:"input"`
	}
	if err := json.Unmarshal(captured, &req); err != nil {
		t.Fatalf("capture unmarshal: %v", err)
	}
	if len(req.Tools) != 2 || req.Tools[0].Type != "function" || req.Tools[0].Name != "read_file" {
		t.Fatalf("tools: %+v", req.Tools)
	}
	var sawPriorCall, sawCallOutput, sawUser bool
	for _, it := range req.Input {
		switch {
		case it.Type == "function_call" && it.CallID == "call_prev" && it.Name == "shell":
			sawPriorCall = true
		case it.Type == "function_call_output" && it.CallID == "call_prev" && strings.Contains(it.Output, "a.txt"):
			sawCallOutput = true
		case it.Role == "user" && it.Content == "now read a.txt":
			sawUser = true
		}
	}
	if !sawPriorCall || !sawCallOutput || !sawUser {
		t.Fatalf("input items wrong: priorCall=%v callOutput=%v user=%v items=%+v", sawPriorCall, sawCallOutput, sawUser, req.Input)
	}
}

func TestSchemaToJSONSchema(t *testing.T) {
	minimal := `{"required":["path"],"fields":{"path":"string","offset":"number","quiet":"boolean"}}`
	got := SchemaToJSONSchema(minimal)
	var obj struct {
		Type       string                    `json:"type"`
		Properties map[string]map[string]any `json:"properties"`
		Required   []string                  `json:"required"`
	}
	if err := json.Unmarshal(got, &obj); err != nil {
		t.Fatalf("invalid schema: %v (%s)", err, got)
	}
	if obj.Type != "object" || len(obj.Properties) != 3 || len(obj.Required) != 1 || obj.Required[0] != "path" {
		t.Fatalf("unexpected schema: %s", got)
	}
	if obj.Properties["path"]["type"] != "string" || obj.Properties["offset"]["type"] != "number" || obj.Properties["quiet"]["type"] != "boolean" {
		t.Fatalf("property types wrong: %s", got)
	}
}
