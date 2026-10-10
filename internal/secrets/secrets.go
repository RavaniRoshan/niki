// Package secrets stores API keys in the OS keychain when available, with
// a process-scoped session store as the intermediate fallback and
// environment variables as the final fallback (resolved by
// config.ResolveAPIKey). Keys written through this package never touch
// config files.
package secrets

import (
	"errors"
	"os"
	"sync"

	keyring "github.com/zalando/go-keyring"
)

// Service is the keychain service name used for all niki keys.
const Service = "nikicode"

// Backend is the storage SPI; production uses the OS keychain.
type Backend interface {
	Set(service, account, key string) error
	Get(service, account string) (string, error)
	Delete(service, account string) error
}

type keyringBackend struct{}

func (keyringBackend) Set(service, account, key string) error {
	return keyring.Set(service, account, key)
}

func (keyringBackend) Get(service, account string) (string, error) {
	return keyring.Get(service, account)
}

func (keyringBackend) Delete(service, account string) error {
	return keyring.Delete(service, account)
}

var (
	mu       sync.Mutex
	backend  Backend = keyringBackend{}
	session          = map[string]string{}
	disabled bool    // keychain proven unavailable this process
)

// UseBackendForTest swaps the backend (and clears session state) for the
// duration of a test, restoring the previous state afterwards.
func UseBackendForTest(t interface{ Cleanup(func()) }, b Backend) {
	mu.Lock()
	oldBackend, oldSession, oldDisabled := backend, session, disabled
	backend = b
	session = map[string]string{}
	disabled = false
	mu.Unlock()
	t.Cleanup(func() {
		mu.Lock()
		backend, session, disabled = oldBackend, oldSession, oldDisabled
		mu.Unlock()
	})
}

// isNotFound reports whether err is the keychain's normal "no such
// secret" answer (as opposed to a transport/service failure).
func isNotFound(err error) bool {
	if errors.Is(err, keyring.ErrNotFound) || errors.Is(err, os.ErrNotExist) {
		return true
	}
	return false
}

// Set stores key for account. It returns true when the key was persisted
// to the OS keychain and false when it fell back to process memory (the
// keychain was unavailable — callers should surface the env-var hint).
// Set never returns an error for keychain unavailability.
func Set(account, key string) (keychain bool, err error) {
	if account == "" {
		return false, errors.New("secrets: empty keychain account")
	}
	mu.Lock()
	defer mu.Unlock()
	if !disabled {
		if kerr := backend.Set(Service, account, key); kerr == nil {
			// A fresh keychain write supersedes any session copy.
			delete(session, account)
			return true, nil
		} else {
			// Unavailable (no Secret Service / locked session / ...):
			// degrade for the rest of this process.
			disabled = true
		}
	}
	session[account] = key
	return false, nil
}

// Get resolves key for account: session store first, then keychain.
// It reports false when nothing is stored (callers fall back to env).
func Get(account string) (string, bool) {
	if account == "" {
		return "", false
	}
	mu.Lock()
	defer mu.Unlock()
	if k, ok := session[account]; ok && k != "" {
		return k, true
	}
	if disabled {
		return "", false
	}
	k, err := backend.Get(Service, account)
	if err != nil {
		if !isNotFound(err) {
			disabled = true
		}
		return "", false
	}
	return k, k != ""
}

// Delete removes the key for account from both stores.
func Delete(account string) error {
	if account == "" {
		return errors.New("secrets: empty keychain account")
	}
	mu.Lock()
	defer mu.Unlock()
	delete(session, account)
	if disabled {
		return nil
	}
	if err := backend.Delete(Service, account); err != nil && !isNotFound(err) {
		disabled = true
		return err
	}
	return nil
}
