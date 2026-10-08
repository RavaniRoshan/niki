// Package paths is the single choke point for NikiCode home-directory
// locations (G1): the canonical ~/.nikicode with a one-time lossless
// migration from the legacy ~/.niki, plus dual-prefix environment
// lookup (NIKICODE_* canonical, NIKI_* compat alias).
package paths

import (
	"crypto/sha256"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"sync"
	"time"
)

// Canonical is the config-home directory name. Legacy is kept only as
// the migration source and the documented compat alias.
const (
	Canonical = ".nikicode"
	Legacy    = ".niki"
)

// Binary is the canonical binary name. Compat is the legacy alias
// installed as a symlink; it appears nowhere else.
const (
	Binary = "nikicode"
	Alias  = "nc"
	Compat = "niki"
)

// Report describes one Ensure (migration) run.
type Report struct {
	// Migrated is true when a legacy tree was copied.
	Migrated bool
	// Files and Bytes count the copied regular files.
	Files int
	Bytes int64
	// Skipped counts non-regular entries left behind (sockets, fifos…).
	Skipped int
	// From and To are the legacy and canonical directories.
	From, To string
}

var (
	mu   sync.Mutex
	done bool
	last Report
)

// Reset clears the once-guard. Tests only: production calls Ensure once.
func Reset() {
	mu.Lock()
	defer mu.Unlock()
	done = false
	last = Report{}
}

// Home returns the OS home directory, or "" when unknown.
func Home() string {
	if h := os.Getenv("HOME"); h != "" {
		return h
	}
	h, _ := os.UserHomeDir()
	return h
}

// Dir returns the canonical home directory (~/.nikicode). Pure: it
// performs no I/O; call Ensure for creation and migration.
func Dir() string {
	return filepath.Join(Home(), Canonical)
}

// LegacyDir returns the legacy home directory (~/.niki): migration
// source only, never written by new code.
func LegacyDir() string {
	return filepath.Join(Home(), Legacy)
}

// Ensure creates the canonical directory and runs the one-time lossless
// migration from the legacy directory. Copy, never move: the legacy
// tree is left untouched. Safe to call repeatedly; only the first call
// per process (or after Reset) does work.
func Ensure() Report {
	mu.Lock()
	defer mu.Unlock()
	if done {
		return last
	}
	done = true
	last = ensure()
	return last
}

func ensure() Report {
	to := Dir()
	rep := Report{From: LegacyDir(), To: to}
	if err := os.MkdirAll(to, 0o755); err != nil {
		return rep
	}
	from := LegacyDir()
	fromInfo, err := os.Stat(from)
	if err != nil || !fromInfo.IsDir() {
		return rep
	}
	if migratedMarkerPresent(to) {
		return rep
	}
	if !dirIsEmpty(to) {
		// A canonical tree already exists (created by another
		// version or by hand): never overlay it.
		return rep
	}
	rep.Migrated = true
	if err := copyTree(from, to, &rep); err != nil {
		return rep
	}
	rep.From, rep.To = from, to
	_ = os.WriteFile(filepath.Join(to, "MIGRATED_FROM"),
		[]byte(fmt.Sprintf("migrated from %s at %s: %d files, %d bytes, %d skipped\n",
			from, time.Now().UTC().Format(time.RFC3339), rep.Files, rep.Bytes, rep.Skipped)),
		0o644)
	return rep
}

func migratedMarkerPresent(to string) bool {
	_, err := os.Stat(filepath.Join(to, "MIGRATED_FROM"))
	return err == nil
}

func dirIsEmpty(dir string) bool {
	f, err := os.Open(dir)
	if err != nil {
		return true
	}
	defer f.Close()
	names, err := f.Readdirnames(3)
	return err != nil || len(names) == 0
}

// copyTree replicates regular files (content, mode), directories and
// symlinks from src to dst. Anything else is counted in Skipped.
func copyTree(src, dst string, rep *Report) error {
	return filepath.WalkDir(src, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(src, path)
		if err != nil {
			return err
		}
		if rel == "." {
			return nil
		}
		target := filepath.Join(dst, rel)
		info, err := d.Info()
		if err != nil {
			return err
		}
		switch {
		case d.Type().IsRegular():
			if err := copyFile(path, target, info.Mode()); err != nil {
				return err
			}
			rep.Files++
			rep.Bytes += info.Size()
		case d.IsDir():
			if err := os.MkdirAll(target, info.Mode().Perm()); err != nil {
				return err
			}
		case info.Mode()&os.ModeSymlink != 0:
			link, err := os.Readlink(path)
			if err != nil {
				return err
			}
			_ = os.Remove(target)
			if err := os.Symlink(link, target); err != nil {
				return err
			}
			rep.Files++
		default:
			rep.Skipped++
		}
		return nil
	})
}

func copyFile(src, dst string, mode os.FileMode) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()
	if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil {
		return err
	}
	out, err := os.OpenFile(dst, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, mode.Perm())
	if err != nil {
		return err
	}
	_, err = io.Copy(out, in)
	cerr := out.Close()
	if err != nil {
		return err
	}
	return cerr
}

// VerifyLossless walks both trees and reports the first mismatch
// (missing file, size, or content hash). Used by tests and doctor.
func VerifyLossless(from, to string) error {
	hash := func(path string) (int64, [32]byte, error) {
		f, err := os.Open(path)
		if err != nil {
			return 0, [32]byte{}, err
		}
		defer f.Close()
		h := sha256.New()
		n, err := io.Copy(h, f)
		if err != nil {
			return 0, [32]byte{}, err
		}
		var sum [32]byte
		copy(sum[:], h.Sum(nil))
		return n, sum, nil
	}
	return filepath.WalkDir(from, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(from, path)
		if err != nil {
			return err
		}
		if rel == "." {
			return nil
		}
		target := filepath.Join(to, rel)
		if d.IsDir() {
			st, err := os.Stat(target)
			if err != nil || !st.IsDir() {
				return fmt.Errorf("migration mismatch: dir %s missing", rel)
			}
			return nil
		}
		if !d.Type().IsRegular() {
			return nil
		}
		n1, h1, err := hash(path)
		if err != nil {
			return err
		}
		n2, h2, err := hash(target)
		if err != nil {
			return fmt.Errorf("migration mismatch: file %s missing: %w", rel, err)
		}
		if n1 != n2 || h1 != h2 {
			return fmt.Errorf("migration mismatch: file %s differs", rel)
		}
		return nil
	})
}

// Env returns the canonical NIKICODE_<name> value, falling back to the
// legacy NIKI_<name> compat alias. Empty when neither is set.
func Env(name string) string {
	v, _ := EnvSource(name)
	return v
}

// EnvSource also reports which variable fired: "NIKICODE_<name>",
// "NIKI_<name>", or "".
func EnvSource(name string) (value, source string) {
	if v := os.Getenv("NIKICODE_" + name); v != "" {
		return v, "NIKICODE_" + name
	}
	if v := os.Getenv("NIKI_" + name); v != "" {
		return v, "NIKI_" + name
	}
	return "", ""
}

// EnvIs reports whether name is set (either prefix) to one of vals.
func EnvIs(name string, vals ...string) bool {
	v := Env(name)
	for _, w := range vals {
		if v == w {
			return true
		}
	}
	return false
}
