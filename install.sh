#!/usr/bin/env bash
set -e

REPO="RavaniRoshan/niki"
INSTALL_DIR="/usr/local/bin"

if [ ! -w "$INSTALL_DIR" ]; then
    INSTALL_DIR="$HOME/.local/bin"
    mkdir -p "$INSTALL_DIR"
fi

OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "$ARCH" in
    x86_64|amd64)
        ARCH="amd64"
        ;;
    aarch64|arm64)
        ARCH="arm64"
        ;;
    *)
        echo "Error: Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

case "$OS" in
    linux)
        OS="linux"
        ;;
    darwin)
        OS="darwin"
        ;;
    *)
        echo "Error: Unsupported operating system: $OS"
        exit 1
        ;;
esac

TARBALL="niki_${OS}_${ARCH}.tar.gz"
URL="https://github.com/${REPO}/releases/latest/download/${TARBALL}"

echo "⚡ Installing NIKI for ${OS}/${ARCH}..."

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

if command -v curl >/dev/null 2>&1; then
    curl -sSL "$URL" -o "$TMP_DIR/$TARBALL"
elif command -v wget >/dev/null 2>&1; then
    wget -qO "$TMP_DIR/$TARBALL" "$URL"
else
    echo "Error: Neither curl nor wget found in PATH."
    exit 1
fi

tar -xzf "$TMP_DIR/$TARBALL" -C "$TMP_DIR"

if [ ! -f "$TMP_DIR/niki" ]; then
    echo "Error: Failed to unpack niki binary."
    exit 1
fi

chmod +x "$TMP_DIR/niki"

if [ -w "$INSTALL_DIR" ]; then
    mv "$TMP_DIR/niki" "$INSTALL_DIR/niki"
else
    sudo mv "$TMP_DIR/niki" "$INSTALL_DIR/niki"
fi

echo "✓ NIKI installed successfully to $INSTALL_DIR/niki!"

if ! command -v niki >/dev/null 2>&1; then
    if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
        echo ""
        echo "Note: $INSTALL_DIR is not currently in your PATH."
        echo "Add it to your shell configuration:"
        echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
    fi
fi

if command -v niki >/dev/null 2>&1; then
    niki --version
fi
