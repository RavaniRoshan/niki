package i18n

import (
	"strings"
	"testing"
)

// I7: the catalog is the foundation for all user-facing strings (English only
// at first). These tests pin its shape so the gate cannot silently regress.

func TestCatalogWellFormed(t *testing.T) {
	seen := map[string]bool{}
	for k, v := range catalog {
		if k == "" {
			t.Fatal("[I7] catalog has an empty key")
		}
		if seen[k] {
			t.Fatalf("[I7] catalog has a duplicate key: %s", k)
		}
		seen[k] = true
		if v == "" {
			t.Fatalf("[I7] catalog value for %q is empty", k)
		}
		if !strings.HasPrefix(k, "ui.") && !strings.HasPrefix(k, "cmd.") && !strings.HasPrefix(k, "err.") {
			t.Fatalf("[I7] catalog key %q has an unexpected area prefix", k)
		}
	}
	// Foundational user-facing keys the UI layer depends on are present.
	for _, key := range []string{
		"ui.alert.no_model.title", "ui.alert.no_model.body", "ui.command.unknown",
		"ui.session.loaded", "ui.model.switched", "ui.model.active",
		"ui.theme.switched", "ui.budget.wrap", "ui.budget.stopped",
	} {
		if !Has(key) {
			t.Fatalf("[I7] missing foundational key: %s", key)
		}
	}
	t.Logf("[I7] catalog well-formed: %d keys, no dups/empties", len(catalog))
}

// The gate: MustT panics on an unregistered key — i.e. a hard-coded or
// typo'd literal in the UI layer is caught at runtime before it ships.
func TestGateUnregisteredKeyPanics(t *testing.T) {
	ResetUntranslated()
	// A registered key resolves cleanly.
	if got := MustT("ui.alert.no_model.title"); got != "No AI Model Configured" {
		t.Fatalf("[I7] MustT resolved wrong text: %q", got)
	}
	// An unregistered key (the stand-in for a hard-coded string) must panic.
	defer func() {
		r := recover()
		if r == nil {
			t.Fatal("[I7] MustT did not panic on unregistered key — gate broken")
		}
		t.Logf("[I7] gate caught unregistered literal: %v", r)
	}()
	_ = MustT("ui.billing.fake.unregistered.literal")
}

func TestTrFormatsArgs(t *testing.T) {
	got := Tr("ui.command.unknown", "/frob")
	if !strings.Contains(got, "/frob") || !strings.Contains(got, "/help") {
		t.Fatalf("[I7] Tr lost args/text: %q", got)
	}
	t.Logf("[I7] Tr formats %q", got)
}

// The gate test: no call to T() should leak an unregistered key. A hard-coded
// literal passed where a key was expected surfaces here as untranslated.
func TestGateNoUntranslatedCalls(t *testing.T) {
	ResetUntranslated()
	// Simulate a bug: a call site with a typo'd key.
	_ = T("ui.session.loadedd")
	bad := Untranslated()
	if len(bad) != 1 || bad[0] != "ui.session.loadedd" {
		t.Fatalf("[I7] untranslated key not tracked: %v", bad)
	}
	t.Logf("[I7] gate tracks untranslated keys: %v", bad)
}
