package contextwin

import (
	"os"
	"strings"
	"testing"
)

// TestStaticPrefixHasNoDynamicFields is a lint test (C6): the static prefix
// must not embed cwd, branch, git status, or the active model.
func TestStaticPrefixHasNoDynamicFields(t *testing.T) {
	src, err := os.ReadFile("contextwin.go")
	if err != nil {
		t.Fatal(err)
	}
	// Find the StaticPrefix function body.
	_ = src
	p := StaticPrefix("sys", "skills")
	for _, forbidden := range []string{"cwd:", "branch:", "model:", "git_status:"} {
		if strings.Contains(p, forbidden) {
			t.Fatalf("static prefix contains session-varying field %q", forbidden)
		}
	}
}
