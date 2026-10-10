package installer

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// The I1 acceptance tests run install.sh in a clean environment: fresh HOME,
// restricted PATH with shims (uname, sudo-trap, curl-log), and a local
// file:// release fixture so no network is used. The owner runs the one real
// download from GitHub Releases (OWNER-VERIFY in docs/finish/CHECKLIST.md).

func repoRoot(t *testing.T) string {
	t.Helper()
	wd, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	// tools/installer -> repo root
	return filepath.Clean(filepath.Join(wd, "..", ".."))
}

func sha256File(t *testing.T, path string) string {
	t.Helper()
	b, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	sum := sha256.Sum256(b)
	return hex.EncodeToString(sum[:])
}

// buildFixture creates a goreleaser-shaped release directory:
// nikicode_<os>_<arch>.tar.gz (zip for windows) + checksums.txt.
func buildFixture(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	platforms := []string{"linux_amd64", "linux_arm64", "darwin_amd64", "darwin_arm64"}
	for _, p := range platforms {
		src := t.TempDir()
		bin := filepath.Join(src, "nikicode")
		content := "#!/bin/sh\necho nikicode 0.0.0-fixture\n"
		if err := os.WriteFile(bin, []byte(content), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(src, ".artifact"), []byte(p+"\n"), 0o644); err != nil {
			t.Fatal(err)
		}
		tarball := filepath.Join(dir, "nikicode_"+p+".tar.gz")
		out, err := exec.Command("tar", "-czf", tarball, "-C", src, "nikicode", ".artifact").CombinedOutput()
		if err != nil {
			t.Fatalf("tar %s: %v\n%s", p, err, out)
		}
	}
	// Windows zip for the PowerShell installer (static assertions).
	winSrc := t.TempDir()
	if err := os.WriteFile(filepath.Join(winSrc, "nikicode.exe"), []byte("MZ-fixture"), 0o644); err != nil {
		t.Fatal(err)
	}
	zipPath := filepath.Join(dir, "nikicode_windows_amd64.zip")
	if out, err := exec.Command("sh", "-c", fmt.Sprintf("cd %s && zip -q %s nikicode.exe", winSrc, zipPath)).CombinedOutput(); err != nil {
		t.Fatalf("zip: %v\n%s", err, out)
	}

	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	var sums strings.Builder
	for _, e := range entries {
		sum := sha256File(t, filepath.Join(dir, e.Name()))
		fmt.Fprintf(&sums, "%s  %s\n", sum, e.Name())
	}
	if err := os.WriteFile(filepath.Join(dir, "checksums.txt"), []byte(sums.String()), 0o644); err != nil {
		t.Fatal(err)
	}
	return dir
}

type installResult struct {
	stdout string
	err    error
}

type shimEnv struct {
	binDir     string
	home       string // stable HOME across runs (idempotency/uninstall)
	sudoCalled string // marker file path
	curlLog    string
	unameOS    string
	unameArch  string
	realCurl   string
}

func newShimEnv(t *testing.T) *shimEnv {
	t.Helper()
	binDir := t.TempDir()
	e := &shimEnv{
		binDir:     binDir,
		home:       t.TempDir(),
		sudoCalled: filepath.Join(binDir, "SUDO_WAS_CALLED"),
		curlLog:    filepath.Join(binDir, "curl.log"),
		unameOS:    "Linux",
		unameArch:  "x86_64",
	}
	// sudo trap: record any invocation and fail (proves "no sudo").
	sudo := "#!/bin/sh\necho CALLED >> " + e.sudoCalled + "\nexit 1\n"
	if err := os.WriteFile(filepath.Join(binDir, "sudo"), []byte(sudo), 0o755); err != nil {
		t.Fatal(err)
	}
	// uname shim controlled per-test.
	uname := "#!/bin/sh\ncase \"$1\" in\n-m) echo " + e.unameArch + " ;;\n*) echo " + e.unameOS + " ;;\nesac\n"
	if err := os.WriteFile(filepath.Join(binDir, "uname"), []byte(uname), 0o755); err != nil {
		t.Fatal(err)
	}
	// curl wrapper logs the URL then delegates to the real curl.
	if p, err := exec.LookPath("curl"); err == nil {
		e.realCurl = p
		curlsh := "#!/bin/sh\necho \"$@\" >> " + e.curlLog + "\nexec " + p + " \"$@\"\n"
		if err := os.WriteFile(filepath.Join(binDir, "curl"), []byte(curlsh), 0o755); err != nil {
			t.Fatal(err)
		}
	}
	return e
}

func (e *shimEnv) writeUname(t *testing.T) {
	t.Helper()
	uname := "#!/bin/sh\ncase \"$1\" in\n-m) echo " + e.unameArch + " ;;\n*) echo " + e.unameOS + " ;;\nesac\n"
	if err := os.WriteFile(filepath.Join(e.binDir, "uname"), []byte(uname), 0o755); err != nil {
		t.Fatal(err)
	}
}

