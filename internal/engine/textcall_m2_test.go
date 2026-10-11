package engine

import (
	"strings"
	"testing"

	"github.com/RavaniRoshan/niki/internal/tools"
)

// M2: text-format tool calls are parsed only when the tool name and
// schema match exactly. A near-miss name is not executed; each
// recovery is logged.
func TestTextToolCallNearMissNotExecuted(t *testing.T) {
	reg := tools.DefaultRegistry()

	// A tagged block with the EXACT tool name + valid args parses and
	// resolves to a real tool.
	exact := `I will read the file.
<read_file>{"path":"/etc/hostname"}</read_file>`
	call, note := RecoverTextCall(exact)
	if call.Tool != "read_file" || note == "" {
		t.Fatalf("[M2] exact tagged call not recovered: %+v note=%q", call, note)
	}
	if _, ok := reg.Get(call.Tool); !ok {
		t.Fatalf("[M2] exact tool %q not in registry", call.Tool)
	}
	// Args must validate against the tool schema before dispatch.
	if err := tools.ValidateArgs(call.Args, mustSchema(reg, call.Tool)); err != nil {
		t.Fatalf("[M2] recovered args failed schema: %v", err)
	}
	t.Logf("[M2] exact call recovered + resolves: %q (%s)", call.Tool, note)

	// A NEAR-MISS name parses as a candidate but is NOT in the
	// registry, so it is never executed.
	nearMiss := `<read_fil>{"path":"/etc/passwd"}</read_fil>`
	nm, nmNote := RecoverTextCall(nearMiss)
	if nm.Tool != "read_fil" {
		t.Fatalf("[M2] near-miss name not extracted for logging: %+v", nm)
	}
	if _, ok := reg.Get(nm.Tool); ok {
		t.Fatalf("[M2] near-miss %q unexpectedly resolved to a real tool", nm.Tool)
	}
	t.Logf("[M2] near-miss %q recovered for logging but refused (unknown tool) (%s)", nm.Tool, nmNote)

	// A fenced JSON block with an explicit tool field also parses, but
	// a bogus tool name is refused at the registry.
	fenced := "```json\n{\"tool\": \"delete_everything\", \"args\": {}}\n```"
	fc, fnote := RecoverTextCall(fenced)
	if fc.Tool != "delete_everything" {
		t.Fatalf("[M2] fenced tool not extracted: %+v", fc)
	}
	if _, ok := reg.Get(fc.Tool); ok {
		t.Fatalf("[M2] fenced bogus tool %q resolved", fc.Tool)
	}
	t.Logf("[M2] fenced bogus tool %q refused (%s)", fc.Tool, fnote)

	// A block whose args fail schema validation is not executed even
	// when the name is exact: recovery is candidate-only; the schema
	// gate rejects it.
	badArgs := `<read_file>{"path":123}</read_file>`
	bc, _ := RecoverTextCall(badArgs)
	if bc.Tool != "read_file" {
		t.Fatalf("[M2] exact-name/bad-args not extracted: %+v", bc)
	}
	if err := tools.ValidateArgs(bc.Args, mustSchema(reg, bc.Tool)); err == nil {
		t.Fatalf("[M2] malformed args should fail schema validation")
	}
	t.Logf("[M2] exact name + malformed args refused by schema gate")

	// Plain prose with no tool block recovers nothing.
	if c, n := RecoverTextCall("just some regular assistant prose, no tools here"); c.Tool != "" || n != "" {
		t.Fatalf("[M2] prose should recover nothing: %+v %q", c, n)
	}
	if !strings.Contains(note, "tagged") {
		t.Fatalf("[M2] recovery note should say how it was recovered")
	}
	t.Logf("[M2] plain prose recovers nothing")
}

func mustSchema(reg *tools.Registry, name string) string {
	t, ok := reg.Get(name)
	if !ok {
		return ""
	}
	return t.Schema()
}
