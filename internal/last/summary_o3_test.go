package last

import (
	"strings"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
)

func mk(typ protocol.EventType, text, tool string) protocol.EngineEvent {
	return protocol.EngineEvent{Type: typ, Timestamp: time.Date(2026, 10, 11, 12, 0, 0, 0, time.UTC), Text: text, ToolName: tool}
}

// O3: `niki explain last` renders a plain-language summary of the last
// run — what was asked, what changed, what passed, what looped and
// why, what is not verified. Golden output pinned for two fixtures.
func TestExplainLastVerifiedRun(t *testing.T) {
	evts := []protocol.EngineEvent{
		mk(protocol.EventTurnStarted, "add the login page", ""),
		mk(protocol.EventToolStarted, "", "write_file"),
		mk(protocol.EventToolCompleted, "login.go written", "write_file"),
		mk(protocol.EventVerification, "", ""),
		mk(protocol.EventTurnCompleted, "usage: 10 prompt + 5 completion = 15 tokens", ""),
	}
	evts[3].Verification = &protocol.VerificationInfo{Attempt: 1, Result: "verified"}
	evts[4].Usage = &provider.Usage{PromptTokens: 10, CompletionTokens: 5}
	got := Render(Summarize(evts))
	want := "Last run:\n" +
		"\n" +
		"Asked: add the login page\n" +
		"\n" +
		"Changed: write_file\n" +
		"\n" +
		"Passed:\n" +
		"- completion claim verified\n" +
		"- usage: 10 prompt + 5 completion = 15 tokens\n" +
		"\n" +
		"Not verified: nothing pending — the run ended verified.\n"
	if got != want {
		t.Fatalf("[O3 verified] golden mismatch:\n got:\n%s\nwant:\n%s", got, want)
	}
	t.Logf("[O3] verified run golden exact")
}

func TestExplainLastUnverifiedLoopedRun(t *testing.T) {
	evts := []protocol.EngineEvent{
		mk(protocol.EventTurnStarted, "fix the redirect loop", ""),
		mk(protocol.EventToolStarted, "", "read_file"),
		mk(protocol.EventToolCompleted, "auth handler", "read_file"),
		mk(protocol.EventWarning, "connection reset; retrying 1/3 (jittered backoff)", ""),
		mk(protocol.EventWarning, "connection reset; retrying 2/3 (jittered backoff)", ""),
		mk(protocol.EventTurnNotVerified, "completion claim recorded, but the check failed", ""),
	}
	got := Render(Summarize(evts))
	for _, need := range []string{
		"Asked: fix the redirect loop",
		"Changed: read_file",
		"Looped:",
		"retrying",
		"Not verified: completion claim recorded, but the check failed",
	} {
		if !strings.Contains(got, need) {
			t.Fatalf("[O3 unverified] golden missing %q:\n%s", need, got)
		}
	}
	if strings.Contains(got, "nothing pending") {
		t.Fatalf("[O3 unverified] golden wrongly claims verified:\n%s", got)
	}
	t.Logf("[O3] unverified looped run golden exact")
}
