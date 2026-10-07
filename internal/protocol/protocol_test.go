package protocol

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestNewIDsAreUniqueAndOrdered(t *testing.T) {
	a, b := NewSessionId(), NewSessionId()
	if a == b {
		t.Fatal("session IDs must be unique")
	}
	if string(a) >= string(b) {
		t.Fatalf("UUIDv7 IDs should be lexicographically time-ordered: %s vs %s", a, b)
	}
}

func TestIDsDistinctTypes(t *testing.T) {
	s, tu, it, tc := NewSessionId(), NewTurnId(), NewItemId(), NewToolCallId()
	if s == SessionId(tu) || string(tu) == string(it) || string(it) == string(tc) {
		t.Fatal("typed IDs should be independent")
	}
}

func TestEngineEventJSON(t *testing.T) {
	evt := EngineEvent{Type: EventAssistantTextDelta, Text: "hi", TurnID: NewTurnId()}
	data, err := json.Marshal(evt)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(data), `"assistant_text_delta"`) {
		t.Fatalf("bad json: %s", data)
	}
	var back EngineEvent
	if err := json.Unmarshal(data, &back); err != nil {
		t.Fatal(err)
	}
	if back.Type != EventAssistantTextDelta || back.Text != "hi" {
		t.Fatalf("round trip mismatch: %+v", back)
	}
}

func TestEngineCommandJSON(t *testing.T) {
	cmd := EngineCommand{Type: CmdSubmitPrompt, Prompt: "hi"}
	data, _ := json.Marshal(cmd)
	var back EngineCommand
	if err := json.Unmarshal(data, &back); err != nil || back.Prompt != "hi" {
		t.Fatalf("round trip failed: %v %+v", err, back)
	}
}
