#!/bin/sh
# Purpose-specific, minimal bootstrap for the NIKI sandbox image.
#
# Installs ONLY the tooling the agent pipeline requires:
#   - git     (for `git apply` / `git diff` patch handling)
#   - python3 (declared runtime)
#   - node    (test project runtime, fetched as an official binary tarball)
#   - npm     (bundled with the node binary)
#
# It deliberately avoids general-purpose `apt-get install nodejs npm python3`,
# which pulls large dependency trees and is what made sandbox startup take 20+ min.
# Node is installed from the official binary distribution instead — a few-second
# extraction with no package-manager dependency resolution. The result is pre-bundled
# into the image so the running pipeline never installs anything.
#
# Base-specific note: on Wolfi (Chainguard) the system package manager is apk, so
# the apt-get calls from the old Ubuntu image are replaced with `apk add`. Node is
# still pulled as an upstream tarball to keep version resolution base-agnostic.
set -eu

NODE_MAJOR=20

# git + python3 + fetch tooling via apk (no recommended packages; keeps the layer small & fast).
apk add --no-cache git python3 ca-certificates wget xz

# Resolve the latest Node release for the requested major line.
ARCH=$(uname -m)
case "$ARCH" in
  x86_64)  NODE_ARCH=x64 ;;
  aarch64) NODE_ARCH=arm64 ;;
  arm64)   NODE_ARCH=arm64 ;;
  *)       NODE_ARCH=x64 ;;
esac

# busybox grep (Wolfi) has no -P, so use -E and strip the leading "v" afterwards
# (the original PCRE used \K to drop it; here we sed it off so the URL below
# stays `v<ver>` / `node-v<ver>`).
NODE_VERSION=$(wget -qO- "https://nodejs.org/dist/index.json" \
  | grep -oE "v${NODE_MAJOR}\.[0-9]+\.[0-9]+" | head -1 | sed 's/^v//')
if [ -z "$NODE_VERSION" ]; then
  echo "sandbox-bootstrap: could not resolve node ${NODE_MAJOR}.x version" >&2
  exit 1
fi

wget -q "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-${NODE_ARCH}.tar.xz" -O /tmp/node.tar.xz
tar -xJf /tmp/node.tar.xz -C /usr/local --strip-components=1
rm -f /tmp/node.tar.xz

# Sanity check the toolchain the pipeline depends on.
git --version
python3 --version
node --version
npm --version
