package provider

import (
	"fmt"
	"net/http"
	"time"
)

// OllamaBaseURL is Ollama's OpenAI-compatible endpoint on its default port.
const OllamaBaseURL = "http://localhost:11434/v1"

// OllamaStartHint is the exact "what to start" error text shown when no
// local model is available (I5): nothing listening, or the model not pulled.
func OllamaStartHint(model string) string {
	pull := "qwen2.5-coder:7b-instruct-q4_K_M"
	if model != "" {
		pull = model
	}
	return fmt.Sprintf("no local model available — start it with `ollama serve`, then pull the model: `ollama pull %s` (after that it runs fully offline)", pull)
}

// NewOllamaProvider returns an OpenAI-compatible client for Ollama's
// default local endpoint. No API key, loopback only (offline path).
func NewOllamaProvider(model string) *OpenAIProvider {
	return &OpenAIProvider{
		BaseURL:   OllamaBaseURL,
		APIKey:    "",
		Model:     model,
		Client:    &http.Client{Timeout: 120 * time.Second},
		StartHint: OllamaStartHint(model),
		name:      "ollama",
	}
}
