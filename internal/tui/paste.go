package tui

import (
	"fmt"
	"regexp"
	"strings"
	"sync"
)

var (
	pasteMutex   sync.RWMutex
	pasteCounter int
	pasteTable   = make(map[string]string)
	pasteRegex   = regexp.MustCompile(`\[paste #(\d+) \+\d+ lines\]`)
)

// NormalizePasteText converts Windows CRLF and legacy CR into uniform newlines.
func NormalizePasteText(text string) string {
	text = strings.ReplaceAll(text, "\r\n", "\n")
	return strings.ReplaceAll(text, "\r", "\n")
}

// HandlePastedText inspects incoming paste payload. If it exceeds 10 lines
// or 1000 characters, it collapses it into an atomic token `[paste #N +L lines]`.
func HandlePastedText(raw string) (displayText string, isCollapsed bool) {
	norm := NormalizePasteText(raw)
	lines := strings.Split(norm, "\n")
	lineCount := len(lines)

	if lineCount > 10 || len(norm) > 1000 {
		pasteMutex.Lock()
		pasteCounter++
		id := fmt.Sprintf("%d", pasteCounter)
		token := fmt.Sprintf("[paste #%s +%d lines]", id, lineCount)
		pasteTable[id] = norm
		pasteMutex.Unlock()
		return token, true
	}
	return norm, false
}

// ExpandPasteTokens expands all collapsed tokens in the prompt before transmission to the model.
func ExpandPasteTokens(text string) string {
	pasteMutex.RLock()
	defer pasteMutex.RUnlock()

	return pasteRegex.ReplaceAllStringFunc(text, func(match string) string {
		sub := pasteRegex.FindStringSubmatch(match)
		if len(sub) > 1 {
			if full, ok := pasteTable[sub[1]]; ok {
				return full
			}
		}
		return match
	})
}
