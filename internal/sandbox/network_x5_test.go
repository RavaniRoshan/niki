package sandbox

import "testing"

// X5: network policy — an allowlist for command network access, off by
// default (deny all until explicitly allowed).
func TestNetworkPolicyAllowlistOffByDefault(t *testing.T) {
	// Default: off — nothing is allowed.
	def := DefaultNetworkPolicy()
	if def.AllowsHost("example.com") || def.AllowsURL("https://example.com") {
		t.Fatalf("[X5] default policy must deny all hosts")
	}
	// An empty (but enabled) allowlist also denies everything.
	empty := NetworkPolicy{}
	if empty.AllowsHost("example.com") {
		t.Fatalf("[X5] empty allowlist must deny all hosts")
	}
	t.Logf("[X5] off by default: empty/disabled policy denies all network access")

	// Configured allowlist: listed hosts allowed, others denied.
	p := NetworkPolicy{Allowlist: []string{"api.example.com", "*.internal.net"}}
	if !p.AllowsHost("api.example.com") {
		t.Fatalf("[X5] listed host must be allowed")
	}
	if !p.AllowsHost("api.example.com:443") {
		t.Fatalf("[X5] listed host with port must be allowed")
	}
	if p.AllowsHost("evil.example.com") {
		t.Fatalf("[X5] unlisted host must be denied")
	}
	if p.AllowsHost("example.com") {
		t.Fatalf("[X5] partial suffix match must be denied")
	}
	if !p.AllowsHost("db.internal.net") || !p.AllowsHost("cache.internal.net") {
		t.Fatalf("[X5] wildcard allowlist entries must match their suffix")
	}
	if p.AllowsHost("internal.net.evil.com") {
		t.Fatalf("[X5] wildcard must not match a crafted suffix")
	}
	t.Logf("[X5] allowlist: exact + wildcard entries allow listed hosts, deny all others")

	// URL form resolves the host before consulting the policy.
	if !p.AllowsURL("https://api.example.com/v1/thing") {
		t.Fatalf("[X5] allowed URL denied")
	}
	if p.AllowsURL("https://evil.example.com/exfil") {
		t.Fatalf("[X5] unlisted URL allowed")
	}
	// A garbage URL is never allowed.
	if p.AllowsURL("not a url") {
		t.Fatalf("[X5] malformed URL must be denied")
	}
	t.Logf("[X5] URL gate: allowed / denied / malformed-denied")
}
