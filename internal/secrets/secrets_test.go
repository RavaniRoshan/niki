package secrets

import (
	"errors"
	"testing"
)

// Fake backend that records calls and can be made to fail (keychain
// unavailable — headless CI, missing D-Bus, etc).
type fakeBackend struct {
	setCalls []string
	failSet  bool
	failGet  bool
	store    map[string]string
}

func (f *fakeBackend) Set(service, account, key string) error {
	if f.failSet {
		return errors.New("cannot connect to Secret Service")
	}
	f.setCalls = append(f.setCalls, service+"/"+account)
	f.store[service+"/"+account] = key
	return nil
}

func (f *fakeBackend) Get(service, account string) (string, error) {
	if f.failGet {
		return "", errors.New("cannot connect to Secret Service")
	}
	v, ok := f.store[service+"/"+account]
	if !ok {
		return "", errors.New("secret not found")
	}
	return v, nil
}

func (f *fakeBackend) Delete(service, account string) error {
	if f.failSet {
		return errors.New("cannot connect to Secret Service")
	}
	delete(f.store, service+"/"+account)
	return nil
}

// I4: when the keychain backend is available, Set persists there and Get
// reads back — no process memory involved.
func TestSetPersistsToKeychainWhenAvailable(t *testing.T) {
	fb := &fakeBackend{store: map[string]string{}}
	UseBackendForTest(t, fb)

	persisted, err := Set("anthropic", "sk-planted-keychain-test")
	if err != nil {
		t.Fatalf("Set: %v", err)
	}
	if !persisted {
		t.Fatal("expected keychain persistence")
	}
	if len(fb.setCalls) != 1 || fb.setCalls[0] != Service+"/anthropic" {
		t.Fatalf("expected one %s/anthropic write, got %v", Service, fb.setCalls)
	}
	got, ok := Get("anthropic")
	if !ok || got != "sk-planted-keychain-test" {
		t.Fatalf("Get=%q ok=%v", got, ok)
	}
	t.Logf("keychain round-trip ok: %s/anthropic (service=%s)", "anthropic", Service)
}

// I4: when the keychain is unavailable, Set degrades to a session-scoped
// store and reports not-persisted so the caller can surface the env-var
// fallback hint — the key is still resolvable in this process.
func TestSetFallsBackToSessionWhenKeychainUnavailable(t *testing.T) {
	fb := &fakeBackend{failSet: true, failGet: true, store: map[string]string{}}
	UseBackendForTest(t, fb)

	persisted, err := Set("openai", "sk-session-only-key")
	if err != nil {
		t.Fatalf("Set must not error on keychain unavailability: %v", err)
	}
	if persisted {
		t.Fatal("expected session fallback (persisted=false)")
	}
	if len(fb.setCalls) != 0 {
		t.Fatalf("must not claim a keychain write: %v", fb.setCalls)
	}
	got, ok := Get("openai")
	if !ok || got != "sk-session-only-key" {
		t.Fatalf("session fallback broken: Get=%q ok=%v", got, ok)
	}
	t.Logf("session fallback ok (keychain unavailable): openai resolved from process memory")
}

// Get must prefer a session write (made when the keychain was down or the
// key was rotated this process) over a stale keychain entry.
func TestSessionWritePreferredOverKeychain(t *testing.T) {
	fb := &fakeBackend{store: map[string]string{Service + "/anthropic": "old-key"}}
	UseBackendForTest(t, fb)
	if _, err := Set("anthropic", "new-key"); err != nil {
		t.Fatal(err)
	}
	// New write went to the keychain here (backend healthy) — rotate by
	// forcing the session path first.
	fb.failSet = true
	if _, err := Set("anthropic", "rotated-key"); err != nil {
		t.Fatal(err)
	}
	got, ok := Get("anthropic")
	if !ok || got != "rotated-key" {
		t.Fatalf("session value must win: Get=%q ok=%v", got, ok)
	}
	t.Logf("resolution order verified: session rotation (%q) preferred over keychain entry", got)
}
