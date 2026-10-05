# NIKI — Release Channel Rollback & Yank Plan (R13)

Procedures for emergency rollback, unpublishing, or deprecation across all distribution channels.

---

## 1. Distribution Channels Overview

| Channel | Mechanism | Rollback SLA | Recovery Impact |
|---|---|---|---|
| **GitHub Releases** | `gh release delete` / edit draft | Immediate | Broken installer downloads |
| **Homebrew Tap** | Git revert in `homebrew-niki` | Immediate upon git push | Formula reverts to previous version |
| **Scoop Bucket** | Git revert in scoop repository | Immediate upon git push | Manifest reverts to previous version |
| **Windows Package Manager (winget)** | PR to `microsoft/winget-pkgs` | ~2-6 hours (review gate) | Users can pin or install prior version |
| **npm Registry** | `npm unpublish` (within 72h) or `npm deprecate` | Immediate | Installs fail back or warn |
| **Docker / GHCR** | Delete tag via GitHub Packages API | Immediate | `docker pull` falls back to prior tag |
| **crates.io** | `cargo yank --vers <version>` | Immediate | Dependency resolution skips yanked version |

---

## 2. Channel-by-Channel Rollback Procedures

### A. GitHub Releases
If an asset is corrupted, vulnerable, or broken:

1. **Mark Release as Draft or Delete**:
   ```bash
   # Option 1: Demote release to draft (preserves assets for investigation)
   gh release edit v<version> --draft

   # Option 2: Delete release immediately
   gh release delete v<version> --yes --cleanup-tag
   ```

2. **Delete Remote Git Tag**:
   ```bash
   git push --delete origin v<version>
   git tag -d v<version>
   ```

3. **Verify Installer Fallback**:
   The installer script falls back to `api.github.com/repos/RavaniRoshan/niki/releases/latest`, which will now point to the previous valid release tag.
   ```bash
   curl -fsSL https://raw.githubusercontent.com/RavaniRoshan/niki/master/scripts/install.sh | bash
   ```

---

### B. Homebrew Tap (`homebrew-niki`)
If a release formula points to a bad binary:

1. **Revert the Formula Commit**:
   ```bash
   cd /path/to/homebrew-niki
   git log -n 5 --oneline
   git revert HEAD -m "revert: rollback formula to v<previous-version>"
   git push origin main
   ```

2. **Verify Local Resolution**:
   ```bash
   brew update
   brew info RavaniRoshan/niki/niki
   ```

---

### C. Scoop Manifest (`scoop/niki.json`)
1. **Revert Manifest in Main Repo / Bucket**:
   ```bash
   git checkout master
   git revert <commit-hash-of-bump>
   git push origin master
   ```

2. **Verify Scoop Cache Flush**:
   ```powershell
   scoop update
   scoop info niki
   ```

---

### D. Windows Package Manager (winget)
Winget packages are immutable once merged into `microsoft/winget-pkgs`. To rollback:

1. **Submit Reversion or Deprecation PR**:
   Create a PR against `microsoft/winget-pkgs` reverting the `manifests/r/RavaniRoshan/niki/<version>` directory or updating to a patch fix `<version>.1`.
2. **Advise Users in Release Notes**:
   Direct Windows users to install explicit version:
   ```cmd
   winget install RavaniRoshan.niki --version <previous-version>
   ```

---

### E. npm Package (`@niki/cli` or `niki`)
1. **If Published < 72 Hours Ago (Zero Downstream Dependents)**:
   ```bash
   npm unpublish niki@<version> --force
   ```
2. **If Published > 72 Hours Ago**:
   ```bash
   npm deprecate niki@<version> "Critical regression; please use <previous-version>"
   npm dist-tag add niki@<previous-version> latest
   ```

---

### F. Docker / GHCR Image (`ghcr.io/ravaniroshan/niki-agent`)
1. **Retag `latest` to Previous Digest**:
   ```bash
   docker pull ghcr.io/ravaniroshan/niki-agent:<previous-version>
   docker tag ghcr.io/ravaniroshan/niki-agent:<previous-version> ghcr.io/ravaniroshan/niki-agent:latest
   docker push ghcr.io/ravaniroshan/niki-agent:latest
   ```
2. **Delete the Faulty Tag via GitHub UI or API**:
   Navigate to repository packages -> `niki-agent` -> tag settings -> delete tag `v<version>`.

---

### G. crates.io
If engine crate was published to crates.io:
```bash
cargo yank --vers <version> niki
```
To unyank after fixing an upstream issue:
```bash
cargo yank --undo --vers <version> niki
```

---

## 3. Communication & Post-Rollback Checklist
1. Update GitHub Releases page or pinned issue with status and remediation timeline.
2. Invalidate CDN cache for `scripts/install.sh` if any mirror is used.
3. Record root cause in `docs/ship/INCIDENTS.md` before attempting a new release tag.
