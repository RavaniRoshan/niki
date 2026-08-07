# NIKI Distribution & CI/CD Plan

## Goal

Make NIKI (`niki` CLI) easy to install on Linux, Windows, and macOS with a reliable
automated pipeline that tests, builds, and releases on every push — never shipping
a broken binary.

---

## Research Summary (2026 Best Practices)

Based on analysis of popular Rust CLIs (ripgrep, bat, helix, zed) and distribution tools:

- **cargo-dist** is the de facto standard for Rust CLI release automation (2.1k stars, rapidly growing)
- **cargo-binstall** provides fast binary installation (2.8k stars, recommended in ripgrep docs)
- **GitHub Releases** is the central distribution hub — all package managers pull from there
- **Package managers > custom installers** for CLI tools
- **Snap/Flatpak/AppImage are NOT used** by any major Rust CLI — skip them entirely
- **Static musl binaries** for Linux ensure maximum compatibility
- **Code signing is optional** for CLI tools distributed via package managers
- **Self-update is unnecessary** — rely on package managers + cargo-binstall

---

## 1. Distribution Strategy (by platform)

### Recommended primary method: **cargo-dist** + **GitHub Releases**

[cargo-dist](https://github.com/axodotdev/cargo-dist) generates installers, archives, and install
scripts automatically. Used by Zed, helix, fnm, biome, and many others.

| Platform | Primary method | Secondary | Updater |
|----------|---------------|-----------|---------|
| Linux    | shell installer → `~/.cargo/bin/` | .deb, .rpm in Releases | cargo-binstall |
| Windows  | PowerShell installer → `%USERPROFILE%\.cargo\bin` | winget, Scoop, Chocolatey | package manager |
| macOS    | shell installer → `/usr/local/bin/` | Homebrew | brew upgrade |
| All      | `cargo binstall niki` | `cargo install niki` | cargo-binstall |

### Channel priority (implementation order)

1. **cargo-dist + GitHub Releases** — gives install scripts + archives for all platforms
2. **cargo-binstall** — auto-detected from GitHub Releases, zero config
3. **Homebrew** — mandatory for macOS, highest ROI
4. **winget** — built into Windows 10/11
5. **Scoop** — popular among Windows developers
6. **AUR (Arch Linux)** — usually community-contributed

### What to skip

| Method | Why skip |
|--------|----------|
| Snap | Confinement bugs, slow startup, ripgrep docs explicitly warn against it |
| Flatpak | Primarily for GUI apps, PATH issues for CLI |
| AppImage | GUI-focused, not suited for CLI tools in PATH |
| MSI/MSIX | Overkill for CLI tools, complex to maintain |
| DMG | Not standard for CLI tools |
| Self-update | Unnecessary — package managers handle this |

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
│  • x86_64-pc-windows-msvc    (windows-latest)                   │
│  • x86_64-apple-darwin       (macos-latest)                     │
│  • aarch64-apple-darwin      (macos-latest)                     │
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
cargo-dist plan → build matrix (4-6 targets) → upload artifacts
    │
    ▼
cargo-dist: generate installers + archives + checksums + release notes
    │
    ▼
GitHub Release created with all assets
    │
    ▼
cargo-binstall: auto-detects from Releases
Homebrew: formula updated (via cargo-dist publish)
winget: manifest updated (via cargo-dist publish)
```

---

## 3. cargo-dist Configuration

### Already configured in repo

- `Cargo.toml`: `repository` URL added
- `dist-workspace.toml`: targets for Linux (x86_64, aarch64), macOS (x86_64, aarch64), Windows (x86_64) with shell + PowerShell installers and updater enabled
- `.github/workflows/v-release.yml`: auto-generated CI workflow for building and releasing on tag push

### Targets

| Target | Runner | Installer |
|--------|--------|-----------|
| x86_64-unknown-linux-gnu | ubuntu-latest | shell script |
| aarch64-unknown-linux-gnu | ubuntu-latest | shell script |
| x86_64-apple-darwin | macos-latest | shell script |
| aarch64-apple-darwin | macos-latest | shell script |
| x86_64-pc-windows-msvc | windows-latest | PowerShell script |

---

## 4. Implementation Steps (ordered)

### Phase 1: Foundation (done)
- [x] Integration test harness (Dockerfile + mock LLM)
- [x] cargo-dist setup (config + generated workflow)
- [x] CI pipeline (check → test → build-matrix → e2e)

### Phase 2: First release (do now)
- [ ] Merge `goal/niki-10x-features-v2` into `master`
- [ ] Tag `v0.2.0` and push
- [ ] Verify cargo-dist creates GitHub Release with all binaries
- [ ] Verify `cargo binstall niki` works

### Phase 3: Package managers (week 2-3)
- [ ] Submit Homebrew formula to homebrew-core
- [ ] Submit winget manifest to microsoft/winget-pkgs
- [ ] Submit Scoop manifest to ScoopInstaller/Main

### Phase 4: Community distribution (ongoing)
- [ ] Document .deb/.rpm availability in GitHub Releases
- [ ] Wait for community to submit to AUR, Debian, Fedora
- [ ] Monitor https://repology.org for packaging status

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

| File | Status |
|------|--------|
| `Cargo.toml` | Done — repository URL added |
| `dist-workspace.toml` | Done — cargo-dist config |
| `.github/workflows/v-release.yml` | Done — auto-generated release workflow |
| `.github/workflows/ci.yml` | Done — enhanced 4-phase CI |
| `.github/workflows/integration.yml` | Done — E2E mock-LLM test |
| `tests/integration/Dockerfile` | Done — Ubuntu 24.04 + Rust + Node + Python |
| `tests/integration/mock_llm.py` | Done — SSE streaming mock |
| `tests/integration/niki.test.toml` | Done — config pointing at mock |
| Homebrew formula | To create (week 2) |
| winget manifest | To create (week 2) |
| Scoop manifest | To create (week 3) |

---

## 7. References

- <https://github.com/axodotdev/cargo-dist> — cargo-dist (distribution automation)
- <https://github.com/cargo-bins/cargo-binstall> — cargo-binstall (binary install)
- <https://github.com/sharkdp/bat> — example Rust CLI using cargo-dist
- <https://github.com/BurntSushi/ripgrep> — installation docs model
- <https://github.com/helix-editor/helix> — Rust CLI with broad package manager support
- <https://github.com/zed-industries/zed> — full cross-platform CI example
