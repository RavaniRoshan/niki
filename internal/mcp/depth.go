package mcp

import (
	"context"
	"encoding/json"
	"fmt"
	"time"
)

type Resource struct {
	URI         string `json:"uri"`
	Name        string `json:"name"`
	Description string `json:"description,omitempty"`
	MimeType    string `json:"mimeType,omitempty"`
}

type Prompt struct {
	Name        string `json:"name"`
	Description string `json:"description,omitempty"`
	Arguments   []struct {
		Name        string `json:"name"`
		Description string `json:"description,omitempty"`
		Required    bool   `json:"required,omitempty"`
	} `json:"arguments,omitempty"`
}

// ListResources fetches available server resources.
func (c *Client) ListResources(ctx context.Context) ([]Resource, error) {
	raw, err := c.Call(ctx, "resources/list", map[string]any{})
	if err != nil {
		return nil, err
	}
	var payload struct {
		Resources []Resource `json:"resources"`
	}
	if err := json.Unmarshal(raw, &payload); err != nil {
		return nil, err
	}
	return payload.Resources, nil
}

// ReadResource retrieves content from a resource URI.
func (c *Client) ReadResource(ctx context.Context, uri string) (string, error) {
	raw, err := c.Call(ctx, "resources/read", map[string]any{"uri": uri})
	if err != nil {
		return "", err
	}
	var payload struct {
		Contents []struct {
			URI  string `json:"uri"`
			Text string `json:"text"`
		} `json:"contents"`
	}
	if err := json.Unmarshal(raw, &payload); err != nil {
		return "", err
	}
	if len(payload.Contents) == 0 {
		return "", fmt.Errorf("no content found for resource %s", uri)
	}
	return payload.Contents[0].Text, nil
}

// ListPrompts fetches server prompts.
func (c *Client) ListPrompts(ctx context.Context) ([]Prompt, error) {
	raw, err := c.Call(ctx, "prompts/list", map[string]any{})
	if err != nil {
		return nil, err
	}
	var payload struct {
		Prompts []Prompt `json:"prompts"`
	}
	if err := json.Unmarshal(raw, &payload); err != nil {
		return nil, err
	}
	return payload.Prompts, nil
}

// GetPrompt retrieves a specific prompt with arguments.
func (c *Client) GetPrompt(ctx context.Context, name string, args map[string]string) (string, error) {
	raw, err := c.Call(ctx, "prompts/get", map[string]any{
		"name":      name,
		"arguments": args,
	})
	if err != nil {
		return "", err
	}
	var payload struct {
		Description string `json:"description"`
		Messages    []struct {
			Role    string `json:"role"`
			Content struct {
				Type string `json:"type"`
				Text string `json:"text"`
			} `json:"content"`
		} `json:"messages"`
	}
	if err := json.Unmarshal(raw, &payload); err != nil {
		return "", err
	}
	if len(payload.Messages) == 0 {
		return payload.Description, nil
	}
	return payload.Messages[0].Content.Text, nil
}

// Reconnect attempts to reconnect to the server with exponential backoff.
func (c *Client) Reconnect(ctx context.Context, maxRetries int, initialDelay time.Duration) error {
	delay := initialDelay
	if delay <= 0 {
		delay = 100 * time.Millisecond
	}
	maxDelay := 5 * time.Second

	var lastErr error
	for attempt := 1; attempt <= maxRetries; attempt++ {
		_ = c.Stop()
		err := c.Start(ctx)
		if err == nil {
			return nil
		}
		lastErr = err

		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(delay):
		}

		delay *= 2
		if delay > maxDelay {
			delay = maxDelay
		}
	}
	return fmt.Errorf("failed to reconnect after %d attempts: %w", maxRetries, lastErr)
}
