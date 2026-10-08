package tools

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"sync"
)

type UserQuestion struct {
	Header      string   `json:"header"`
	Question    string   `json:"question"`
	Options     []string `json:"options"`
	AllowCustom bool     `json:"allow_custom"`
}

type AskUserQuestionTool struct {
	Base
	mu         sync.RWMutex
	isSubagent bool
}

func NewAskUserQuestionTool() *AskUserQuestionTool {
	return &AskUserQuestionTool{
		Base: Base{
			SchemaStr: `{"required":["questions"]}`,
		},
	}
}

func (t *AskUserQuestionTool) Name() string        { return "ask_user_question" }
func (t *AskUserQuestionTool) Description() string { return "Prompt the user with structured multiple-choice questions (1-4 questions, 2-4 options, header <= 12 chars) with write-in escape hatch" }

type askUserQuestionArgs struct {
	Questions []UserQuestion `json:"questions"`
}

func (t *AskUserQuestionTool) SetSubagent(subagent bool) {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.isSubagent = subagent
}

func (t *AskUserQuestionTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	t.mu.RLock()
	sub := t.isSubagent
	t.mu.RUnlock()

	if sub {
		return ToolResult{
			Output:  "ask_user_question is disabled within subagent contexts to prevent blocking unattended execution",
			IsError: true,
		}, nil
	}

	var a askUserQuestionArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}

	if len(a.Questions) < 1 || len(a.Questions) > 4 {
		return ToolResult{
			Output:  fmt.Sprintf("questions count must be between 1 and 4 (got %d)", len(a.Questions)),
			IsError: true,
		}, nil
	}

	for i, q := range a.Questions {
		header := strings.TrimSpace(q.Header)
		if len(header) > 12 {
			return ToolResult{
				Output:  fmt.Sprintf("question %d header %q exceeds 12 characters (len=%d)", i+1, q.Header, len(header)),
				IsError: true,
			}, nil
		}
		if strings.TrimSpace(q.Question) == "" {
			return ToolResult{
				Output:  fmt.Sprintf("question %d text cannot be empty", i+1),
				IsError: true,
			}, nil
		}
		if len(q.Options) < 2 || len(q.Options) > 4 {
			return ToolResult{
				Output:  fmt.Sprintf("question %d options count must be between 2 and 4 (got %d)", i+1, len(q.Options)),
				IsError: true,
			}, nil
		}
	}

	var sb strings.Builder
	fmt.Fprintf(&sb, "Presented %d question(s) to user:\n\n", len(a.Questions))
	for i, q := range a.Questions {
		header := q.Header
		if header == "" {
			header = fmt.Sprintf("Question %d", i+1)
		}
		fmt.Fprintf(&sb, "[%s] %s\n", header, q.Question)
		for j, opt := range q.Options {
			fmt.Fprintf(&sb, "  %d) %s\n", j+1, opt)
		}
		if q.AllowCustom {
			fmt.Fprintf(&sb, "  %d) Other (type custom response)\n", len(q.Options)+1)
		}
		sb.WriteString("\n")
	}

	return ToolResult{Output: strings.TrimSpace(sb.String())}, nil
}

func (t *AskUserQuestionTool) IsConcurrencySafe() bool { return false }
func (t *AskUserQuestionTool) IsReadOnly() bool        { return true }
