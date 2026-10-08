package plugins

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/RavaniRoshan/niki/internal/skills"
)

type HookConfig struct {
	Point   string `json:"point"`   // pre_tool_use | post_tool_use | session_start | user_prompt_submit | stop
	Command string `json:"command"` // command or script path
	Args    []string `json:"args,omitempty"`
	SHA256  string `json:"sha256,omitempty"` // trusted hash
	Timeout int    `json:"timeout_seconds,omitempty"`
}

type MCPServerDef struct {
	Command string            `json:"command"`
	Args    []string          `json:"args,omitempty"`
	Env     map[string]string `json:"env,omitempty"`
}

type Manifest struct {
	Name        string                  `json:"name"`
	Version     string                  `json:"version"`
	Description string                  `json:"description"`
	SkillsDir   string                  `json:"skills_dir,omitempty"`
	Hooks       []HookConfig            `json:"hooks,omitempty"`
	MCPServers  map[string]MCPServerDef `json:"mcp_servers,omitempty"`
}

type Plugin struct {
	Manifest Manifest
	Dir      string
	Skills   []skills.Skill
}

type Manager struct {
	mu           sync.RWMutex
	plugins      map[string]*Plugin
	trustedHashes map[string]bool
}

func NewManager() *Manager {
	return &Manager{
		plugins:       make(map[string]*Plugin),
		trustedHashes: make(map[string]bool),
	}
}

func (m *Manager) TrustHash(hash string) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.trustedHashes[strings.ToLower(hash)] = true
}

func (m *Manager) LoadPlugin(manifestPath string) (*Plugin, error) {
	data, err := os.ReadFile(manifestPath)
	if err != nil {
		return nil, fmt.Errorf("failed reading plugin manifest %s: %w", manifestPath, err)
	}

	var manifest Manifest
	if err := json.Unmarshal(data, &manifest); err != nil {
		return nil, fmt.Errorf("invalid plugin manifest %s: %w", manifestPath, err)
	}

	pluginDir := filepath.Dir(manifestPath)
	p := &Plugin{
		Manifest: manifest,
		Dir:      pluginDir,
	}

	// Discover skills inside plugin if specified
	if manifest.SkillsDir != "" {
		resolvedSkillsDir := filepath.Join(pluginDir, manifest.SkillsDir)
		if discovered, err := skills.Discover(resolvedSkillsDir); err == nil {
			p.Skills = discovered
		}
	}

	m.mu.Lock()
	m.plugins[manifest.Name] = p
	m.mu.Unlock()

	return p, nil
}

func (m *Manager) ListPlugins() []*Plugin {
	m.mu.RLock()
	defer m.mu.RUnlock()
	var list []*Plugin
	for _, p := range m.plugins {
		list = append(list, p)
	}
	return list
}

// ExecuteHook runs a command hook with JSON stdin, exit codes, and SHA-256 trust gating.
func (m *Manager) ExecuteHook(h HookConfig, baseDir string, payload []byte) (string, error) {
	cmdPath := h.Command
	if !filepath.IsAbs(cmdPath) && baseDir != "" {
		cmdPath = filepath.Join(baseDir, cmdPath)
	}

	// SHA-256 trust gating
	if h.SHA256 != "" {
		fileBytes, err := os.ReadFile(cmdPath)
		if err == nil {
			sum := sha256.Sum256(fileBytes)
			currentHash := hex.EncodeToString(sum[:])
			if !strings.EqualFold(currentHash, h.SHA256) {
				return "", fmt.Errorf("hook %s failed SHA-256 trust check: got %s, expected %s", cmdPath, currentHash, h.SHA256)
			}
		} else {
			// If not a readable file (e.g. system binary), check registered trusted hashes
			m.mu.RLock()
			trusted := m.trustedHashes[strings.ToLower(h.SHA256)]
			m.mu.RUnlock()
			if !trusted {
				return "", fmt.Errorf("hook command %s untrusted", h.Command)
			}
		}
	}

	timeout := 5 * time.Second
	if h.Timeout > 0 {
		timeout = time.Duration(h.Timeout) * time.Second
	}

	ctx, cancel := context.WithTimeout(context.Background(), timeout)
	defer cancel()

	cmd := exec.CommandContext(ctx, cmdPath, h.Args...)
	cmd.Dir = baseDir
	cmd.Stdin = strings.NewReader(string(payload))

	outBytes, err := cmd.CombinedOutput()
	output := string(outBytes)

	if err != nil {
		if ctx.Err() == context.DeadlineExceeded {
			return output, fmt.Errorf("hook timed out after %v", timeout)
		}
		return output, fmt.Errorf("hook failed with exit code: %w\nOutput: %s", err, output)
	}

	return output, nil
}
