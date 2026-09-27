#!/usr/bin/env bash
# NIKI installer — downloads the matching release archive, verifies its SHA256
# against the release sha256.sum, and installs the `niki` binary.
#
#   curl -fsSL https://raw.githubusercontent.com/RavaniRoshan/niki/master/scripts/install.sh | bash
#
# Options (pass after `bash` when running the piped form):
#   --version <tag>     install an explicit release tag instead of `latest`
#   --install-dir <dir> override the destination directory
#   --no-modify-path    accepted for compatibility; this script never edits your
#                       shell profile under any circumstances
set -euo pipefail

REPO="RavaniRoshan/niki"
API="https://api.github.com/repos/${REPO}/releases/latest"

err() { echo "error: $*" >&2; exit 1; }

# --- flags -------------------------------------------------------------------
WANT_TAG=""
INSTALL_DIR_OVERRIDE=""
while [ $# -gt 0 ]; do
  case "$1" in
    --version)       shift; [ $# -gt 0 ] || err "--version needs a tag"; WANT_TAG="$1" ;;
    --version=*)     WANT_TAG="${1#*=}" ;;
    --install-dir)   shift; [ $# -gt 0 ] || err "--install-dir needs a path"; INSTALL_DIR_OVERRIDE="$1" ;;
    --install-dir=*) INSTALL_DIR_OVERRIDE="${1#*=}" ;;
    --no-modify-path) ;;  # no-op: this script never touches your shell profile
    *) err "unknown option: $1" ;;
  esac
  shift
done

# --- detect platform ---------------------------------------------------------
OS="$(uname -s)"; ARCH="$(uname -m)"
case "$OS" in
  Linux)  OS_PART="unknown-linux-gnu" ;;
  Darwin) OS_PART="apple-darwin" ;;
  *) err "unsupported OS: $OS (supported: Linux, macOS)" ;;
esac

case "$ARCH" in
  x86_64|amd64) ARCH_PART="x86_64" ;;
  arm64|aarch64) ARCH_PART="aarch64" ;;
  *) err "unsupported architecture: $ARCH" ;;
esac

TARGET="${ARCH_PART}-${OS_PART}"
case "$TARGET" in
  x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu|x86_64-apple-darwin|aarch64-apple-darwin) ;;
  *) err "no prebuilt binary for $TARGET yet" ;;
esac

# cargo-dist publishes .tar.xz archives and a single sha256.sum manifest.
ASSET="niki-${TARGET}.tar.xz"
SUMS="sha256.sum"

# --- pick checksum tool ------------------------------------------------------
if command -v sha256sum >/dev/null 2>&1; then SUM="sha256sum";
elif command -v shasum  >/dev/null 2>&1; then SUM="shasum -a 256";
else err "need sha256sum or shasum to verify the download"; fi

# --- resolve release ---------------------------------------------------------
if [ -n "$WANT_TAG" ]; then
  TAG="$WANT_TAG"
else
  echo "Resolving latest NIKI release..."
  RELEASE="$(curl -fsSL "$API")" || err "could not reach GitHub releases API"
  TAG="$(printf '%s' "$RELEASE" | grep -m1 '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')"
  [ -n "$TAG" ] || err "could not determine latest release tag"
fi
BASE="https://github.com/${REPO}/releases/download/${TAG}"

echo "Release: ${TAG}  (target ${TARGET})"

TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
cd "$TMP"

# --- download ----------------------------------------------------------------
# GitHub resets connections and rate-limits unauthenticated fetches often
# enough that a single-attempt install is unreliable. Retry with backoff.
fetch() {  # fetch <url> <dest> <label>
  local url="$1" dest="$2" label="$3" attempt
  for attempt in 1 2 3 4; do
    if curl -fsSL --retry 0 -o "$dest" "$url"; then
      return 0
    fi
    echo "  attempt ${attempt}/4 failed for ${label}; retrying..." >&2
    sleep $((attempt * 2))
  done
  err "download failed for ${label} after 4 attempts: ${url}"
}

echo "Downloading ${ASSET} ..."
fetch "${BASE}/${ASSET}" "$ASSET" "$ASSET"
fetch "${BASE}/${SUMS}" "$SUMS" "$SUMS"

# --- verify checksum ---------------------------------------------------------
# cargo-dist writes BSD-style lines ("<hash> *<name>"); some mirrors emit the
# GNU form ("<hash>  <name>"). Accept any run of '*' and spaces between them.
# The dots in the asset name are escaped so they match literally.
ASSET_RE="$(printf '%s' "$ASSET" | sed 's/\./\\./g')"
EXPECTED="$(sed -nE "s/^([0-9a-f]{64})[*[:space:]]*${ASSET_RE}\$/\1/p" "$SUMS" | head -1)"
[ -n "$EXPECTED" ] || err "${SUMS} has no entry for ${ASSET}"
ACTUAL="$($SUM "$ASSET" | awk '{print $1}')"
[ "$EXPECTED" = "$ACTUAL" ] || err "checksum mismatch for ${ASSET} (expected ${EXPECTED}, got ${ACTUAL})"

# --- install -----------------------------------------------------------------
# cargo-dist archives carry a top-level directory; older/flat archives do not.
# Resolve the binary either way.
tar -xJf "$ASSET"
if [ -x ./niki ]; then
  BINARY="./niki"
else
  BINARY="$(find . -mindepth 2 -maxdepth 2 -type f -name niki -perm -u+x -print -quit)"
  [ -n "$BINARY" ] || err "extracted archive did not contain an executable 'niki'"
fi

# Install-dir priority ladder: NIKI_INSTALL_DIR -> XDG_BIN_DIR -> ~/.local/bin
# -> ~/.niki/bin. The first three are conventional; the last is the fallback
# that always works without touching system directories.
if [ -n "$INSTALL_DIR_OVERRIDE" ]; then
  DEST="$INSTALL_DIR_OVERRIDE"
elif [ -n "${NIKI_INSTALL_DIR:-}" ]; then
  DEST="$NIKI_INSTALL_DIR"
elif [ -n "${XDG_BIN_DIR:-}" ]; then
  DEST="$XDG_BIN_DIR"
elif [ -d "$HOME/.local/bin" ] || mkdir -p "$HOME/.local/bin" 2>/dev/null; then
  DEST="$HOME/.local/bin"
else
  DEST="$HOME/.niki/bin"
fi
mkdir -p "$DEST"
install -m 0755 "$BINARY" "$DEST/niki"

echo
echo "Installed niki to ${DEST}/niki"
"$DEST/niki" --version
case ":$PATH:" in
  *":$DEST:"*) ;;
  *) echo "NOTE: ${DEST} is not on your PATH. Add it yourself, e.g.:"; echo "      echo 'export PATH=\"${DEST}:\$PATH\"' >> ~/.bashrc" ;;
esac
echo
echo "Next:"
echo "  1. Create config + keys:  niki init  (or: niki init --scan to also draft AGENTS.md)"
echo "  2. Sandbox image (container backend): from a niki source checkout, run"
echo "       podman build -t niki-sandbox:24.04 -f docker/Dockerfile .   # or: docker build ..."
echo "     No container runtime? Use the worktree backend instead: niki run --backend worktree ..."
echo "  3. Verify everything:      niki doctor"
echo "  4. First task:             niki run \"Add a health endpoint to src/api.rs\""
