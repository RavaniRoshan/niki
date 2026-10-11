package sandbox

import (
	"net"
	"net/url"
	"strings"
)

// Network policy (X5): command network access is gated by an
// allowlist that is OFF by default — when no allowlist is configured,
// no command is granted network access by the policy (the sandbox's
// own network namespace remains the enforcement layer). When an
// allowlist IS configured, a command may only reach listed hosts.

// NetworkPolicy is the command network-access policy (X5).
type NetworkPolicy struct {
	// Allowlist holds permitted hosts (exact host or "*.suffix").
	// Empty means the policy allows nothing (off by default).
	Allowlist []string
	// Disabled forces the deny-all default regardless of Allowlist.
	Disabled bool
}

// DefaultNetworkPolicy returns the default policy: off (deny all).
func DefaultNetworkPolicy() NetworkPolicy {
	return NetworkPolicy{Disabled: true}
}

// AllowsHost reports whether the policy permits a command to reach
// host (X5). An off/empty allowlist denies everything — commands get
// network access only by explicit opt-in.
func (p NetworkPolicy) AllowsHost(host string) bool {
	if p.Disabled || len(p.Allowlist) == 0 {
		return false
	}
	host = strings.ToLower(strings.TrimSpace(host))
	if h, _, err := net.SplitHostPort(host); err == nil {
		host = h
	}
	host = strings.Trim(host, "[]")
	for _, pattern := range p.Allowlist {
		pattern = strings.ToLower(strings.TrimSpace(pattern))
		if pattern == host {
			return true
		}
		// Wildcard suffix: "*.example.com" matches sub.example.com.
		if strings.HasPrefix(pattern, "*.") {
			suffix := strings.TrimPrefix(pattern, "*")
			if strings.HasSuffix(host, suffix) && host != strings.TrimPrefix(suffix, ".") {
				return true
			}
		}
	}
	return false
}

// AllowsURL reports whether the policy permits the URL's host.
func (p NetworkPolicy) AllowsURL(raw string) bool {
	u, err := url.Parse(raw)
	if err != nil || u.Host == "" {
		return false
	}
	return p.AllowsHost(u.Host)
}
