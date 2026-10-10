// Package budget implements per-run execution budgets (L2): wall time,
// cost, and steps, each with a gauge. At 85% of any enabled budget the
// run enters WRAP; at 100% (or a single step crossing both thresholds)
// it stops. The state machine is sticky and only observed at safe
// points (step boundaries), so the current step always finishes.
package budget

import (
	"sync"
	"time"
)

// Kind names a budget dimension.
type Kind string

const (
	KindWallTime Kind = "wall_time"
	KindCost     Kind = "cost"
	KindSteps    Kind = "steps"
)

// State is the run's budget state.
type State string

const (
	StateOK      State = "ok"
	StateWrap    State = "wrap"
	StateStopped State = "stopped"
)

// WrapThreshold is the fraction of any enabled budget at which the run
// enters WRAP (85%).
const WrapThreshold = 0.85

// Limits are the per-run budget limits. A zero value disables that
// dimension.
type Limits struct {
	WallTime time.Duration
	CostUSD  float64
	Steps    int
}

// Enabled reports whether any dimension is active.
func (l Limits) Enabled() bool {
	return l.WallTime > 0 || l.CostUSD > 0 || l.Steps > 0
}

// Gauges is a snapshot of budget utilization. Each gauge is a fraction
// of the limit in [0, 1+); disabled dimensions report 0.
type Gauges struct {
	State State
	// Hit names the dimension that triggered the current non-ok state
	// ("" while ok).
	Hit Kind

	Wall, Cost, Step float64

	WallEnabled bool
	CostEnabled bool
	StepEnabled bool

	WallUsed  time.Duration
	CostUsed  float64
	StepsUsed int
}

// Tracker accumulates usage for one run and transitions state at
// observation points.
type Tracker struct {
	mu      sync.Mutex
	limits  Limits
	started time.Time
	cost    float64
	steps   int
	state   State
	hit     Kind
}

// New starts a tracker for the given limits. Zero limits are allowed:
// the tracker then reports ok forever (callers gate event emission on
// Limits.Enabled).
func New(l Limits) *Tracker {
	return &Tracker{limits: l, started: time.Now(), state: StateOK}
}

// AddStep records one completed provider step and its USD cost.
func (t *Tracker) AddStep(costUSD float64) {
	if t == nil {
		return
	}
	t.mu.Lock()
	defer t.mu.Unlock()
	t.steps++
	t.cost += costUSD
}

// Observe evaluates all gauges at a safe point and applies the
// transition rules: >=100% stops, >=85% enters WRAP; both are sticky.
func (t *Tracker) Observe() Gauges {
	t.mu.Lock()
	defer t.mu.Unlock()
	now := time.Now()

	var g Gauges
	g.State = t.state
	g.Hit = t.hit
	g.CostUsed = t.cost
	g.StepsUsed = t.steps
	g.WallUsed = now.Sub(t.started)
	g.WallEnabled = t.limits.WallTime > 0
	g.CostEnabled = t.limits.CostUSD > 0
	g.StepEnabled = t.limits.Steps > 0

	if g.WallEnabled {
		g.Wall = float64(g.WallUsed) / float64(t.limits.WallTime)
	}
	if g.CostEnabled {
		g.Cost = t.cost / t.limits.CostUSD
	}
	if g.StepEnabled {
		g.Step = float64(t.steps) / float64(t.limits.Steps)
	}

	// Transition: stopped beats wrap; evaluate every dimension so the
	// reported Hit is deterministic (wall, cost, steps order).
	if t.state != StateStopped {
		var over100, over85 Kind
		for _, c := range []struct {
			kind Kind
			frac float64
			on   bool
		}{
			{KindWallTime, g.Wall, g.WallEnabled},
			{KindCost, g.Cost, g.CostEnabled},
			{KindSteps, g.Step, g.StepEnabled},
		} {
			if !c.on {
				continue
			}
			if c.frac >= 1.0 && over100 == "" {
				over100 = c.kind
			}
			if c.frac >= WrapThreshold && over85 == "" {
				over85 = c.kind
			}
		}
		switch {
		case over100 != "":
			t.state = StateStopped
			t.hit = over100
		case over85 != "" && t.state != StateWrap:
			t.state = StateWrap
			t.hit = over85
		}
		g.State = t.state
		g.Hit = t.hit
	}
	return g
}

// State returns the current sticky state.
func (t *Tracker) State() State {
	if t == nil {
		return StateOK
	}
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.state
}

// ParseLimits builds Limits from raw config values. An invalid or empty
// wall duration disables the wall budget (error is returned so callers
// can log it); cost <= 0 and steps <= 0 disable those dimensions.
func ParseLimits(wall string, costUSD float64, steps int) (Limits, error) {
	var l Limits
	l.CostUSD = costUSD
	l.Steps = steps
	if wall != "" && wall != "0" {
		d, err := time.ParseDuration(wall)
		if err != nil {
			return Limits{}, err
		}
		l.WallTime = d
	}
	return l, nil
}
