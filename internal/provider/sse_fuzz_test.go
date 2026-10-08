package provider

import (
	"bytes"
	"io"
	"testing"
)

// FuzzParseSSE verifies that arbitrary byte streams never cause panic or infinite loop in SSEScanner.
func FuzzParseSSE(f *testing.F) {
	// Seed corpus
	f.Add([]byte("data: hello\n\n"))
	f.Add([]byte("event: delta\ndata: {\"content\": \"world\"}\n\n"))
	f.Add([]byte(":comment\ndata: multiline\ndata: text\n\n"))
	f.Add([]byte("event: error\ndata: 500\nid: 123\nretry: 1000\n\n"))
	f.Add([]byte("data: {\"choices\":[{\"delta\":{\"content\":\"test\"}}]}\ndata: [DONE]\n"))
	f.Add([]byte("data: incomplete without closing newline"))
	f.Add([]byte("\r\n\r\n\r\n"))
	f.Add([]byte("data: \x00\xff\xfe\x01\n\n"))

	f.Fuzz(func(t *testing.T, data []byte) {
		scanner := NewSSEScanner(bytes.NewReader(data))
		for {
			ev, err := scanner.Next()
			if err != nil {
				if err == io.EOF {
					break
				}
				// Any io/scanner error is acceptable, just no panics
				break
			}
			if ev == nil {
				t.Fatal("Next() returned nil event without error")
			}
		}
	})
}
