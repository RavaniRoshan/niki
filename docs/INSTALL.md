# Installing NIKI

NIKI is distributed as a single release archive containing both the Rust engine (`niki`) and the terminal interface (`niki-shell`).

---

## Quick Install

### Linux & macOS

```bash
curl -fsSL https://raw.githubusercontent.com/RavaniRoshan/niki/master/scripts/install.sh | bash
```

To install a specific version:

```bash
NIKI_VERSION=0.10.0 curl -fsSL https://raw.githubusercontent.com/RavaniRoshan/niki/master/scripts/install.sh | bash
```

### macOS via Homebrew

```bash
brew install niki
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/RavaniRoshan/niki/master/scripts/install.ps1 | iex
```

---

## Stable Release URLs

Release archives are hosted directly on GitHub Releases:

- **Linux x86_64 (glibc)**: `https://github.com/RavaniRoshan/niki/releases/latest/download/niki-x86_64-unknown-linux-gnu.tar.gz`
- **Linux aarch64 (glibc)**: `https://github.com/RavaniRoshan/niki/releases/latest/download/niki-aarch64-unknown-linux-gnu.tar.gz`
- **macOS Apple Silicon**: `https://github.com/RavaniRoshan/niki/releases/latest/download/niki-aarch64-apple-darwin.tar.gz`
- **macOS Intel**: `https://github.com/RavaniRoshan/niki/releases/latest/download/niki-x86_64-apple-darwin.tar.gz`
- **Windows x64**: `https://github.com/RavaniRoshan/niki/releases/latest/download/niki-x86_64-pc-windows-msvc.zip`
- **Checksums**: `https://github.com/RavaniRoshan/niki/releases/latest/download/SHA256SUMS`

---

## Platform Support Matrix

| Platform | Target Triple | Support Status | Notes |
|---|---|---|---|
| Linux x86_64 | `x86_64-unknown-linux-gnu` | **Supported** | glibc >= 2.36 (Ubuntu 22.04+, Debian 12+, RHEL 9+) |
| Linux aarch64 | `aarch64-unknown-linux-gnu` | **Supported** | ARM64 Linux |
| macOS Apple Silicon | `aarch64-apple-darwin` | **Supported** | macOS 12+ (M1/M2/M3/M4) |
| macOS Intel | `x86_64-apple-darwin` | **Supported** | macOS 12+ x86_64 |
| Windows x64 | `x86_64-pc-windows-msvc` | **Supported** | Windows 10+, Windows 11 |

### Unsupported Combinations

- **Alpine Linux / musl**: The compiled interactive shell (`niki-shell`) is dynamically linked against glibc and fails to run on Alpine Linux (`missing dynamic library`). Alpine is **unsupported** for the prebuilt interactive package. In containerized benchmark environments where musl is required, use the headless engine (`niki run`) compiled for musl or build from source with cargo.
- **Windows on ARM (ARM64)**: Not currently provided as a pre-compiled binary.
- **32-bit platforms (`i686`, `armv7l`)**: Unsupported.

---

## Verifying the Installation

After installation, verify that the binaries are in your `PATH`:

```bash
niki --version
niki doctor
```

## Uninstalling

To uninstall NIKI:

```bash
curl -fsSL https://raw.githubusercontent.com/RavaniRoshan/niki/master/scripts/uninstall.sh | bash
```

To purge all task data and configuration as well:

```bash
curl -fsSL https://raw.githubusercontent.com/RavaniRoshan/niki/master/scripts/uninstall.sh | bash -s -- --purge
```
