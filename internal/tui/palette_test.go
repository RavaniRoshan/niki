package tui

import (
	"errors"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/secrets"
)

func TestPaletteCatalogAndFilter(t *testing.T) {
	items := DefaultPaletteCatalog()
	if len(items) == 0 {
		t.Fatal("expected non-empty palette catalog")
	}

	claudeMatches := FilterPalette(items, "claude")
	if len(claudeMatches) == 0 {
		t.Fatal("expected matches for 'claude'")
	}

	spinnerMatches := FilterPalette(items, "spinner")
	if len(spinnerMatches) < 4 {
		t.Fatalf("expected at least 4 spinner options, got %d", len(spinnerMatches))
	}
}

func TestCtrlPTogglesPalette(t *testing.T) {
	cmdChan := make(chan protocol.EngineCommand, 8)
	eventChan := make(chan protocol.EngineEvent, 8)
	m := NewAppModel(cmdChan, eventChan)

	if m.state.Palette.Open {
		t.Fatal("palette should start closed")
	}

	// Press Ctrl+P -> open
	m2, _ := m.Update(tea.KeyPressMsg{Code: 'p', Mod: tea.ModCtrl})
	m = m2.(AppModel)
	if !m.state.Palette.Open {
		t.Fatal("palette should be open after Ctrl+P")
	}

	// Press Esc -> close
	m2, _ = m.Update(tea.KeyPressMsg{Code: tea.KeyEsc})
	m = m2.(AppModel)
	if m.state.Palette.Open {
		t.Fatal("palette should be closed after Esc")
	}
}

func TestPaletteSelectSpinnerStyle(t *testing.T) {
	cmdChan := make(chan protocol.EngineCommand, 8)
	eventChan := make(chan protocol.EngineEvent, 8)
	m := NewAppModel(cmdChan, eventChan)

	// Open palette
	m2, _ := m.Update(tea.KeyPressMsg{Code: 'p', Mod: tea.ModCtrl})
	m = m2.(AppModel)

	// Filter to "braille"
	for _, r := range "braille" {
		m2, _ = m.Update(tea.KeyPressMsg{Text: string([]rune{r}), Code: []rune{r}[0]})
		m = m2.(AppModel)
	}

	// Press Enter to select
	m2, _ = m.Update(tea.KeyPressMsg{Code: tea.KeyEnter})
	m = m2.(AppModel)

	if m.state.Palette.Open {
		t.Fatal("palette should close after selection")
	}
	if m.state.SpinnerStyle != SpinnerBraille {
		t.Fatalf("expected SpinnerBraille, got %v", m.state.SpinnerStyle)
	}
}

// memBackend is an in-memory secrets.Backend for hermetic tests.
type memBackend struct{ store map[string]string }

func (b *memBackend) Set(service, account, key string) error {
	b.store[service+"/"+account] = key
	return nil
}

func (b *memBackend) Get(service, account string) (string, error) {
	if v, ok := b.store[service+"/"+account]; ok {
		return v, nil
	}
	return "", errors.New("secret not found")
}

func (b *memBackend) Delete(service, account string) error {
	delete(b.store, service+"/"+account)
	return nil
}

func TestPaletteConnectFlow(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	// The connect command writes the OS keychain; inject a memory
	// backend so the test never touches (or blocks on) the real one.
	secrets.UseBackendForTest(t, &memBackend{store: map[string]string{}})
	cmdChan := make(chan protocol.EngineCommand, 8)
	eventChan := make(chan protocol.EngineEvent, 8)
	m := NewAppModel(cmdChan, eventChan)

	// Slash command /connect openai
	m.composer.Input.SetValue("/connect openai")
	m2, _ := m.Update(tea.KeyPressMsg{Code: tea.KeyEnter})
	m = m2.(AppModel)

	if !m.state.Palette.Open || m.state.Palette.Mode != "connect" {
		t.Fatalf("expected open connect modal, got open=%v mode=%s", m.state.Palette.Open, m.state.Palette.Mode)
	}
	if m.state.Palette.ConnectProvider != "openai" {
		t.Fatalf("expected provider openai, got %s", m.state.Palette.ConnectProvider)
	}

	// Type dummy key
	for _, r := range "sk-test-key-123" {
		m2, _ = m.Update(tea.KeyPressMsg{Text: string([]rune{r}), Code: []rune{r}[0]})
		m = m2.(AppModel)
	}

	if m.state.Palette.ConnectKey != "sk-test-key-123" {
		t.Fatalf("unexpected connect key: %s", m.state.Palette.ConnectKey)
	}

	// Submit key: the keychain write and config save happen in the
	// returned command (never in Update), so the test drives it.
	m2, cmd := m.Update(tea.KeyPressMsg{Code: tea.KeyEnter})
	m = m2.(AppModel)

	if m.state.Palette.Open {
		t.Fatal("connect modal should close after submit")
	}
	if cmd == nil {
		t.Fatal("submit must dispatch the connect command")
	}
	m3, _ := m.Update(cmd())
	m = m3.(AppModel)
	lastCell := m.history.Cells[len(m.history.Cells)-1].Text
	if !strings.Contains(lastCell, "Connected openai") {
		t.Fatalf("expected connect notification in history, got: %s", lastCell)
	}
}

