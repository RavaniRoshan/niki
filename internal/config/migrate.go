package config

import (
	"fmt"
	"os"
	"strings"

	"github.com/pelletier/go-toml/v2"
)

// ConfigSchemaVersion is the current config schema (U1). Files
// without a version are v0 (legacy).
const ConfigSchemaVersion = 1

// Migration describes one applied change.
type Migration struct {
	Description string
}

// Migrate upgrades a config file to the current schema (U1). With
// dryRun it only reports planned changes and writes nothing.
// Otherwise it backs the original up to path+".bak" before writing.
// Rules (v0 -> v1):
//   - stamp version = 1 (missing version means legacy);
//   - drop a legacy on-disk `api_key` (keys live in the OS
//     keychain or env now — I4), reporting it instead of copying.
func Migrate(path string, dryRun bool) ([]Migration, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var doc map[string]any
	if err := toml.Unmarshal(data, &doc); err != nil {
		return nil, fmt.Errorf("bad config toml: %w", err)
	}
	var planned []Migration
	ver, _ := doc["version"].(int64)
	if ver == 0 && doc["version"] == nil {
		planned = append(planned, Migration{Description: "stamp version = 1 (legacy unversioned file)"})
		doc["version"] = int64(ConfigSchemaVersion)
	} else if int(ver) != ConfigSchemaVersion {
		return nil, fmt.Errorf("unsupported config version %d (want %d)", ver, ConfigSchemaVersion)
	}
	if prov, ok := doc["provider"].(map[string]any); ok {
		if key, ok := prov["api_key"].(string); ok && strings.TrimSpace(key) != "" {
			planned = append(planned, Migration{Description: "remove on-disk provider.api_key (move it to the OS keychain or env; never stored in files)"})
			delete(prov, "api_key")
		}
	}
	if dryRun {
		return planned, nil
	}
	if len(planned) == 0 {
		return nil, nil
	}
	backup := path + ".bak"
	if err := os.WriteFile(backup, data, 0o600); err != nil {
		return nil, fmt.Errorf("backup failed: %w", err)
	}
	out, err := toml.Marshal(doc)
	if err != nil {
		return nil, err
	}
	if err := os.WriteFile(path, out, 0o600); err != nil {
		return nil, fmt.Errorf("write migrated config: %w", err)
	}
	return planned, nil
}