func (e *shimEnv) run(t *testing.T, fixture string, args ...string) installResult {
	t.Helper()
	cmd := exec.Command("bash", append([]string{filepath.Join(repoRoot(t), "install.sh")}, args...)...)
	cmd.Env = []string{
		"HOME=" + e.home,
		"USER=installer-test",
		"PATH=" + e.binDir + ":/usr/bin:/bin",
		"NIKI_INSTALL_BASE_URL=file://" + fixture,
	}
	out, err := cmd.CombinedOutput()
	return installResult{stdout: string(out), err: err}
}

// installDir parses the machine-readable INSTALLED_DIR= line.
func (res installResult) installDir() string {
	for _, line := range strings.Split(res.stdout, "\n") {
		if strings.HasPrefix(line, "INSTALLED_DIR=") {
			return strings.TrimPrefix(line, "INSTALLED_DIR=")
		}
	}
	return ""
}

func (e *shimEnv) sudoWasCalled() bool {
	_, err := os.Stat(e.sudoCalled)
	return err == nil
}

func (e *shimEnv) curlTargets(t *testing.T) string {
	t.Helper()
	b, _ := os.ReadFile(e.curlLog)
	return string(b)
}

// TestI1InstallVerifiesChecksumAndInstalls: happy path on linux/amd64 —
// checksum verified, binary + compat symlinks installed, version runs,
// and no sudo is ever invoked.
func TestI1InstallVerifiesChecksumAndInstalls(t *testing.T) {
	fixture := buildFixture(t)
	env := newShimEnv(t)
	res := env.run(t, fixture)
	if res.err != nil {
		t.Fatalf("install failed: %v\n%s", res.err, res.stdout)
	}
	if env.sudoWasCalled() {
		t.Fatalf("installer invoked sudo:\n%s", res.stdout)
	}
	for _, want := range []string{"checksum", "OK"} {
		if !strings.Contains(strings.ToUpper(res.stdout), strings.ToUpper(want)) {
			t.Fatalf("output missing %q:\n%s", want, res.stdout)
		}
	}
	dir := res.installDir()
	if dir == "" {
		t.Fatalf("could not determine install dir:\n%s", res.stdout)
	}
	bin := filepath.Join(dir, "nikicode")
	if _, err := os.Stat(bin); err != nil {
		t.Fatalf("binary missing: %v\n%s", err, res.stdout)
	}
	for _, link := range []string{"nc", "niki"} {
		p := filepath.Join(dir, link)
		if _, err := os.Lstat(p); err != nil {
			t.Fatalf("symlink %s missing: %v", link, err)
		}
	}
	if !strings.Contains(res.stdout, "0.0.0-fixture") {
		t.Fatalf("installed binary version not printed:\n%s", res.stdout)
	}
	t.Logf("I1 happy path output:\n%s", res.stdout)
}

// TestI1TamperedDownloadRefused: a corrupted tarball (checksum mismatch)
// is refused before anything is installed.
func TestI1TamperedDownloadRefused(t *testing.T) {
	fixture := buildFixture(t)
	// Flip one byte inside the linux_amd64 tarball.
	target := filepath.Join(fixture, "nikicode_linux_amd64.tar.gz")
	b, err := os.ReadFile(target)
	if err != nil {
		t.Fatal(err)
	}
	b[len(b)/2] ^= 0xFF
	if err := os.WriteFile(target, b, 0o644); err != nil {
		t.Fatal(err)
	}

	env := newShimEnv(t)
	res := env.run(t, fixture)
	if res.err == nil {
		t.Fatalf("tampered download was accepted:\n%s", res.stdout)
	}
	if !strings.Contains(strings.ToLower(res.stdout), "checksum") {
		t.Fatalf("expected checksum failure message:\n%s", res.stdout)
	}
	// No binary may exist under HOME.
	if _, err := os.Lstat(filepath.Join(env.home, ".local", "bin", "nikicode")); err == nil {
		t.Fatal("binary installed despite checksum failure")
	}
	if _, err := os.Lstat(filepath.Join(env.home, ".local", "bin", "niki")); err == nil {
		t.Fatal("symlink installed despite checksum failure")
	}
	t.Logf("I1 tamper refusal output:\n%s", res.stdout)
}

// TestI1MissingChecksumRefused: install aborts when checksums.txt is absent.
func TestI1MissingChecksumRefused(t *testing.T) {
	fixture := buildFixture(t)
	if err := os.Remove(filepath.Join(fixture, "checksums.txt")); err != nil {
		t.Fatal(err)
	}
	env := newShimEnv(t)
	res := env.run(t, fixture)
	if res.err == nil {
		t.Fatalf("install proceeded without checksums:\n%s", res.stdout)
	}
	if !strings.Contains(strings.ToLower(res.stdout), "checksum") {
		t.Fatalf("expected missing-checksum refusal:\n%s", res.stdout)
	}
	t.Logf("I1 missing checksum refusal:\n%s", res.stdout)
}

