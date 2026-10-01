#!/usr/bin/env bash
# Every download URL a package manifest names must resolve.
#
# Extracted from the `Manifest parity` CI job so there is **one** copy of the
# rule. The workflow used to hold it inline, which meant the only place that
# could be exercised locally was a push — so a dead `brew install` URL, the
# headline install command in the README, could not be caught before it
# shipped. That is exactly what happened: the manifests pointed at a release
# that was never published, `Cargo.toml` had already moved past it, and the
# job caught it only in CI.
#
# The rule, in order:
#
#   1. a `$`-containing URL is a package-manager template (scoop autoupdate),
#      not a link, so it is skipped rather than curled;
#   2. a URL that resolves is fine;
#   3. a 404 on an asset is a real failure **if** the release it names exists;
#   4. a missing release tag is a real failure **if** this repository has
#      already declared that version shipped — `Cargo.toml` at that version, or
#      a dated `CHANGELOG.md` entry. "Just early" is doing real work there, and
#      before this check it did so silently: three manifests pointed at
#      releases/download/v0.9.0, the newest published release was v0.8.0, and
#      the job was green.
#
# Usage: scripts/manifest-parity.sh [--offline]
#   --offline  checks the manifests' internal agreement (versions, asset names)
#              and skips every network call. Use where there is no network.
set -uo pipefail
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)" || exit 1

REPO="${GITHUB_REPOSITORY:-RavaniRoshan/niki}"
OFFLINE=0
[ "${1:-}" = "--offline" ] && OFFLINE=1

fail=0
urls=$(grep -rhoE 'https://github\.com/[^ "]*\.(tar\.gz|tar\.xz|zip)' homebrew/ scoop/ winget/)

if [ -z "$urls" ]; then
  echo "no release URLs found in homebrew/ scoop/ winget/ — the grep is wrong," >&2
  echo "not the manifests" >&2
  exit 1
fi

crate=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)

for u in $urls; do
  case "$u" in
    *'$'*)
      echo "SKIP(template) $u"
      continue
      ;;
  esac
  if [ "$OFFLINE" -eq 1 ]; then
    echo "SKIP(offline)   $u"
    continue
  fi
  if curl -fIsS --max-time 30 "$u" -o /dev/null 2>/dev/null; then
    echo "OK   $u"
    continue
  fi
  tag=$(echo "$u" | sed -n 's|.*/releases/download/\([^/]*\)/.*|\1|p')
  if [ -n "$tag" ] && curl -fIsS --max-time 30 "https://github.com/$REPO/releases/tag/$tag" -o /dev/null 2>/dev/null; then
    echo "DEAD $u  (release $tag exists but the asset does not)"
    fail=1
    continue
  fi
  ver="${tag#v}"
  changelog_says_shipped=$(grep -cE "^## \[$ver\] - [0-9]{4}-[0-9]{2}-[0-9]{2}$" CHANGELOG.md 2>/dev/null || true)
  changelog_says_shipped=${changelog_says_shipped:-0}
  if [ "$ver" = "$crate" ] || [ "$changelog_says_shipped" -gt 0 ]; then
    echo "UNPUBLISHED $u  (this repo already declares $ver released: crate=$crate, changelog entries=$changelog_says_shipped)"
    fail=1
  else
    echo "PENDING  $u  (release $tag not published yet; crate is $crate)"
  fi
done

if [ "$fail" -ne 0 ]; then
  echo "manifest parity: FAILED — an install command a user is told to run 404s" >&2
  exit 1
fi
echo "manifest parity: ok"
