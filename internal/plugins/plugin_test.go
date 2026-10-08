package plugins

import (
	"crypto/sha256"
	"encoding/hex"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/RavaniRoshan/niki/internal/skills"
)

func TestPluginLoadingAndSkillsBundling(t *testing.T) {
	tmpDir := t.TempDir()

	// 1. Create a bundled skill inside the plugin
	skillsDir := filepath.Join(tmpDir, "skills")
	os.MkdirAll(skillsDir, 0o755)
	skillFile := filepath.Join(skillsDir, "review.md")
	os.WriteFile(skillFile, []byte("---\nname: code-review\ndescription: Review pull request\ncontext: fork\n---\nReview instructions\n"), 0o644)

	// 2. Create plugin.json manifest
	manifestJSON := `{
		"name": "dev-kit",
		"version": "1.0.0",
		"description": "Developer toolkit plugin",
		"skills_dir": "./skills",
		"hooks": [
			{
				"point": "pre_tool_use",
				"command": "echo",
				"args": ["PreToolUse hook executed"]
			}
		],
		"mcp_servers": {
			"db_server": {
				"command": "node",
				"args": ["srv.js"]
			}
		}
	}`
	manifestPath := filepath.Join(tmpDir, "plugin.json")
	os.WriteFile(manifestPath, []byte(manifestJSON), 0o644)

	mgr := NewManager()
	plugin, err := mgr.LoadPlugin(manifestPath)
	if err != nil {
		t.Fatalf("failed loading plugin: %v", err)
	}

	if plugin.Manifest.Name != "dev-kit" {
		t.Fatalf("unexpected plugin name: %s", plugin.Manifest.Name)
	}
	if len(plugin.Skills) != 1 || plugin.Skills[0].Name != "code-review" {
		t.Fatalf("bundled skills not discovered properly: %v", plugin.Skills)
	}
	if plugin.Skills[0].Context != "fork" {
		t.Fatalf("context: fork was not parsed: %s", plugin.Skills[0].Context)
	}
}

func TestCommandHookExecutionAndTrustGating(t *testing.T) {
	tmpDir := t.TempDir()
	mgr := NewManager()

	// 1. Create an executable hook script
	scriptPath := filepath.Join(tmpDir, "hook.sh")
	scriptContent := "#!/bin/bash\nread -r input\necho \"Hook processed: $input\"\nexit 0\n"
	os.WriteFile(scriptPath, []byte(scriptContent), 0o755)

	scriptHash := sha256.Sum256([]byte(scriptContent))
	hashHex := hex.EncodeToString(scriptHash[:])

	// 2. Successful hook run with matching trusted hash
	hookCfg := HookConfig{
		Point:   "pre_tool_use",
		Command: scriptPath,
		SHA256:  hashHex,
		Timeout: 5,
	}

	payload := []byte(`{"tool":"shell","command":"ls -la"}`)
	out, err := mgr.ExecuteHook(hookCfg, tmpDir, payload)
	if err != nil {
		t.Fatalf("hook execution error: %v %s", err, out)
	}
	if !strings.Contains(out, "Hook processed: {\"tool\":\"shell\"") {
		t.Fatalf("unexpected hook output: %s", out)
	}

	// 3. SHA-256 trust gating failure: mismatch blocks execution
	tamperedCfg := HookConfig{
		Point:   "pre_tool_use",
		Command: scriptPath,
		SHA256:  "deadbeef00000000000000000000000000000000000000000000000000000000",
	}
	_, err = mgr.ExecuteHook(tamperedCfg, tmpDir, payload)
	if err == nil || !strings.Contains(err.Error(), "trust check") {
		t.Fatalf("expected SHA-256 trust failure, got: %v", err)
	}

	// 4. Hook non-zero exit code: blocks tool call
	failingScript := filepath.Join(tmpDir, "fail_hook.sh")
	failContent := "#!/bin/bash\necho \"Blocked by policy\"\nexit 1\n"
	os.WriteFile(failingScript, []byte(failContent), 0o755)

	failCfg := HookConfig{
		Point:   "pre_tool_use",
		Command: failingScript,
	}
	_, err = mgr.ExecuteHook(failCfg, tmpDir, payload)
	if err == nil || !strings.Contains(err.Error(), "exit code") {
		t.Fatalf("expected non-zero exit code failure, got: %v", err)
	}
}

func TestSkillContextFork(t *testing.T) {
	tmpDir := t.TempDir()
	skillDir := filepath.Join(tmpDir, "fork_skill")
	os.MkdirAll(skillDir, 0o755)
	skillPath := filepath.Join(skillDir, "SKILL.md")
	content := "---\nname: isolated-task\ndescription: Run in a fork\ncontext: fork\n---\nForked instructions.\n"
	os.WriteFile(skillPath, []byte(content), 0o644)

	skillsList, err := skills.Discover(tmpDir)
	if err != nil || len(skillsList) != 1 {
		t.Fatalf("discover failed: %v", err)
	}
	if skillsList[0].Context != "fork" {
		t.Fatalf("expected context to be fork, got %q", skillsList[0].Context)
	}
}
