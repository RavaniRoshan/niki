package mcp

import (
	"regexp"
	"strings"
)

// instructionPatterns match tool-description text that reads like
// an instruction to the agent rather than a description of a tool
// (T3). Descriptions are untrusted server content: a match flags
// the tool, which is then hidden from the catalog and uncallable.
var instructionPatterns = []*regexp.Regexp{
	regexp.MustCompile(`(?i)ignore\s+(all\s+)?(previous|prior|above)\s+instructions?`),
	regexp.MustCompile(`(?i)disregard\s+.*instructions?`),
	regexp.MustCompile(`(?i)\b(exfiltrat|send|upload|post)\b.*\b(credentials?|secrets?|tokens?|passwords?|keys?)\b`),
	regexp.MustCompile(`(?i)\b(run|execute)\s+(this\s+)?commands?\b`),
	regexp.MustCompile(`(?i)\|\s*sh\s*$`),
	regexp.MustCompile(`(?i)you\s+(must|should|have to)\s+`),
	regexp.MustCompile(`(?i)\b(system|developer)\s+prompt\b`),
}

// ScanDescription reports whether a tool description looks like an
// instruction-injection attempt, with a human reason. Empty or
// plain descriptions pass.
func ScanDescription(desc string) (bool, string) {
	desc = strings.TrimSpace(desc)
	if desc == "" {
		return false, ""
	}
	for _, p := range instructionPatterns {
		if p.MatchString(desc) {
			return true, "instruction-like text: " + p.String()
		}
	}
	return false, ""
}
