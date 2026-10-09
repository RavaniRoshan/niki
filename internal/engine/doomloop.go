package engine

import (
	"crypto/sha256"
	"encoding/hex"
	"sync"
)

// DefaultDoomLoopThreshold is the threshold of consecutive identical failed calls to halt.
const DefaultDoomLoopThreshold = 3

type toolCallEntry struct {
	hash    string
	success bool
}

// DoomLoopDetector detects pathological repetitive loops where the model repeatedly calls
// the same tool with identical arguments resulting in failures.
type DoomLoopDetector struct {
	threshold int
	history   []toolCallEntry
	mu        sync.Mutex
}

// NewDoomLoopDetector returns a detector with the specified threshold.
func NewDoomLoopDetector(threshold int) *DoomLoopDetector {
	if threshold <= 0 {
		threshold = DefaultDoomLoopThreshold
	}
	return &DoomLoopDetector{
		threshold: threshold,
		history:   make([]toolCallEntry, 0),
	}
}

func (d *DoomLoopDetector) hashCall(name string, argsJSON string) string {
	h := sha256.New()
	h.Write([]byte(name))
	h.Write([]byte(":"))
	h.Write([]byte(argsJSON))
	return hex.EncodeToString(h.Sum(nil))
}

// Record records a tool call attempt. Returns true if the doom loop circuit breaker tripped.
func (d *DoomLoopDetector) Record(name string, argsJSON string, success bool) bool {
	d.mu.Lock()
	defer d.mu.Unlock()

	h := d.hashCall(name, argsJSON)
	d.history = append(d.history, toolCallEntry{hash: h, success: success})

	if len(d.history) < d.threshold {
		return false
	}

	recent := d.history[len(d.history)-d.threshold:]
	firstHash := recent[0].hash
	for _, entry := range recent {
		if entry.hash != firstHash || entry.success {
			return false
		}
	}

	return true
}

// Reset clears the historical call tracker.
func (d *DoomLoopDetector) Reset() {
	d.mu.Lock()
	defer d.mu.Unlock()
	d.history = d.history[:0]
}
