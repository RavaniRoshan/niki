# NIKI Distribution & CI/CD Plan

## Goal

Make NIKI (`niki` CLI) easy to install on Linux, Windows, and macOS with a reliable
automated pipeline that tests, builds, and releases on every push — never shipping
a broken binary.

---

## 1. Distribution Strategy (by platform)

### Recommended primary method: **cargo-dist** + **GitHub Releases**

[cargo-dist](https://github.com/axodotdev/cargo-dist) is the modern standard for Rust CLI
distribution. It generates installers, archives, and a shell/PowerShell install script.
Used by: Zed, helix, fnm, biome, and many others.

| Platform | Install method | Updater | Notes |
|----------|---------------|---------|-------|
| Linux    | shell installer → `~/.cargo/bin/` or `/usr/local/bin/` | built-in self-update | Also generates .deb, .rpm, AppImage |
| Windows  | PowerShell installer → `%USERPROFILE%\.cargo\bin` | built-in self-update | Also generates .msi |
| macOS    | shell installer → `/usr/local/bin/` | built-in self-update | Also generates .dmg |
| All      | `cargo install niki` | cargo | For developers |

### Secondary / fallback channels

| Channel | Platform | Reach | Effort |
|---------|----------|-------|--------|
| Homebrew | macOS + Linux | Very high (devs) | Low — one formula file |
| winget  | Windows | High | Low — auto-generated manifest |
| Scoop   | Windows | Medium | Low — one manifest JSON |
| cargo-binstall | All | Medium | Zero — auto-detected from GitHub Releases |

### Why cargo-dist over hand-rolled installers

1. **One config** (`dist-workspace.toml` or `Cargo.toml` `[workspace.metadata.dist]`) drives all platforms
2. **Generates install scripts** that auto-detect platform + architecture
3. **Native package formats** (.deb, .rpm, .msi, .dmg) from a single source
4. **SHA256 checksums** generated automatically
5. **npm installable** (optional — for JS-first devs)
6. **Integrates with GitHub Actions** via `taiki-e/install-action`

---

## 2. CI/CD Pipeline Design

### Workflow structure

```
┌─────────────────────────────────────────────────────────────────┐
│                        PUSH / PR                                │
└─────────────────┬───────────────────────────────────────────────┘
                  │
                  ▼
┌─────────────────────────────────────────────────────────────────┐
│  Phase 1: CHECK (fast fail)                                     │
│  • cargo fmt --check                                            │
│  • cargo clippy -- -D warnings                                  │
│  (~30s)                                                         │
└─────────────────┬───────────────────────────────────────────────┘
                  │ pass
                  ▼
┌─────────────────────────────────────────────────────────────────┐
│  Phase 2: TEST                                                  │
│  • cargo test --lib                                             │
│  • cargo test --test '*'                                        │
│  • cargo build --release (verify binary)                        │
│  (~2min)                                                        │
└─────────────────┬───────────────────────────────────────────────┘
                  │ pass
                  ▼
┌─────────────────────────────────────────────────────────────────┐
│  Phase 3: BUILD MATRIX (parallel)                               │
│  • x86_64-unknown-linux-gnu  (ubuntu-latest)                    │
│  • x86_64-apple-darwin       (macos-latest)                     │
│  • aarch64-apple-darwin      (macos-latest)                     │
│  • x86_64-pc-windows-msvc    (windows-latest)                   │
│  (~3min)                                                        │
└─────────────────┬───────────────────────────────────────────────┘
                  │ pass
                  ▼
┌─────────────────────────────────────────────────────────────────┐
│  Phase 4: E2E PIPELINE TEST (mock LLM)                          │
│  • Start mock LLM server on :8080                               │
│  • Run: niki run "task" --backend worktree --quiet              │
│  • Verify: branch created, report.md generated, artifacts OK    │
│  (~2min)                                                        │
└─────────────────┬───────────────────────────────────────────────┘
                  │ pass
                  ▼
            ✅ READY TO MERGE
```

### Release workflow (tag push)

```
v* tag pushed
    │
    ▼
Build matrix (4 targets) ──→ Upload artifacts
    │
    ▼
cargo-dist: generate installers + archives + checksums
    │
    ▼
GitHub Release created with all assets
    │
    ▼
Homebrew formula updated (via cargo-dist publish)
winget manifest updated (via cargo-dist publish)
```

---

## 3. cargo-dist Configuration

### `Cargo.toml` additions

```toml
[workspace.metadata.dist]
# Targets to build
targets = ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu",
           "x86_64-apple-darwin", "aarch64-apple-darwin",
           "x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"]
# Installers to generate
installers = ["shell", "powershell", "npm"]
# Native packages
casks = []
formula = ["niki"]
# .deb and .rpm for Linux
[[workspace.metadata.dist.apt]]
name = "niki"
[[workspace.metadata.dist.rpm]]
name = "niki"
```

---

## 4. Implementation Steps

### Step 1: Add cargo-dist (1 hour)
- Add `[workspace.metadata.dist]` to `Cargo.toml`
- Install cargo-dist: `cargo install cargo-dist`
- Generate config: `cargo dist init`
- Test locally: `cargo dist build`

### Step 2: Enhance CI workflow (30 min)
- Add windows-latest to build matrix
- Add E2E mock-LLM pipeline test job
- Configure fail-fast: false so all targets build even if one fails

### Step 3: Add Homebrew formula (30 min)
- Create `HomebrewFormula/niki.rb` (or let cargo-dist auto-generate)
- Tap: `RavaniRoshan/homebrew-niki`

### Step 4: Add winget manifest (30 min)
- Create `manifests/r/RavaniRoshan/niki/<version>/`
- Auto-published via cargo-dist or winget-create

### Step 5: Integration test hardening (1 hour)
- Fix the mock server to be more robust against streaming edge cases
- Add retry logic for the pipeline test
- Verify artifact generation for all agent roles

### Step 6: First real release (1 hour)
- Tag `v0.2.0`
- Push tag → CI builds all targets
- cargo-dist creates Release with all assets
- Verify install script works on all three platforms

---

## 5. Quality Gates (the "verify, test, build loop")

The pipeline is designed as a **verify → test → build** loop:

1. **Verify**: fmt + clippy (code quality)
2. **Test**: unit tests + mock-LLM e2e (functional correctness)
3. **Build**: multi-platform matrix (portability)

If any phase fails, the PR cannot merge. This ensures the `master` branch
is always green and release-ready.

For releases, the loop repeats on tag push with the additional step of
publishing to distribution channels.

---

## 6. File Changes Summary

| File | Action |
|------|--------|
| `Cargo.toml` | Add `[workspace.metadata.dist]` section |
| `.github/workflows/ci.yml` | Enhance with build matrix + e2e test |
| `.github/workflows/release.yml` | Replace with cargo-dist workflow |
| `tests/integration/Dockerfile` | Already created |
| `tests/integration/mock_llm.py` | Already created |
| `tests/integration/niki.test.toml` | Already created |
| `HomebrewFormula/niki.rb` | To create |
| `manifests/` | To create (winget) |

---

## 7. References

- <https://github.com/axodotdev/cargo-dist> — cargo-dist (distribution)
- <https://github.com/cargo-bins/cargo-binstall> — cargo-binstall (binary install)
- <https://github.com/sharkdp/bat> — example Rust CLI using cargo-dist
- <https://github.com/zed-industries/zed> — example Rust app with full cross-platform CI
