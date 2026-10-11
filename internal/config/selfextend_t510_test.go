package config

import (
	"os"
	"testing"
)

// T5.10 DEFAULT OFF: self_extend = "off" | "propose"; default off,
// unknown values fail closed to off.
func TestSelfExtendDefaultOff(t *testing.T) {
	if got := Default().SelfExtend; got != "off" {
		t.Fatalf("[T5.10] default self_extend = %q, want off", got)
	}
	dir := t.TempDir()
	origWd, _ := os.Getwd()
	_ = os.Chdir(dir)
	defer func() { _ = os.Chdir(origWd) }()

	// Explicit propose is honored.
	_ = os.WriteFile("nikicode.toml", []byte("self_extend = \"propose\"\n"), 0o644)
	cfg, err := Load("")
	if err != nil {
		t.Fatal(err)
	}
	if cfg.SelfExtend != "propose" {
		t.Fatalf("[T5.10] propose not honored: %q", cfg.SelfExtend)
	}
	// Garbage fails closed to off, never to propose.
	_ = os.WriteFile("nikicode.toml", []byte("self_extend = \"yes-please\"\n"), 0o644)
	cfg, err = Load("")
	if err != nil {
		t.Fatal(err)
	}
	if cfg.SelfExtend != "off" {
		t.Fatalf("[T5.10] garbage must fail closed to off, got %q", cfg.SelfExtend)
	}
	t.Logf("[T5.10] default off; propose honored; garbage fails closed to off")
}
