package editor

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// PrepareEditorDraft writes the initial text to a temporary markdown file
// and returns the constructed *exec.Cmd to launch the editor along with the file path.
func PrepareEditorDraft(initialText string) (*exec.Cmd, string, error) {
	tmpDir := os.TempDir()
	draftFile := filepath.Join(tmpDir, "nikicode_prompt_draft.md")

	if err := os.WriteFile(draftFile, []byte(initialText), 0o600); err != nil {
		return nil, "", err
	}

	editor := detectEditor()
	parts := strings.Fields(editor)
	bin := parts[0]
	args := append(parts[1:], draftFile)

	cmd := exec.Command(bin, args...)
	return cmd, draftFile, nil
}

// ReadAndCleanupDraft reads the final content from the temporary draft file
// and deletes the file.
func ReadAndCleanupDraft(draftFile string) (string, error) {
	data, err := os.ReadFile(draftFile)
	_ = os.Remove(draftFile)
	if err != nil {
		return "", err
	}
	return string(data), nil
}

// detectEditor resolves the editor executable from environment variables,
// falling back to nano, vim, or vi.
func detectEditor() string {
	if visual := strings.TrimSpace(os.Getenv("VISUAL")); visual != "" {
		return visual
	}
	if editor := strings.TrimSpace(os.Getenv("EDITOR")); editor != "" {
		return editor
	}
	for _, fallback := range []string{"nano", "vim", "vi"} {
		if path, err := exec.LookPath(fallback); err == nil && path != "" {
			return fallback
		}
	}
	return "nano"
}
