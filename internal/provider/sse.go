package provider

import (
	"bufio"
	"bytes"
	"io"
	"strconv"
	"strings"
)

// SSEEvent represents a single Server-Sent Event as parsed from an event stream.
type SSEEvent struct {
	Event string
	Data  string
	ID    string
	Retry int
}

// SSEScanner parses an SSE stream according to the W3C EventSource specification,
// while accommodating real-world LLM streaming servers that may omit blank line delimiters.
type SSEScanner struct {
	scanner *bufio.Scanner
	pending *SSEEvent
}

// NewSSEScanner constructs an SSEScanner reading from r.
func NewSSEScanner(r io.Reader) *SSEScanner {
	sc := bufio.NewScanner(r)
	// Allow up to 1MB buffer for larger model chunks
	sc.Buffer(make([]byte, 64*1024), 1024*1024)
	return &SSEScanner{
		scanner: sc,
	}
}

// Next reads and returns the next dispatched SSEEvent.
// Returns io.EOF when the stream terminates cleanly without more events.
func (s *SSEScanner) Next() (*SSEEvent, error) {
	if s.pending != nil {
		ev := s.pending
		s.pending = nil
		return ev, nil
	}

	var ev SSEEvent
	var dataBuf bytes.Buffer
	hasData := false

	for s.scanner.Scan() {
		line := s.scanner.Text()

		// An empty line signals the dispatch of the accumulated event
		if line == "" || line == "\r" {
			if hasData || ev.Event != "" || ev.ID != "" {
				ev.Data = dataBuf.String()
				return &ev, nil
			}
			continue
		}

		// Comment lines are ignored
		if strings.HasPrefix(line, ":") {
			continue
		}

		field, value, _ := strings.Cut(line, ":")
		value = strings.TrimPrefix(value, " ")
		value = strings.TrimSuffix(value, "\r")

		switch field {
		case "event":
			if hasData {
				s.pending = &SSEEvent{Event: value}
				ev.Data = dataBuf.String()
				return &ev, nil
			}
			ev.Event = value
		case "data":
			if hasData {
				prev := strings.TrimSpace(dataBuf.String())
				if prev == "[DONE]" || (strings.HasPrefix(prev, "{") && strings.HasSuffix(prev, "}")) {
					s.pending = &SSEEvent{Data: value}
					ev.Data = prev
					return &ev, nil
				}
				dataBuf.WriteByte('\n')
			}
			dataBuf.WriteString(value)
			hasData = true
		case "id":
			ev.ID = value
		case "retry":
			if n, err := strconv.Atoi(value); err == nil && n >= 0 {
				ev.Retry = n
			}
		}
	}

	if err := s.scanner.Err(); err != nil {
		return nil, err
	}

	if hasData || ev.Event != "" || ev.ID != "" {
		ev.Data = dataBuf.String()
		return &ev, nil
	}

	return nil, io.EOF
}
