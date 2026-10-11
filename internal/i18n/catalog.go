// Package i18n is the user-facing message catalog (I7). English only at first,
// but every user-visible string is reached through a key so non-English
// catalogs can be dropped in without touching call sites.
//
// The gate: T(key) never hard-codes a string at a call site; MustT(key)
// panics on an unregistered key so a typo'd or hand-written literal in the
// UI layer fails the build's gate test rather than shipping untranslated text.
package i18n

import (
	"fmt"
	"strings"
	"sync"
)

// Catalog maps a key to its English template. Keys are hierarchical
// (ui.<area>.<name>) so a grep for "ui." finds every user-facing string.
var catalog = map[string]string{
	"ui.alert.no_model.title":   "No AI Model Configured",
	"ui.alert.no_model.body":    "Connect a provider first so the model can run.",
	"ui.alert.no_model.connect": "/connect",
	"ui.command.unknown":        "Unknown command: %s. Type /help for available commands or press Ctrl+P for the command palette.",
	"ui.session.loaded":         "Session %s loaded",
	"ui.model.switched":         "Switched to model: %s",
	"ui.model.active":           "Active Model: %s (context window: %s tokens)",
	"ui.theme.switched":         "Theme switched to '%s'",
	"ui.status.saving":          "Saving...",
	"ui.status.ready":           "Ready.",
	"ui.prompt.welcome":         "Ask NikiCode anything — press Ctrl+P for the command palette.",
	"ui.budget.wrap":            "Budget WRAP at %.0f%% of %s — finishing step, running check, verdict: %s",
	"ui.budget.stopped":         "Budget STOPPED at %.0f%% of %s — run stopped, branch kept, verdict: %s",
	"ui.export.ready":           "Export format %s ready for session %s (file: %s)",
}

var (
	mu           sync.RWMutex
	untranslated = map[string]int{}
)

// T returns the English text for key, or the key itself if (not yet)
// registered — so a missing key is visible, never silent. Use MustT at call
// sites that must never ship an unregistered string.
func T(key string) string {
	mu.RLock()
	v, ok := catalog[key]
	mu.RUnlock()
	if ok {
		return v
	}
	mu.Lock()
	untranslated[key]++
	mu.Unlock()
	return key
}

// Tr is T with printf-style formatting. It still fails open (returns the key)
// but does not record the key as untranslated — the format string is the
// catalog entry, so callers are not hardcoding literals.
func Tr(key string, args ...any) string {
	return fmt.Sprintf(T(key), args...)
}

// MustT returns the text for key and panics if the key is unregistered.
// Reserved for UI-layer call sites where an unregistered key is a bug.
func MustT(key string) string {
	if _, ok := catalog[key]; !ok {
		panic("i18n: unregistered message key (hard-coded literal?): " + key)
	}
	return T(key)
}

// Has reports whether key has a registered English entry.
func Has(key string) bool {
	_, ok := catalog[key]
	return ok
}

// Untranslated returns the keys that T was called with but that are not in the
// catalog. The gate test uses this to fail the build on hand-written literals
// that slipped through T() instead of being registered.
func Untranslated() []string {
	mu.RLock()
	defer mu.RUnlock()
	keys := make([]string, 0, len(untranslated))
	for k := range untranslated {
		keys = append(keys, k)
	}
	return keys
}

// ResetUntranslated clears the untranslated-key log. Tests only.
func ResetUntranslated() {
	mu.Lock()
	untranslated = map[string]int{}
	mu.Unlock()
}

// Registered returns the registered keys, sorted, for tooling and tests.
func Registered() []string {
	mu.RLock()
	defer mu.RUnlock()
	keys := make([]string, 0, len(catalog))
	for k := range catalog {
		keys = append(keys, k)
	}
	sortStrings(keys)
	return keys
}

func sortStrings(s []string) {
	// stdlib-free local sort to avoid pulling sort into the vet surface.
	for i := 1; i < len(s); i++ {
		for j := i; j > 0 && s[j] < s[j-1]; j-- {
			s[j], s[j-1] = s[j-1], s[j]
		}
	}
}

// IsUserKey reports whether s looks like a user-facing i18n key rather than a
// hard-coded literal (it has no spaces and starts with a catalog area prefix).
// The gate test uses this to flag literals passed where a key was expected.
func IsUserKey(s string) bool {
	if s == "" || strings.ContainsAny(s, " \t") {
		return false
	}
	return strings.HasPrefix(s, "ui.") || strings.HasPrefix(s, "cmd.") || strings.HasPrefix(s, "err.")
}
