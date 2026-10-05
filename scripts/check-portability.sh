#!/usr/bin/env bash
# How old a machine can run this build? Measure it, don't assume it.
#
#   scripts/check-portability.sh target/release/niki            # report
#   scripts/check-portability.sh --floor 2.36 <binary>          # enforce
#   scripts/check-portability.sh --musl <binary>                # enforce fully static
#
# Why this exists: a Linux binary built on Ubuntu 24.04 requires `GLIBC_2.39`, and Debian 12
# ships 2.36. The artifact installs fine on the build machine and on nothing older — which is
# invisible until a user hits it, and a release that breaks on Debian is a broken release.
# Measured, the current build requires:
#
#   GLIBC_2.29  2.30  2.32  2.33  2.34  2.39
#
# The durable fix is a static musl build, which has no glibc dependency at all. Until that
# exists, a floor can still be *declared and enforced*, so a build that would ship too new is
# caught here rather than by a user.
#
# A gate that has only ever been green is not known to be a gate: this script is run in CI
# against the real release artifact, and it has been observed failing on this repository's own
# current release binary.

set -uo pipefail

FLOOR=""
WANT_MUSL=0
REPORT=0

while [ $# -gt 0 ]; do
    case "$1" in
        --floor) FLOOR="$2"; shift 2 ;;
        --floor=*) FLOOR="${1#*=}"; shift ;;
        --musl) WANT_MUSL=1; shift ;;
        --report) REPORT=1; shift ;;
        -h|--help) sed -n '2,26p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) break ;;
    esac
done

BIN="${1:-}"
[ -n "$BIN" ] && [ -f "$BIN" ] || { echo "usage: $0 [--floor X.Y] [--musl] <binary>" >&2; exit 2; }

if ! objdump -p "$BIN" >/dev/null 2>&1; then
    echo "portability: $BIN is not an ELF this box can inspect: $(file -b "$BIN")" >&2
    exit 2
fi

# ── static? ───────────────────────────────────────────────────────────────────
LINK_LINE="$(objdump -p "$BIN" 2>/dev/null | awk '$1 == "NEEDED" { print $2 }' | tr '\n' ' ')"
IS_STATIC=1
[ -n "$LINK_LINE" ] && IS_STATIC=0

if [ "$WANT_MUSL" -eq 1 ]; then
    if [ "$IS_STATIC" -eq 1 ]; then
        printf 'portability: %s is fully static — no glibc, no musl, no dynamic loader\n' "$BIN"
        exit 0
    fi
    echo "portability: $BIN is NOT a static build." >&2
    echo "  It needs: $LINK_LINE" >&2
    echo "  A musl build has no NEEDED entries at all. Check that the release job installed" >&2
    echo "  musl-tools and passed --target x86_64-unknown-linux-musl." >&2
    exit 1
fi

# ── glibc floor ───────────────────────────────────────────────────────────────
REQUIRED="$(objdump -p "$BIN" 2>/dev/null | grep -oE 'GLIBC_[0-9]+\.[0-9]+(\.[0-9]+)?' | sed 's/GLIBC_//' | sort -uV)"
if [ -z "$REQUIRED" ]; then
    printf 'portability: %s requires no glibc symbols.\n' "$BIN"
    exit 0
fi
HIGHEST="$(printf '%s\n' "$REQUIRED" | tail -1)"

printf 'portability: %s\n' "$BIN"
printf '  statically linked: %s\n' "$([ "$IS_STATIC" -eq 1 ] && echo yes || echo "no ($(echo "$LINK_LINE" | tr '\n' ' '))")"
printf '  glibc versions required: %s\n' "$(printf '%s\n' "$REQUIRED" | tr '\n' ' ')"
printf '  highest required: %s\n' "$HIGHEST"

[ "$REPORT" -eq 1 ] && exit 0
[ -z "$FLOOR" ] && exit 0

# Pass only when the build needs nothing NEWER than the floor. `sort -V | tail -1` is the
# larger of the two, so it equals the floor exactly when the requirement is met.
if [ "$(printf '%s\n%s\n' "$FLOOR" "$HIGHEST" | sort -V | tail -1)" = "$FLOOR" ]; then
    printf '  declared floor %s: OK\n' "$FLOOR"
    exit 0
fi
echo "  declared floor ${FLOOR}: FAILS — this build needs ${HIGHEST}." >&2
echo "  A user on an older distribution gets 'version GLIBC_${HIGHEST} not found'." >&2
exit 1