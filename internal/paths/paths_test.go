package paths

import (
	"os"
	"path/filepath"
	"testing"
)

// seedLegacy builds a representative legacy tree: nested dirs, text,
// binary bytes, kept modes, and a symlink.
func seedLegacy(t *testing.T, home string) map[string]string {
	t.Helper()
	legacy := filepath.Join(home, Legacy)
	files := map[string]string{
		"config.toml":                "model = \"x\"\n",
		"sessions/sessions.db.jsonl": "{\"t\":1}\n",
		"nested/deep/blob.bin":       string([]byte{0, 1, 2, 255, 254, 253}),
		"log/niki.log":               "line1\nline2\n",
	}
	for rel, content := range files {
		p := filepath.Join(legacy, rel)
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, []byte(content), 0o600); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.Symlink("config.toml", filepath.Join(legacy, "config-link.toml")); err != nil {
		t.Fatal(err)
	}
	return files
}

func TestDirIsCanonical(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	Reset()
	if got := Dir(); got != filepath.Join(home, Canonical) {
		t.Fatalf("Dir() = %s, want %s", got, filepath.Join(home, Canonical))
	}
	if got := LegacyDir(); got != filepath.Join(home, Legacy) {
		t.Fatalf("LegacyDir() = %s, want %s", got, filepath.Join(home, Legacy))
	}
}

func TestMigrateLossless(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	Reset()
	want := seedLegacy(t, home)

	rep := Ensure()
	if !rep.Migrated {
		t.Fatal("Ensure() did not migrate a present legacy tree")
	}
	if rep.From != LegacyDir() || rep.To != Dir() {
		t.Fatalf("report dirs = %s -> %s", rep.From, rep.To)
	}
	if err := VerifyLossless(LegacyDir(), Dir()); err != nil {
		t.Fatalf("migration not lossless: %v", err)
	}
	// Content spot-check through the canonical path.
	for rel, content := range want {
		got, err := os.ReadFile(filepath.Join(Dir(), rel))
		if err != nil || string(got) != content {
			t.Fatalf("canonical %s mismatch: %v", rel, err)
		}
	}
	// Mode preserved.
	if st, err := os.Stat(filepath.Join(Dir(), "config.toml")); err != nil || st.Mode().Perm() != 0o600 {
		t.Fatalf("mode not preserved: %v %v", st, err)
	}
	// Symlink replicated.
	if link, err := os.Readlink(filepath.Join(Dir(), "config-link.toml")); err != nil || link != "config.toml" {
		t.Fatalf("symlink not replicated: %s %v", link, err)
	}
	// Marker written.
	if _, err := os.Stat(filepath.Join(Dir(), "MIGRATED_FROM")); err != nil {
		t.Fatalf("MIGRATED_FROM missing: %v", err)
	}
	// Legacy tree untouched.
	if _, err := os.Stat(filepath.Join(LegacyDir(), "config.toml")); err != nil {
		t.Fatalf("legacy tree moved instead of copied: %v", err)
	}
	// Second Ensure is a no-op (does not overwrite canonical data).
	if err := os.WriteFile(filepath.Join(Dir(), "config.toml"), []byte("changed"), 0o600); err != nil {
		t.Fatal(err)
	}
	Reset()
	rep2 := Ensure()
	if rep2.Migrated {
		t.Fatal("second Ensure() re-migrated over canonical data")
	}
	if got, _ := os.ReadFile(filepath.Join(Dir(), "config.toml")); string(got) != "changed" {
		t.Fatal("second Ensure() clobbered canonical data")
	}
}

func TestMigrateNoLegacy(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	Reset()
	rep := Ensure()
	if rep.Migrated {
		t.Fatal("Ensure() migrated with no legacy tree present")
	}
	if st, err := os.Stat(Dir()); err != nil || !st.IsDir() {
		t.Fatalf("canonical dir not created: %v", err)
	}
}

func TestMigrateKeepsExistingCanonical(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	seedLegacy(t, home)
	if err := os.MkdirAll(Dir(), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(Dir(), "mine.toml"), []byte("mine"), 0o644); err != nil {
		t.Fatal(err)
	}
	Reset()
	rep := Ensure()
	if rep.Migrated {
		t.Fatal("Ensure() overlaid an existing canonical tree")
	}
	if _, err := os.Stat(filepath.Join(Dir(), "config.toml")); !os.IsNotExist(err) {
		t.Fatal("legacy content leaked into an existing canonical tree")
	}
}

func TestDualEnv(t *testing.T) {
	t.Setenv("NIKICODE_BOOT_TRACE", "")
	t.Setenv("NIKI_BOOT_TRACE", "")
	Reset()
	if got := Env("BOOT_TRACE"); got != "" {
		t.Fatalf("Env with neither set = %q", got)
	}
	t.Setenv("NIKI_BOOT_TRACE", "1")
	if got, src := EnvSource("BOOT_TRACE"); got != "1" || src != "NIKI_BOOT_TRACE" {
		t.Fatalf("legacy fallback = %q via %q", got, src)
	}
	t.Setenv("NIKICODE_BOOT_TRACE", "2")
	if got, src := EnvSource("BOOT_TRACE"); got != "2" || src != "NIKICODE_BOOT_TRACE" {
		t.Fatalf("canonical should win: %q via %q", got, src)
	}
	if !EnvIs("BOOT_TRACE", "1", "2") {
		t.Fatal("EnvIs missed a set value")
	}
}
