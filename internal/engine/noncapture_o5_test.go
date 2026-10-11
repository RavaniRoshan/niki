package engine

import (
	"fmt"
	"net"
	"os"
	"runtime"
	"strings"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
)

// externalConns counts established TCP connections whose remote end
// is not loopback (Linux /proc parsing). It is the network-capture
// half of O5: NIKI emits no telemetry of any kind.
func externalConns() int {
	n := 0
	for _, f := range []string{"/proc/net/tcp", "/proc/net/tcp6"} {
		data, err := os.ReadFile(f)
		if err != nil {
			continue
		}
		for _, line := range strings.Split(string(data), "\n")[1:] {
			f := strings.Fields(line)
			if len(f) < 4 || f[3] != "01" { // 01 = ESTABLISHED
				continue
			}
			ip := parseProcIP(strings.Split(f[2], ":")[0])
			if ip == nil || ip.IsLoopback() || ip.IsUnspecified() {
				continue
			}
			n++
		}
	}
	return n
}

func parseProcIP(hexIP string) net.IP {
	if len(hexIP) == 8 {
		var b [4]byte
		for i := 0; i < 4; i++ {
			var v uint
			_, _ = fmt.Sscanf(hexIP[2*i:2*i+2], "%02x", &v)
			b[3-i] = byte(v) // /proc is little-endian per word
		}
		return net.IPv4(b[0], b[1], b[2], b[3])
	}
	return nil
}

// O5 (network half): a full run against an in-process fixture
// provider opens zero outbound connections except (here: none at
// all — the fixture provider is in-process, so ANY external
// connection would be telemetry).
func TestFullRunOpensNoExternalConnections(t *testing.T) {
	if runtime.GOOS != "linux" {
		t.Skip("network capture uses /proc (linux only)")
	}
	if _, err := os.Stat("/proc/net/tcp"); err != nil {
		t.Skip("no /proc/net/tcp")
	}
	before := externalConns()
	p := &scriptProvider{steps: [][]provider.ToolCall{
		{{ID: "c1", Tool: "read_file", Args: `{"path":"noncapture_o5_test.go"}`}},
	}}
	eng, cmdChan, eventChan := newScriptEngine(t, p)
	_ = eng
	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "read"}
	drainUntil(t, eventChan, protocol.EventTurnCompleted, 15*time.Second)
	after := externalConns()
	if after > before {
		t.Fatalf("[O5] external connections grew %d -> %d during a full run", before, after)
	}
	t.Logf("[O5] full run: external connections %d -> %d (no telemetry)", before, after)
}
