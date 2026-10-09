package tools

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"regexp"
	"strings"
	"sync"
	"time"
)

const (
	maxFetchBodyBytes = 2 * 1024 * 1024 // 2MB read limit
	fetchHardCapChars = 8000            // Hard cap preventing page leaks into context
	cacheTTL          = 15 * time.Minute
)

type cacheEntry struct {
	content   string
	timestamp time.Time
}

type WebFetchTool struct {
	Base
	client *http.Client
	mu     sync.RWMutex
	cache  map[string]cacheEntry
}

func NewWebFetchTool() *WebFetchTool {
	return &WebFetchTool{
		Base: Base{
			SchemaStr: `{"required":["url"],"fields":{"url":"string"}}`,
		},
		client: &http.Client{
			Timeout: 15 * time.Second,
			CheckRedirect: func(req *http.Request, via []*http.Request) error {
				if len(via) >= 10 {
					return fmt.Errorf("stopped after 10 redirects")
				}
				return nil
			},
		},
		cache: make(map[string]cacheEntry),
	}
}

func (t *WebFetchTool) Name() string { return "web_fetch" }
func (t *WebFetchTool) Description() string {
	return "Fetch a web page, upgrade to HTTPS, and extract clean markdown content"
}

type webFetchArgs struct {
	URL string `json:"url"`
}

var (
	reScript   = regexp.MustCompile(`(?is)<script.*?</script>`)
	reStyle    = regexp.MustCompile(`(?is)<style.*?</style>`)
	reTag      = regexp.MustCompile(`(?s)<[^>]+>`)
	reSpaces   = regexp.MustCompile(`[ \t\r\f]+`)
	reNewlines = regexp.MustCompile(`\n{3,}`)
)

// htmlToMarkdown performs a lightweight, clean conversion from HTML to Markdown/text.
func htmlToMarkdown(html string) string {
	s := reScript.ReplaceAllString(html, "")
	s = reStyle.ReplaceAllString(s, "")

	// Common block tags to newlines
	s = regexp.MustCompile(`(?i)<h1[^>]*>(.*?)</h1>`).ReplaceAllString(s, "\n\n# $1\n\n")
	s = regexp.MustCompile(`(?i)<h2[^>]*>(.*?)</h2>`).ReplaceAllString(s, "\n\n## $1\n\n")
	s = regexp.MustCompile(`(?i)<h3[^>]*>(.*?)</h3>`).ReplaceAllString(s, "\n\n### $1\n\n")
	s = regexp.MustCompile(`(?i)<p[^>]*>`).ReplaceAllString(s, "\n\n")
	s = regexp.MustCompile(`(?i)</p>`).ReplaceAllString(s, "\n")
	s = regexp.MustCompile(`(?i)<br\s*/?>`).ReplaceAllString(s, "\n")
	s = regexp.MustCompile(`(?i)<li[^>]*>(.*?)</li>`).ReplaceAllString(s, "\n* $1")
	s = regexp.MustCompile(`(?i)<a\s+[^>]*href=["']([^"']+)["'][^>]*>(.*?)</a>`).ReplaceAllString(s, "[$2]($1)")
	s = regexp.MustCompile(`(?i)<pre><code>(.*?)</code></pre>`).ReplaceAllString(s, "\n\n```\n$1\n```\n\n")
	s = regexp.MustCompile(`(?i)<code[^>]*>(.*?)</code>`).ReplaceAllString(s, "`$1`")

	// Strip remaining HTML tags
	s = reTag.ReplaceAllString(s, "")

	// HTML unescaping basics
	s = strings.ReplaceAll(s, "&nbsp;", " ")
	s = strings.ReplaceAll(s, "&amp;", "&")
	s = strings.ReplaceAll(s, "&lt;", "<")
	s = strings.ReplaceAll(s, "&gt;", ">")
	s = strings.ReplaceAll(s, "&quot;", "\"")

	// Whitespace normalization
	s = reSpaces.ReplaceAllString(s, " ")
	s = reNewlines.ReplaceAllString(s, "\n\n")
	return strings.TrimSpace(s)
}

func (t *WebFetchTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a webFetchArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}

	targetURL := strings.TrimSpace(a.URL)
	if targetURL == "" {
		return ToolResult{Output: "url cannot be empty", IsError: true}, nil
	}

	// Upgrade HTTP to HTTPS for non-local addresses
	if strings.HasPrefix(strings.ToLower(targetURL), "http://") {
		if !strings.Contains(targetURL, "localhost") && !strings.Contains(targetURL, "127.0.0.1") && !strings.Contains(targetURL, "[::1]") {
			targetURL = "https://" + targetURL[7:]
		}
	} else if !strings.HasPrefix(strings.ToLower(targetURL), "https://") {
		targetURL = "https://" + targetURL
	}

	// Check 15m TTL cache
	t.mu.RLock()
	if entry, ok := t.cache[targetURL]; ok {
		if time.Since(entry.timestamp) < cacheTTL {
			t.mu.RUnlock()
			return ToolResult{Output: entry.content + "\n\n(served from 15m cache)"}, nil
		}
	}
	t.mu.RUnlock()

	parsedTarget, err := url.Parse(targetURL)
	if err != nil {
		return ToolResult{Output: fmt.Sprintf("invalid url: %v", err), IsError: true}, nil
	}

	req, err := http.NewRequestWithContext(ctx, "GET", targetURL, nil)
	if err != nil {
		return ToolResult{Output: fmt.Sprintf("request error: %v", err), IsError: true}, nil
	}
	req.Header.Set("User-Agent", "Mozilla/5.0 (compatible; NikiAgent/1.0; +https://github.com/RavaniRoshan/niki)")

	var redirectedFrom string
	var finalURL *url.URL

	resp, err := t.client.Do(req)
	if err != nil {
		return ToolResult{Output: fmt.Sprintf("fetch failed: %v", err), IsError: true}, nil
	}
	defer resp.Body.Close()

	finalURL = resp.Request.URL
	if finalURL != nil && parsedTarget.Host != "" && finalURL.Host != parsedTarget.Host {
		redirectedFrom = fmt.Sprintf("Redirected from %s to %s\n\n", parsedTarget.Host, finalURL.Host)
	}

	limitedReader := io.LimitReader(resp.Body, maxFetchBodyBytes)
	bodyBytes, err := io.ReadAll(limitedReader)
	if err != nil {
		return ToolResult{Output: fmt.Sprintf("read error: %v", err), IsError: true}, nil
	}

	contentType := resp.Header.Get("Content-Type")
	var extractedText string
	if strings.Contains(contentType, "text/html") || strings.Contains(string(bodyBytes[:min(512, len(bodyBytes))]), "<html") {
		extractedText = htmlToMarkdown(string(bodyBytes))
	} else {
		extractedText = string(bodyBytes)
	}

	// Apply hard cap preventing page leaks into context
	var output string
	if len(extractedText) > fetchHardCapChars {
		output = extractedText[:fetchHardCapChars] + fmt.Sprintf("\n\n... [content truncated at %d characters to preserve context limit]", fetchHardCapChars)
	} else {
		output = extractedText
	}

	if redirectedFrom != "" {
		output = redirectedFrom + output
	}

	// Save to cache
	t.mu.Lock()
	t.cache[targetURL] = cacheEntry{
		content:   output,
		timestamp: time.Now(),
	}
	t.mu.Unlock()

	return ToolResult{Output: output}, nil
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}

func (t *WebFetchTool) IsConcurrencySafe() bool { return true }
func (t *WebFetchTool) IsReadOnly() bool        { return true }
