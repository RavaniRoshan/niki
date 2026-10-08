package main

import (
	"os"
	"testing"
)

func TestBenchVersionSmoke(t *testing.T) {
	r, vals, err := benchVersion(ensureBinary(t), 3)
	if err != nil {
		t.Fatal(err)
	}
	if r.Stats.N != 3 || len(vals) != 3 || r.Stats.P50 <= 0 {
		t.Fatalf("row = %+v", r)
	}
}

func TestBenchFootprintSmoke(t *testing.T) {
	r, err := benchFootprint(os.Args[0])
	if err != nil {
		t.Fatal(err)
	}
	if r.Stats.P50 <= 0 {
		t.Fatalf("row = %+v", r)
	}
}

func TestBenchSkillsAndTurnSmoke(t *testing.T) {
	r, _, err := benchSkillsWarm()
	if err != nil {
		t.Fatal(err)
	}
	if r.Stats.N != 5 {
		t.Fatalf("row = %+v", r)
	}
	tr, _, err := benchTurnOverhead(2)
	if err != nil {
		t.Fatal(err)
	}
	if tr.Stats.N != 2 || tr.Stats.P50 <= 0 {
		t.Fatalf("turn row = %+v", tr)
	}
}
