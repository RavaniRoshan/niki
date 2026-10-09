package format

import (
	"context"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
)

// FormatterResult holds the outcome of an automated format attempt.
type FormatterResult struct {
	Formatted bool   `json:"formatted"`
	Tool      string `json:"tool"`
	Error     string `json:"error,omitempty"`
}

type toolSpec struct {
	binary string
	args   []string
}

var formatters = map[string]toolSpec{
	".go":   {binary: "gofmt", args: []string{"-w"}},
	".rs":   {binary: "rustfmt", args: []string{}},
	".py":   {binary: "ruff", args: []string{"format"}},
	".ts":   {binary: "prettier", args: []string{"--write"}},
	".tsx":  {binary: "prettier", args: []string{"--write"}},
	".js":   {binary: "prettier", args: []string{"--write"}},
	".jsx":  {binary: "prettier", args: []string{"--write"}},
	".json": {binary: "prettier", args: []string{"--write"}},
	".c":    {binary: "clang-format", args: []string{"-i"}},
	".cpp":  {binary: "clang-format", args: []string{"-i"}},
	".h":    {binary: "clang-format", args: []string{"-i"}},
	".hpp":  {binary: "clang-format", args: []string{"-i"}},
	".sh":   {binary: "shfmt", args: []string{"-w"}},
}

// FormatFile formats a file on disk using available local tooling.
func FormatFile(ctx context.Context, filePath string) FormatterResult {
	ext := strings.ToLower(filepath.Ext(filePath))
	spec, ok := formatters[ext]
	if !ok {
		return FormatterResult{Formatted: false}
	}

	binPath, err := exec.LookPath(spec.binary)
	if err != nil {
		// Formatter binary not available in PATH; fail open cleanly without error
		return FormatterResult{Formatted: false, Tool: spec.binary}
	}

	args := append([]string{}, spec.args...)
	args = append(args, filePath)

	cmdCtx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()

	cmd := exec.CommandContext(cmdCtx, binPath, args...)
	if out, err := cmd.CombinedOutput(); err != nil {
		return FormatterResult{
			Formatted: false,
			Tool:      spec.binary,
			Error:     string(out),
		}
	}

	return FormatterResult{
		Formatted: true,
		Tool:      spec.binary,
	}
}
