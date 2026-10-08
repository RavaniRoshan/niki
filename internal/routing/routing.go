package routing

import (
	"context"
	"errors"
	"fmt"
	"strings"

	"github.com/RavaniRoshan/niki/internal/provider"
)

// ModelProfile defines model preferences, provider, and fallback chains.
type ModelProfile struct {
	Name      string
	Provider  string
	Model     string
	Fallbacks []string
}

// IsNonFallbackError returns true if the error is terminal and must NOT trigger fallback.
// Specifically: auth errors, invalid credentials, and context cancellation.
func IsNonFallbackError(err error) bool {
	if err == nil {
		return false
	}
	if errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) {
		return true
	}
	msg := strings.ToLower(err.Error())
	nonFallbackSubstrings := []string{
		"401",
		"403",
		"unauthorized",
		"authentication",
		"forbidden",
		"invalid api key",
		"invalid_api_key",
		"bad credentials",
	}
	for _, s := range nonFallbackSubstrings {
		if strings.Contains(msg, s) {
			return true
		}
	}
	return false
}

// FallbackProvider wraps a primary provider and up to 3 fallback providers.
type FallbackProvider struct {
	primary   provider.ModelProvider
	fallbacks []provider.ModelProvider
}

// NewFallbackProvider creates a provider with up to 3 fallback providers.
func NewFallbackProvider(primary provider.ModelProvider, fallbacks ...provider.ModelProvider) *FallbackProvider {
	if len(fallbacks) > 3 {
		fallbacks = fallbacks[:3]
	}
	return &FallbackProvider{
		primary:   primary,
		fallbacks: fallbacks,
	}
}

func (f *FallbackProvider) Name() string {
	if f.primary != nil {
		return f.primary.Name()
	}
	return "fallback-router"
}

func (f *FallbackProvider) Stream(ctx context.Context, messages []provider.Message) (<-chan provider.Delta, <-chan error) {
	deltaCh := make(chan provider.Delta, 32)
	errCh := make(chan error, 1)

	go func() {
		defer close(deltaCh)
		defer close(errCh)

		providers := append([]provider.ModelProvider{f.primary}, f.fallbacks...)

		var lastErr error
		for i, p := range providers {
			if p == nil {
				continue
			}

			pDelta, pErr := p.Stream(ctx, messages)
			var hadDelta bool
			var streamErr error

			for pDelta != nil || pErr != nil {
				select {
				case <-ctx.Done():
					errCh <- ctx.Err()
					return
				case d, ok := <-pDelta:
					if !ok {
						pDelta = nil
						continue
					}
					hadDelta = true
					deltaCh <- d
				case err, ok := <-pErr:
					if !ok {
						pErr = nil
						continue
					}
					if err != nil {
						streamErr = err
					}
				}
			}

			if streamErr == nil {
				// Provider succeeded
				return
			}

			lastErr = streamErr

			// If deltas were already emitted, we cannot cleanly switch providers mid-stream
			if hadDelta {
				errCh <- streamErr
				return
			}

			// If it's a non-fallback error (e.g. auth error or context cancelled), fail fast immediately!
			if IsNonFallbackError(streamErr) {
				errCh <- fmt.Errorf("provider %s failed with non-fallback error: %w", p.Name(), streamErr)
				return
			}

			// Otherwise, if we have fallbacks left, log or proceed to the next fallback provider
			if i < len(providers)-1 {
				// Fallback to next provider
				continue
			}
		}

		if lastErr != nil {
			errCh <- fmt.Errorf("all %d providers in fallback chain failed, last error: %w", len(providers), lastErr)
		}
	}()

	return deltaCh, errCh
}
