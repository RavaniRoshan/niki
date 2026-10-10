package budget

import (
	"testing"
	"time"
)

// L2 unit contract: gauges for wall time, cost, and steps; 85% -> WRAP,
// 100% -> stopped; disabled dimensions never trigger.

func TestObserveOkBelow85(t *testing.T) {
	tr := New(Limits{WallTime: time.Minute, CostUSD: 1, Steps: 100})
	tr.AddStep(0.05) // 5%
	tr.started = time.Now().Add(-30 * time.Second)
	g := tr.Observe()
	if g.State != StateOK {
		t.Fatalf("below 85%% must stay ok, got %s", g.State)
	}
	if g.Cost < 0.049 || g.Cost > 0.051 {
		t.Errorf("cost gauge want ~0.05 got %f", g.Cost)
	}
	if g.Hit != "" {
		t.Errorf("no hit expected, got %s", g.Hit)
	}
}

func TestObserveWrapAt85(t *testing.T) {
	cases := []struct {
		name   string
		setup  func(tr *Tracker)
		verify func(g Gauges) bool
		want   Kind
	}{
		{"wall", func(tr *Tracker) { tr.started = time.Now().Add(-86 * time.Second) }, func(g Gauges) bool { return g.State == StateWrap }, KindWallTime},
		{"cost", func(tr *Tracker) { tr.AddStep(0.86) }, func(g Gauges) bool { return g.State == StateWrap }, KindCost},
		{"steps", func(tr *Tracker) {
			for i := 0; i < 86; i++ {
				tr.AddStep(0)
			}
		}, func(g Gauges) bool { return g.State == StateWrap }, KindSteps},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			tr := New(Limits{WallTime: 100 * time.Second, CostUSD: 1, Steps: 100})
			tc.setup(tr)
			g := tr.Observe()
			if !tc.verify(g) {
				t.Fatalf("want wrap, got state=%s hit=%s", g.State, g.Hit)
			}
			if g.Hit != tc.want {
				t.Errorf("hit kind want %s got %s", tc.want, g.Hit)
			}
			// Sticky: wrap survives later observes below threshold.
			if tr.Observe().State != StateWrap {
				t.Error("wrap must be sticky")
			}
		})
	}
}

func TestObserveStoppedAt100ViaJump(t *testing.T) {
	// A single step crosses 85 -> 100 in one observation.
	tr := New(Limits{CostUSD: 1, Steps: 10, WallTime: time.Minute})
	tr.AddStep(1.05)
	g := tr.Observe()
	if g.State != StateStopped {
		t.Fatalf(">=100%% must stop, got %s", g.State)
	}
	if g.Hit != KindCost {
		t.Errorf("hit want cost got %s", g.Hit)
	}
	if tr.Observe().State != StateStopped {
		t.Error("stopped must be sticky")
	}
}

func TestDisabledDimensionsNeverTrigger(t *testing.T) {
	tr := New(Limits{}) // all off
	tr.started = time.Now().Add(-10 * time.Hour)
	for i := 0; i < 1000; i++ {
		tr.AddStep(99)
	}
	g := tr.Observe()
	if g.State != StateOK {
		t.Fatalf("disabled budgets must stay ok, got %s", g.State)
	}
	if g.WallEnabled || g.CostEnabled || g.StepEnabled {
		t.Error("disabled budgets must report enabled=false")
	}
}

func TestGaugeSnapshotValues(t *testing.T) {
	tr := New(Limits{WallTime: 200 * time.Millisecond, CostUSD: 0.1, Steps: 4})
	tr.started = time.Now().Add(-100 * time.Millisecond) // 50%
	tr.AddStep(0.05)                                     // 50% cost
	tr.AddStep(0)                                        // 2 steps -> 50%
	g := tr.Observe()
	if g.State != StateOK {
		t.Fatalf("50%% everywhere must be ok, got %s", g.State)
	}
	if abs(g.Wall-0.5) > 0.05 {
		t.Errorf("wall gauge want ~0.5 got %f", g.Wall)
	}
	if abs(g.Cost-0.5) > 0.01 {
		t.Errorf("cost gauge want 0.5 got %f", g.Cost)
	}
	if abs(g.Step-0.5) > 0.01 {
		t.Errorf("step gauge want 0.5 got %f", g.Step)
	}
}

func abs(f float64) float64 {
	if f < 0 {
		return -f
	}
	return f
}
