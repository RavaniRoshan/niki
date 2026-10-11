package config

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// U1: configuration migrations with a dry-run mode and a backup.
func TestMigrateDryRunAndBackup(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "nikicode.toml")
	legacy := "[provider]\nname = \"mock\"\napi_key = \"sk-should-leave-the-file\"\n"
	if err := os.WriteFile(path, []byte(legacy), 0o644); err != nil {
		t.Fatal(err)
	}

	// Dry run: reports, writes nothing.
	planned, err := Migrate(path, true)
	if err != nil {
		t.Fatal(err)
	}
	if len(planned) != 2 {
		t.Fatalf("[U1] dry-run planned %d changes: %v", len(planned), planned)
	}
	after, _ := os.ReadFile(path)
	if string(after) != legacy {
		t.Fatalf("[U1] dry run modified the file")
	}
	if _, err := os.Stat(path + ".bak"); !os.IsNotExist(err) {
		t.Fatalf("[U1] dry run wrote a backup")
	}
	t.Logf("[U1] dry run plans without touching: %q, %q", planned[0].Description, planned[1].Description)

	// Real run: backup first, then migrate.
	planned, err = Migrate(path, false)
	if err != nil {
		t.Fatal(err)
	}
	backup, err := os.ReadFile(path + ".bak")
	if err != nil || string(backup) != legacy {
		t.Fatalf("[U1] backup missing or wrong: %v", err)
	}
	migrated, _ := os.ReadFile(path)
	if strings.Contains(string(migrated), "sk-should-leave-the-file") {
		t.Fatalf("[U1] on-disk key survived migration")
	}
	if !strings.Contains(string(migrated), "version = 1") {
		t.Fatalf("[U1] version not stamped:\n%s", migrated)
	}
	// Migrated file still loads.
	if _, err := Load(path); err != nil {
		t.Fatalf("[U1] migrated file does not load: %v", err)
	}
	t.Logf("[U1] backup written, key removed, version stamped, file loads")
}