func TestPaletteCustomModelFlow(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	cmdChan := make(chan protocol.EngineCommand, 8)
	eventChan := make(chan protocol.EngineEvent, 8)
	m := NewAppModel(cmdChan, eventChan)

	// Open palette
	m2, _ := m.Update(tea.KeyPressMsg{Code: 'p', Mod: tea.ModCtrl})
	m = m2.(AppModel)

	// Filter to "custom model"
	for _, r := range "custom model" {
		m2, _ = m.Update(tea.KeyPressMsg{Text: string([]rune{r}), Code: []rune{r}[0]})
		m = m2.(AppModel)
	}

	// Select it
	m2, _ = m.Update(tea.KeyPressMsg{Code: tea.KeyEnter})
	m = m2.(AppModel)

	if !m.state.Palette.Open || m.state.Palette.Mode != "custom_model" {
		t.Fatalf("expected custom_model mode, got open=%v mode=%s", m.state.Palette.Open, m.state.Palette.Mode)
	}

	// Type model name
	for _, r := range "openai:deepseek-r1" {
		m2, _ = m.Update(tea.KeyPressMsg{Text: string([]rune{r}), Code: []rune{r}[0]})
		m = m2.(AppModel)
	}

	// Submit: the config write and the label update happen in the
	// returned commands (never in Update), so the test drives them.
	m2, cmd := m.Update(tea.KeyPressMsg{Code: tea.KeyEnter})
	m = m2.(AppModel)

	if m.state.Palette.Open {
		t.Fatal("palette should close after custom model submit")
	}
	if cmd == nil {
		t.Fatal("custom model submit must dispatch the config command")
	}
	msg := cmd()
	if batch, ok := msg.(tea.BatchMsg); ok {
		for _, c := range batch {
			m3, _ := m.Update(c())
			m = m3.(AppModel)
		}
	} else {
		m3, _ := m.Update(msg)
		m = m3.(AppModel)
	}
	if !strings.Contains(m.state.ModelName, "deepseek-r1") {
		t.Fatalf("expected model to contain deepseek-r1, got: %s", m.state.ModelName)
	}
	if len(m.history.Cells) == 0 || !strings.Contains(m.history.Cells[len(m.history.Cells)-1].Text, "Switched custom model") {
		t.Fatal("expected the custom model note in history")
	}
}

func TestPaletteMCPAddFlow(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	cmdChan := make(chan protocol.EngineCommand, 8)
	eventChan := make(chan protocol.EngineEvent, 8)
	m := NewAppModel(cmdChan, eventChan)

	// Open palette and switch to mcp_add
	m.state.Palette.Open = true
	m.state.Palette.Mode = "mcp_add"
	m.state.Palette.InputStep = 0
	m.state.Palette.InputBuffer = ""

	// Step 0: name
	for _, r := range "github" {
		m2, _ := m.Update(tea.KeyPressMsg{Text: string([]rune{r}), Code: []rune{r}[0]})
		m = m2.(AppModel)
	}
	m2, _ := m.Update(tea.KeyPressMsg{Code: tea.KeyEnter})
	m = m2.(AppModel)

	if m.state.Palette.InputStep != 1 || m.state.Palette.MCPName != "github" {
		t.Fatalf("expected step 1 and name github, got step=%d name=%s", m.state.Palette.InputStep, m.state.Palette.MCPName)
	}

	// Step 1: command
	for _, r := range "npx" {
		m2, _ = m.Update(tea.KeyPressMsg{Text: string([]rune{r}), Code: []rune{r}[0]})
		m = m2.(AppModel)
	}
	m2, _ = m.Update(tea.KeyPressMsg{Code: tea.KeyEnter})
	m = m2.(AppModel)

	if m.state.Palette.InputStep != 2 || m.state.Palette.MCPCmd != "npx" {
		t.Fatalf("expected step 2 and cmd npx, got step=%d cmd=%s", m.state.Palette.InputStep, m.state.Palette.MCPCmd)
	}

	// Step 2: args
	for _, r := range "-y @modelcontextprotocol/server-github" {
		m2, _ = m.Update(tea.KeyPressMsg{Text: string([]rune{r}), Code: []rune{r}[0]})
		m = m2.(AppModel)
	}
	m2, cmd := m.Update(tea.KeyPressMsg{Code: tea.KeyEnter})
	m = m2.(AppModel)

	if m.state.Palette.Open {
		t.Fatal("mcp_add modal should close after step 2")
	}
	if cmd == nil {
		t.Fatal("mcp_add step 2 must dispatch the config command")
	}
	m3, _ := m.Update(cmd())
	m = m3.(AppModel)

	lastCell := m.history.Cells[len(m.history.Cells)-1].Text
	if !strings.Contains(lastCell, "Added MCP server 'github'") {
		t.Fatalf("expected confirmation message, got: %s", lastCell)
	}
}

func TestPaletteEscAndBackspace(t *testing.T) {
	cmdChan := make(chan protocol.EngineCommand, 8)
	eventChan := make(chan protocol.EngineEvent, 8)
	m := NewAppModel(cmdChan, eventChan)

	m.state.Palette.Open = true
	m.state.Palette.Mode = "custom_endpoint"
	m.state.Palette.InputBuffer = "http://localhost"

	// Backspace
	m2, _ := m.Update(tea.KeyPressMsg{Code: tea.KeyBackspace})
	m = m2.(AppModel)
	if m.state.Palette.InputBuffer != "http://localhos" {
		t.Fatalf("expected backspace to trim char, got: %s", m.state.Palette.InputBuffer)
	}

	// Esc returns to palette mode
	m2, _ = m.Update(tea.KeyPressMsg{Code: tea.KeyEsc})
	m = m2.(AppModel)
	if !m.state.Palette.Open || m.state.Palette.Mode != "palette" {
		t.Fatalf("expected palette mode after Esc, got open=%v mode=%s", m.state.Palette.Open, m.state.Palette.Mode)
	}

	// Esc again closes palette
	m2, _ = m.Update(tea.KeyPressMsg{Code: tea.KeyEsc})
	m = m2.(AppModel)
	if m.state.Palette.Open {
		t.Fatal("expected palette to be closed after second Esc")
	}
}
