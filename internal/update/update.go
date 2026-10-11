// Package update implements the opt-in update check (I6): nothing
// runs unless the user invokes `nikicode update`. A manifest is
// fetched, the payload's SHA-256 is verified BEFORE anything is
// trusted, and a tampered download is refused. There is no
// background or automatic check anywhere in the codebase.
package update

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"time"
)

// Manifest describes one release artifact.
type Manifest struct {
	Version string `json:"version"`
	URL     string `json:"url"`
	SHA256  string `json:"sha256"`
}

// client times out fast: an update check must never hang a session.
var client = &http.Client{Timeout: 30 * time.Second}

// FetchManifest downloads and parses the manifest.
func FetchManifest(manifestURL string) (Manifest, error) {
	resp, err := client.Get(manifestURL)
	if err != nil {
		return Manifest{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != 200 {
		return Manifest{}, fmt.Errorf("manifest status %d", resp.StatusCode)
	}
	data, err := io.ReadAll(io.LimitReader(resp.Body, 1<<20))
	if err != nil {
		return Manifest{}, err
	}
	var m Manifest
	if err := json.Unmarshal(data, &m); err != nil {
		return Manifest{}, fmt.Errorf("bad manifest: %w", err)
	}
	if m.Version == "" || m.URL == "" || m.SHA256 == "" {
		return Manifest{}, fmt.Errorf("incomplete manifest")
	}
	return m, nil
}

// Download verifies the payload against the manifest hash BEFORE
// writing it to dst (I6: checksum before replacing). A mismatch is
// a refusal, and any partial file is removed.
func Download(m Manifest, dst string) error {
	resp, err := client.Get(m.URL)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode != 200 {
		return fmt.Errorf("artifact status %d", resp.StatusCode)
	}
	data, err := io.ReadAll(io.LimitReader(resp.Body, 64<<20))
	if err != nil {
		return err
	}
	sum := sha256.Sum256(data)
	if hex.EncodeToString(sum[:]) != m.SHA256 {
		return fmt.Errorf("refusing tampered download: sha256 mismatch (want %.12s)", m.SHA256)
	}
	tmp := dst + ".tmp"
	if err := os.WriteFile(tmp, data, 0o755); err != nil {
		return err
	}
	return os.Rename(tmp, dst)
}
