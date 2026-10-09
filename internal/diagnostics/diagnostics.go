package diagnostics

import (
	"bufio"
	"bytes"
	"context"
	"fmt"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"time"
)

// DiagnosticItem holds an individual compiler or linter diagnostic issue.
type DiagnosticItem struct {
	File     string `json:"file"`
	Line     int    `json:"line"`
	Col      int    `json:"col"`
	Severity string `json:"severity"`
	Message  string `json:"message"`
}

// Report holds aggregated diagnostics for a target file.
type Report struct {
	File  string           `json:"file"`
	Items []DiagnosticItem `json:"items"`
}

// FormatXML serializes the diagnostic issues into structured XML for model consumption.
func (r Report) FormatXML() string {
	if len(r.Items) == 0 {
		return ""
	}
	var sb strings.Builder
	sb.WriteString(fmt.Sprintf("<diagnostics file=\"%s\">\n", r.File))
	for _, it := range r.Items {
		sb.WriteString(fmt.Sprintf("%s [%d:%d] %s\n", it.Severity, it.Line, it.Col, it.Message))
	}
	sb.WriteString("</diagnostics>")
	return sb.String()
}

var goErrorRe = regexp.MustCompile(`^(.+?):(\d+):(\d+):\s*(.+)$`)

// CollectDiagnostics runs available compilers/linters to extract issues on modified files.
func CollectDiagnostics(ctx context.Context, root string, file string) Report {
	ext := strings.ToLower(filepath.Ext(file))
	relFile, err := filepath.Rel(root, file)
	if err != nil {
		relFile = file
	}

	report := Report{File: relFile}
	if ext != ".go" {
		return report
	}

	goBin, err := exec.LookPath("go")
	if err != nil {
		return report
	}

	cmdCtx, cancel := context.WithTimeout(ctx, 4*time.Second)
	defer cancel()

	cmd := exec.CommandContext(cmdCtx, goBin, "vet", "./...")
	cmd.Dir = root
	out, _ := cmd.CombinedOutput()

	scanner := bufio.NewScanner(bytes.NewReader(out))
	for scanner.Scan() {
		line := scanner.Text()
		matches := goErrorRe.FindStringSubmatch(line)
		if len(matches) == 5 {
			lNum, _ := strconv.Atoi(matches[2])
			cNum, _ := strconv.Atoi(matches[3])
			report.Items = append(report.Items, DiagnosticItem{
				File:     matches[1],
				Line:     lNum,
				Col:      cNum,
				Severity: "ERROR",
				Message:  matches[4],
			})
		}
	}

	if len(report.Items) > 15 {
		report.Items = report.Items[:15]
	}

	return report
}
