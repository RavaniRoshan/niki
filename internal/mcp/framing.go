package mcp

import (
	"bufio"
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"strconv"
	"strings"
)

// FramingReader reads frames from an io.Reader supporting both newline-delimited
// JSON and Content-Length header framing.
type FramingReader struct {
	reader *bufio.Reader
}

// NewFramingReader creates a new framing reader.
func NewFramingReader(r io.Reader) *FramingReader {
	return &FramingReader{reader: bufio.NewReader(r)}
}

// ReadMessage reads the next JSON-RPC payload frame.
func (fr *FramingReader) ReadMessage() ([]byte, error) {
	for {
		peek, err := fr.reader.Peek(1)
		if err != nil {
			return nil, err
		}
		if peek[0] == '\r' || peek[0] == '\n' {
			_, _ = fr.reader.ReadByte()
			continue
		}
		break
	}

	// Check if this frame begins with a Content-Length header
	headerPrefix, err := fr.reader.Peek(15)
	if err == nil && strings.HasPrefix(strings.ToLower(string(headerPrefix)), "content-length:") {
		contentLength := -1
		for {
			line, err := fr.reader.ReadString('\n')
			if err != nil {
				return nil, err
			}
			line = strings.TrimSpace(line)
			if line == "" {
				// End of headers
				break
			}
			parts := strings.SplitN(line, ":", 2)
			if len(parts) == 2 && strings.EqualFold(strings.TrimSpace(parts[0]), "content-length") {
				val, err := strconv.Atoi(strings.TrimSpace(parts[1]))
				if err != nil || val < 0 {
					return nil, fmt.Errorf("invalid Content-Length: %s", parts[1])
				}
				contentLength = val
			}
		}

		if contentLength < 0 {
			return nil, errors.New("missing Content-Length header")
		}
		if contentLength > 32*1024*1024 {
			return nil, fmt.Errorf("content-length exceeds max limit: %d", contentLength)
		}

		body := make([]byte, contentLength)
		if _, err := io.ReadFull(fr.reader, body); err != nil {
			return nil, err
		}
		return body, nil
	}

	// Fallback to newline-delimited JSON framing
	line, err := fr.reader.ReadBytes('\n')
	if err != nil && len(line) == 0 {
		return nil, err
	}
	return bytes.TrimSpace(line), nil
}

// ParseJSONRPCMessage attempts to parse a raw payload into a Request or Response.
func ParseJSONRPCMessage(raw []byte) (*Request, *Response, error) {
	raw = bytes.TrimSpace(raw)
	if len(raw) == 0 {
		return nil, nil, errors.New("empty message payload")
	}

	var peek struct {
		JSONRPC string          `json:"jsonrpc"`
		ID      *int64          `json:"id"`
		Method  string          `json:"method"`
		Result  json.RawMessage `json:"result,omitempty"`
		Error   any             `json:"error,omitempty"`
	}

	if err := json.Unmarshal(raw, &peek); err != nil {
		return nil, nil, err
	}

	if peek.Method != "" {
		var req Request
		if err := json.Unmarshal(raw, &req); err != nil {
			return nil, nil, err
		}
		return &req, nil, nil
	}

	var resp Response
	if err := json.Unmarshal(raw, &resp); err != nil {
		return nil, nil, err
	}
	return nil, &resp, nil
}
