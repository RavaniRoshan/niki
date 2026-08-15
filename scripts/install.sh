#!/usr/bin/env bash
# NIKI installer — downloads the matching release archive, verifies its SHA256
# against the release checksums.txt, and installs the `niki` binary.
#
#   curl -fsSL https://raw.githubusercontent.com/RavaniRoshan/niki/master/scripts/install.sh | bash
#
set -euo pipefail

REPO="RavaniRoshan/niki"
API="https://api.github.com/repos/${REPO}/releases/latest"

err() { echo "error: $*" >&2; exit 1; }

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

# Only the three targets we actually ship are valid.
TARGET="${ARCH_PART}-${OS_PART}"
case "$TARGET" in
  x86_64-unknown-linux-gnu|x86_64-apple-darwin|aarch64-apple-darwin) ;;
  *) err "no prebuilt binary for $TARGET yet (Windows and linux/arm64 are planned)" ;;
esac

ASSET="niki-${TARGET}.tar.gz"

# --- pick checksum tool ------------------------------------------------------
if command -v sha256sum >/dev/null 2>&1; then SUM="sha256sum";
elif command -v shasum  >/dev/null 2>&1; then SUM="shasum -a 256";
else err "need sha256sum or shasum to verify the download"; fi

# --- resolve latest release --------------------------------------------------
echo "Resolving latest NIKI release..."
RELEASE="$(curl -fsSL "$API")" || err "could not reach GitHub releases API"
TAG="$(printf '%s' "$RELEASE" | grep -m1 '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')"
[ -n "$TAG" ] || err "could not determine latest release tag"
BASE="https://github.com/${REPO}/releases/download/${TAG}"

echo "Latest release: ${TAG}  (target ${TARGET})"

TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
cd "$TMP"

echo "Downloading ${ASSET} ..."
curl -fsSL -o "$ASSET" "${BASE}/${ASSET}" || err "download failed for $ASSET"
curl -fsSL -o checksums.txt "${BASE}/checksums.txt" || err "download failed for checksums.txt"

# --- verify checksum ---------------------------------------------------------
EXPECTED="$(grep -E "(\s|/)${ASSET}\$" checksums.txt | awk '{print $1}')"
[ -n "$EXPECTED" ] || err "checksums.txt has no entry for $ASSET"
ACTUAL="$($SUM "$ASSET" | awk '{print $1}')"
[ "$EXPECTED" = "$ACTUAL" ] || err "checksum mismatch for $ASSET (expected $EXPECTED, got $ACTUAL)"

# --- install -----------------------------------------------------------------
tar -xzf "$ASSET"
[ -x niki ] || err "extracted archive did not contain an executable 'niki'"

# Prefer ~/.local/bin, fall back to /usr/local/bin.
if [ -d "$HOME/.local/bin" ] || { [ ! -w /usr/local/bin ] && mkdir -p "$HOME/.local/bin" 2>/dev/null; }; then
  DEST="$HOME/.local/bin"
else
  DEST="/usr/local/bin"
fi
mkdir -p "$DEST"
install -m 0755 niki "$DEST/niki"

echo
echo "Installed niki to ${DEST}/niki"
"$DEST/niki" --version
case ":$PATH:" in
  *":$DEST:"*) ;;
  *) echo "NOTE: $DEST is not on your PATH — add it to your shell profile." ;;
esac
echo
echo "Next: set an API key (e.g. ANTHROPIC_API_KEY) and run:"
echo "  niki run \"Add a health endpoint to src/api.rs\""
