package update

import (
	"crypto/sha256"
	"encoding/hex"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// I6: update check is opt-in (no background check exists — this
// package is only called from the `update` command); the checksum
// is verified before replacing; a tampered download is refused.
func TestTamperedDownloadRefused(t *testing.T) {
	good := []byte("nikicode-binary-bytes")
	sum := sha256.Sum256(good)
	manifest := `{"version":"0.12.0","url":"URL","sha256":"SHA"}`
	mux := http.NewServeMux()
	mux.HandleFunc("/manifest.json", func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte(strings.ReplaceAll(strings.ReplaceAll(manifest, "URL", "http://"+r.Host+"/niki.bin"), "SHA", hex.EncodeToString(sum[:]))))
	})
	mux.HandleFunc("/niki.bin", func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Query().Get("tamper") == "1" {
			w.Write([]byte("evil-bytes"))
			return
		}
		w.Write(good)
	})
	srv := httptest.NewServer(mux)
	defer srv.Close()

	m, err := FetchManifest(srv.URL + "/manifest.json")
	if err != nil {
		t.Fatal(err)
	}
	if m.Version != "0.12.0" {
		t.Fatalf("[I6] manifest version = %q", m.Version)
	}
	dst := filepath.Join(t.TempDir(), "nikicode")
	if err := Download(m, dst); err != nil {
		t.Fatalf("[I6] good download refused: %v", err)
	}
	if data, _ := os.ReadFile(dst); string(data) != string(good) {
		t.Fatalf("[I6] downloaded bytes wrong")
	}
	t.Logf("[I6] manifest fetched, checksum verified before install")

	// Tampered payload: same manifest, evil bytes.
	m.URL = srv.URL + "/niki.bin?tamper=1"
	if err := Download(m, filepath.Join(t.TempDir(), "evil")); err == nil ||
		!strings.Contains(err.Error(), "tampered") {
		t.Fatalf("[I6] tampered download not refused: %v", err)
	}
	t.Logf("[I6] tampered download refused (sha256 mismatch)")
}
