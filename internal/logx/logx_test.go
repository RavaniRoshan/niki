package logx

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// I4: keys never appear in log files — a planted key handed to the logger
// is redacted before the bytes hit disk.
func TestLogNeverWritesPlantedKey(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "nikicode.log")
	l, err := Open(path)
	if err != nil {
		t.Fatal(err)
	}
	defer l.Close()

	planted := "sk-planted-I4-log-canary-abcdef123456"
	l.Info("connect", map[string]string{
		"provider": "anthropic",
		"key":      planted,
		"header":   "Bearer " + planted,
	})
	l.Error("provider handshake failed", errString("401 with key "+planted), nil)
	l.Close()

	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	content := string(data)
	if strings.Contains(content, planted) {
		t.Fatalf("planted key leaked into log:\n%s", content)
	}
	if !strings.Contains(content, "[REDACTED]") {
		t.Fatalf("expected redaction markers in log:\n%s", content)
	}
	if !strings.Contains(content, "connect") || !strings.Contains(content, "provider handshake failed") {
		t.Fatalf("redaction damaged normal log events:\n%s", content)
	}
	t.Logf("log scan ok (planted key absent, events intact):\n%s", content)
}

type errString string

func (e errString) Error() string { return string(e) }
