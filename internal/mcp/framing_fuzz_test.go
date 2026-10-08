package mcp

import (
	"bytes"
	"io"
	"testing"
)

// FuzzJSONRPCFraming verifies that arbitrary byte streams never panic the framing reader or parser.
func FuzzJSONRPCFraming(f *testing.F) {
	// Seed corpus
	f.Add([]byte(`{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}` + "\n"))
	f.Add([]byte(`{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}` + "\n"))
	f.Add([]byte("Content-Length: 48\r\n\r\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}"))
	f.Add([]byte("Content-Length: 0\r\n\r\n"))
	f.Add([]byte("Content-Length: -1\r\n\r\n"))
	f.Add([]byte("Content-Length: invalid\r\n\r\n"))
	f.Add([]byte("\n\n\r\n"))
	f.Add([]byte("\x00\xff\xfe\x01\n"))

	f.Fuzz(func(t *testing.T, data []byte) {
		reader := NewFramingReader(bytes.NewReader(data))
		for {
			msg, err := reader.ReadMessage()
			if err != nil {
				if err == io.EOF {
					break
				}
				break
			}
			_, _, _ = ParseJSONRPCMessage(msg)
		}
	})
}