// TestI1IdempotentAndUninstall: a second install succeeds cleanly, and
// --uninstall leaves no residue (before/after directory comparison).
func TestI1IdempotentAndUninstall(t *testing.T) {
	fixture := buildFixture(t)
	env := newShimEnv(t)

	first := env.run(t, fixture)
	if first.err != nil {
		t.Fatalf("first install: %v\n%s", first.err, first.stdout)
	}
	dir := first.installDir()
	if dir == "" {
		t.Fatalf("no install dir:\n%s", first.stdout)
	}

	second := env.run(t, fixture)
	if second.err != nil {
		t.Fatalf("second install not idempotent: %v\n%s", second.err, second.stdout)
	}
	if env.sudoWasCalled() {
		t.Fatal("sudo invoked during re-install")
	}

	before, _ := os.ReadDir(dir)
	if len(before) == 0 {
		t.Fatal("expected installed files before uninstall")
	}
	var beforeNames []string
	for _, e := range before {
		beforeNames = append(beforeNames, e.Name())
	}

	third := env.run(t, fixture, "--uninstall")
	if third.err != nil {
		t.Fatalf("uninstall failed: %v\n%s", third.err, third.stdout)
	}
	after, err := os.ReadDir(dir)
	var afterNames []string
	if err == nil {
		for _, e := range after {
			afterNames = append(afterNames, e.Name())
		}
	}
	leftover := []string{}
	for _, n := range beforeNames {
		for _, m := range afterNames {
			if n == m {
				leftover = append(leftover, n)
			}
		}
	}
	if len(leftover) > 0 {
		t.Fatalf("uninstall left residue: before=%v after=%v", beforeNames, afterNames)
	}
	t.Logf("I1 idempotent reinstall OK; uninstall before=%v after=%v", beforeNames, afterNames)
}

// TestI1DarwinArchMapping: uname shims for darwin/arm64 and darwin/x64 make
// the installer fetch the matching artifact from the release set.
func TestI1DarwinArchMapping(t *testing.T) {
	fixture := buildFixture(t)
	for _, tc := range []struct{ os, arch, want string }{
		{"Darwin", "arm64", "nikicode_darwin_arm64.tar.gz"},
		{"Darwin", "x86_64", "nikicode_darwin_amd64.tar.gz"},
	} {
		env := newShimEnv(t)
		env.unameOS = tc.os
		env.unameArch = tc.arch
		env.writeUname(t)
		res := env.run(t, fixture)
		if res.err != nil {
			t.Fatalf("%s/%s install failed: %v\n%s", tc.os, tc.arch, res.err, res.stdout)
		}
		logged := env.curlTargets(t)
		if !strings.Contains(logged, tc.want) {
			t.Fatalf("%s/%s: expected fetch of %s, curl log:\n%s", tc.os, tc.arch, tc.want, logged)
		}
		if env.sudoWasCalled() {
			t.Fatalf("%s/%s: sudo invoked", tc.os, tc.arch)
		}
		t.Logf("%s/%s -> fetched %s", tc.os, tc.arch, tc.want)
	}
}

// TestI1UnsupportedPlatformRejected: unknown OS/arch produce a clear error
// and no install.
func TestI1UnsupportedPlatformRejected(t *testing.T) {
	fixture := buildFixture(t)
	env := newShimEnv(t)
	env.unameArch = "mips"
	env.writeUname(t)
	res := env.run(t, fixture)
	if res.err == nil {
		t.Fatalf("unsupported arch accepted:\n%s", res.stdout)
	}
	if !strings.Contains(res.stdout, "Unsupported architecture") {
		t.Fatalf("expected clear error:\n%s", res.stdout)
	}
	t.Logf("I1 unsupported arch refusal: %s", strings.TrimSpace(res.stdout))
}

// TestI1WindowsPowerShellScript: install.ps1 exists and implements the same
// guarantees statically (checksum before execution, uninstall, no auto-elevate).
// The real Windows run is OWNER-VERIFY (no PowerShell in this environment).
func TestI1WindowsPowerShellScript(t *testing.T) {
	path := filepath.Join(repoRoot(t), "install.ps1")
	b, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("install.ps1 missing: %v", err)
	}
	script := string(b)
	for _, want := range []string{
		"Get-FileHash",  // checksum verification
		"checksums.txt", // published checksum manifest
		"-Uninstall",    // clean uninstall
		"$ErrorActionPreference = 'Stop'",
		"windows_amd64.zip", // artifact mapping
		"windows_arm64.zip",
	} {
		if !strings.Contains(script, want) {
			t.Fatalf("install.ps1 missing %q", want)
		}
	}
	// The archive must be hash-verified before it is expanded or used.
	hashIdx := strings.Index(script, "Get-FileHash")
	expandIdx := strings.Index(script, "Expand-Archive")
	if hashIdx < 0 || expandIdx < 0 || hashIdx > expandIdx {
		t.Fatal("install.ps1 must verify the hash before expanding the archive")
	}
	// No Start-Process elevation / implicit elevation.
	if strings.Contains(script, "Start-Process") && strings.Contains(script, "-Verb RunAs") {
		t.Fatal("install.ps1 must not auto-elevate")
	}
	t.Log("install.ps1 static guarantees verified (checksum before run, uninstall, no elevation)")
}
