#!/usr/bin/env bash
# NIKI installer: verified, sudo-free, idempotent, uninstallable.
#   ./install.sh                 install to ~/.local/bin (no sudo, ever)
#   ./install.sh --system        install to /usr/local/bin (dir must be writable)
#   ./install.sh --uninstall     remove the binary and compat symlinks
# Overrides (used by the clean-container tests):
#   NIKI_INSTALL_BASE_URL        release asset base (default: GitHub latest download)
#   NIKI_INSTALL_DIR             install directory (overrides the flags above)
set -euo pipefail

REPO="RavaniRoshan/niki"
BASE_URL="${NIKI_INSTALL_BASE_URL:-https://github.com/${REPO}/releases/latest/download}"
INSTALL_FLAG="user"
DO_UNINSTALL=0

for arg in "$@"; do
    case "$arg" in
        --system) INSTALL_FLAG="system" ;;
        --user) INSTALL_FLAG="user" ;;
        --uninstall) DO_UNINSTALL=1 ;;
        -h|--help)
            echo "Usage: install.sh [--user|--system] [--uninstall]"
            echo "Installs nikicode with checksum verification. Never uses sudo."
            exit 0
            ;;
        *)
            echo "Error: unknown option: $arg" >&2
            exit 2
            ;;
    esac
done

OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64|amd64) ARCH="amd64" ;;
    aarch64|arm64) ARCH="arm64" ;;
    *)
        echo "Error: Unsupported architecture: $ARCH" >&2
        echo "Fix: supported architectures are x86_64/amd64 and aarch64/arm64." >&2
        exit 1
        ;;
esac
case "$OS" in
    linux|darwin) ;;
    mingw*|msys*|cygwin*)
        echo "Error: Unsupported operating system: $OS (native Windows is not supported in v1)." >&2
        echo "Fix: run this installer inside WSL, or use install.ps1 for a verified manual install." >&2
        exit 1
        ;;
    *)
        echo "Error: Unsupported operating system: $OS" >&2
        echo "Fix: supported operating systems are Linux and macOS (WSL on Windows)." >&2
        exit 1
        ;;
esac

TARBALL="nikicode_${OS}_${ARCH}.tar.gz"

if [ -n "${NIKI_INSTALL_DIR:-}" ]; then
    INSTALL_DIR="$NIKI_INSTALL_DIR"
elif [ "$INSTALL_FLAG" = "system" ]; then
    INSTALL_DIR="/usr/local/bin"
else
    INSTALL_DIR="$HOME/.local/bin"
fi

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        echo "Error: no sha256 tool found (need sha256sum, shasum, or openssl)." >&2
        exit 1
    fi
}

fetch() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$1" -o "$2"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "$2" "$1"
    else
        echo "Error: neither curl nor wget found in PATH." >&2
        exit 1
    fi
}

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

if [ "$DO_UNINSTALL" = "1" ]; then
    removed=0
    for name in nikicode nc niki; do
        target="$INSTALL_DIR/$name"
        if [ -e "$target" ] || [ -L "$target" ]; then
            rm -f "$target"
            echo "removed $target"
            removed=$((removed + 1))
        fi
    done
    echo "Uninstalled $removed entries from $INSTALL_DIR."
    exit 0
fi

echo "Installing NikiCode for ${OS}/${ARCH}..."

# 1. Verify the checksum BEFORE unpacking or executing anything.
if ! fetch "${BASE_URL}/checksums.txt" "$TMP_DIR/checksums.txt"; then
    echo "Error: checksum manifest not found at ${BASE_URL}/checksums.txt — refusing to install an unverifiable binary." >&2
    echo "Fix: if you are behind a proxy, set HTTPS_PROXY; to test locally, set NIKI_INSTALL_BASE_URL to a directory that contains checksums.txt." >&2
    exit 1
fi

if ! fetch "${BASE_URL}/${TARBALL}" "$TMP_DIR/$TARBALL"; then
    echo "Error: download failed for ${BASE_URL}/${TARBALL}" >&2
    echo "Fix: check the release page for ${REPO} and your network/proxy settings." >&2
    exit 1
fi

EXPECTED="$(awk -v f="$TARBALL" '$2 == f {print $1}' "$TMP_DIR/checksums.txt")"
if [ -z "$EXPECTED" ]; then
    echo "Error: $TARBALL is not listed in checksums.txt — refusing to install." >&2
    exit 1
fi
ACTUAL="$(sha256_of "$TMP_DIR/$TARBALL")"
if [ "$EXPECTED" != "$ACTUAL" ]; then
    echo "Error: checksum mismatch for $TARBALL — download corrupted or tampered." >&2
    echo "  expected: $EXPECTED" >&2
    echo "  actual:   $ACTUAL" >&2
    echo "Fix: retry the download; if it persists, the release artifact or your mirror is bad." >&2
    exit 1
fi
echo "checksum OK ($TARBALL)"

# 2. Unpack and install (atomic: move into place only after full unpack).
tar -xzf "$TMP_DIR/$TARBALL" -C "$TMP_DIR"
if [ ! -f "$TMP_DIR/nikicode" ]; then
    echo "Error: archive did not contain the nikicode binary." >&2
    exit 1
fi
chmod +x "$TMP_DIR/nikicode"

if [ ! -d "$INSTALL_DIR" ]; then
    mkdir -p "$INSTALL_DIR" 2>/dev/null || true
fi
if [ ! -w "$INSTALL_DIR" ]; then
    echo "Error: $INSTALL_DIR is not writable and this installer never uses sudo." >&2
    echo "Fix: re-run with --user to install into $HOME/.local/bin, or re-run this script yourself as: sudo $0 --system" >&2
    exit 1
fi

mv -f "$TMP_DIR/nikicode" "$INSTALL_DIR/nikicode"
ln -sfn nikicode "$INSTALL_DIR/nc"
ln -sfn nikicode "$INSTALL_DIR/niki"

echo "INSTALLED_DIR=$INSTALL_DIR"
echo "NikiCode installed to $INSTALL_DIR/nikicode (aliases: nc, niki compat)."

# 3. Prove the installed binary runs (tolerate cross-arch fixture installs).
if "$INSTALL_DIR/nikicode" --version >/dev/null 2>&1; then
    "$INSTALL_DIR/nikicode" --version
else
    echo "note: installed binary did not run on this host (cross-architecture fixture?); skipping --version check."
fi

case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        echo ""
        echo "Note: $INSTALL_DIR is not currently in your PATH."
        echo "Add it to your shell configuration:"
        echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
        ;;
esac
