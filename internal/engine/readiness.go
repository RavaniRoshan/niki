package engine

import (
	"sync"
	"time"
)

// Capability classes for the readiness contract (B7). Required
// capabilities must be warm before the first prompt is accepted;
// optional capabilities load lazily and never block readiness.
const (
	CapTerminal Capability = "terminal"
	CapEngine   Capability = "engine"
	CapModel    Capability = "model"
	CapSkills   Capability = "skills"
	CapMcp      Capability = "mcp"
)

type Capability string

// BootPhase records one capability's warm-up timing.
type BootPhase struct {
	Name     string
	Required bool
	Duration time.Duration
	At       time.Time
}

// Readiness tracks which capabilities are warm. It is safe for
// concurrent use: background warmers report in from their own
// goroutines while the engine loop reads the state.
type Readiness struct {
	mu       sync.RWMutex
	phases   []BootPhase
	warm     map[Capability]bool
	required map[Capability]bool
}

func NewReadiness(required ...Capability) *Readiness {
	r := &Readiness{
		warm:     map[Capability]bool{},
		required: map[Capability]bool{},
	}
	for _, c := range required {
		r.required[c] = true
	}
	return r
}

// Warm marks a capability ready and records its phase timing.
func (r *Readiness) Warm(cap Capability, required bool, d time.Duration) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.warm[cap] = true
	r.phases = append(r.phases, BootPhase{Name: string(cap), Required: required, Duration: d, At: time.Now()})
}

// IsWarm reports whether a capability has finished warming.
func (r *Readiness) IsWarm(cap Capability) bool {
	r.mu.RLock()
	defer r.mu.RUnlock()
	return r.warm[cap]
}

// Ready reports whether every required capability is warm.
func (r *Readiness) Ready() bool {
	r.mu.RLock()
	defer r.mu.RUnlock()
	for cap := range r.required {
		if !r.warm[cap] {
			return false
		}
	}
	return true
}

// Phases returns a copy of the recorded boot phases.
func (r *Readiness) Phases() []BootPhase {
	r.mu.RLock()
	defer r.mu.RUnlock()
	out := make([]BootPhase, len(r.phases))
	copy(out, r.phases)
	return out
}

// RequiredMissing lists required capabilities that are not yet warm.
func (r *Readiness) RequiredMissing() []Capability {
	r.mu.RLock()
	defer r.mu.RUnlock()
	var out []Capability
	for cap := range r.required {
		if !r.warm[cap] {
			out = append(out, cap)
		}
	}
	return out
}
