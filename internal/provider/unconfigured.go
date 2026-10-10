package provider

import (
	"context"
	"errors"
)

// UnconfiguredProvider represents the absence of a configured AI model or API key.
// Instead of silently falling back to mock completions, it returns a diagnostic error
// guiding the user to connect a supported provider.
// An optional custom message carries the ordered first-run detection
// guidance (I2); the zero value keeps the default diagnostic.
type UnconfiguredProvider struct {
	message string
}

func NewUnconfiguredProvider() *UnconfiguredProvider {
	return &UnconfiguredProvider{}
}

// NewUnconfiguredProviderWithMessage returns the provider with a
// caller-supplied first-turn diagnostic (first-run detection output).
func NewUnconfiguredProviderWithMessage(msg string) *UnconfiguredProvider {
	return &UnconfiguredProvider{message: msg}
}

func (u *UnconfiguredProvider) Name() string {
	return "none"
}

func (u *UnconfiguredProvider) Stream(ctx context.Context, messages []Message) (<-chan Delta, <-chan error) {
	deltaCh := make(chan Delta)
	close(deltaCh)

	errCh := make(chan error, 1)
	if u.message != "" {
		errCh <- errors.New(u.message)
	} else {
		errCh <- errors.New("no AI model configured. Connect a provider with /connect or Ctrl+P (or set ANTHROPIC_API_KEY, OPENAI_API_KEY, OPENROUTER_API_KEY, DEEPSEEK_API_KEY). Local shell commands can be run with '! <command>'")
	}
	close(errCh)

	return deltaCh, errCh
}
