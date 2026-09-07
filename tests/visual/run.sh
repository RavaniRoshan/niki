#!/usr/bin/env bash
# Visual verification harness: per-page VHS captures + reference diff gate.
#
#   ./run.sh                  # capture frames/ + compare against reference/
#   ./run.sh --build          # cargo build --release first
#   ./run.sh --bin PATH       # use a specific binary
#   ./run.sh 01 04            # only tapes matching 01, 04, ...
#   REGEN=1 ./run.sh          # re-bless reference/ from frames/ (human reviews PNGs first)
#
# Determinism contract (documented, not accidental):
# - Fresh fixture project per tape (rm -rf + git init) so the onboarding
#   modal always appears and is always dismissed with Esc.
# - NIKI_REDUCED_MOTION=1 so spinners render static frames.
# - Fixed VHS geometry + font in every tape (see tapes/_head.tape.inc).
# - The `chat` command is fully offline (no keys, no network, no model calls).
# Residual nondeterminism (cursor blink phase) is absorbed by the diff
# threshold in compare.py — calibrated, see that file.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$HERE/../.."
NIKI_BIN="${NIKI_BIN:-$REPO/target/release/niki}"
FIXTURE="${VISUAL_FIXTURE:-/tmp/niki-visual-fixture}"
FRAMES="$HERE/frames"
REF="$HERE/reference"

BUILD=0
SELECTED=()
while [ $# -gt 0 ]; do
  case "$1" in
    --build) BUILD=1 ;;
    --bin) shift; NIKI_BIN="$1" ;;
    [0-9]*) SELECTED+=("$1") ;;
    *) echo "unknown flag: $1" >&2; exit 2 ;;
  esac
  shift
done

[ "$BUILD" = "1" ] && cargo build --release --manifest-path "$REPO/Cargo.toml"
[ -x "$NIKI_BIN" ] || { echo "binary not found: $NIKI_BIN" >&2; exit 1; }
command -v vhs >/dev/null 2>&1 || { echo "vhs not installed" >&2; exit 1; }

mkdir -p "$FRAMES"
WORK="$HERE/.work"
mkdir -p "$WORK"

TAPES=()
for t in "$HERE"/tapes/[0-9]*.tape; do
  base="$(basename "$t" .tape)"
  if [ "${#SELECTED[@]}" -eq 0 ]; then
    TAPES+=("$t")
  else
    for sel in "${SELECTED[@]}"; do
      case "$base" in *"$sel"*) TAPES+=("$t"); break ;; esac
    done
  fi
done
[ "${#TAPES[@]}" -gt 0 ] || { echo "no tapes selected" >&2; exit 1; }

fail=0
for tape in "${TAPES[@]}"; do
  base="$(basename "$tape" .tape)"
  # Fresh fixture per tape: onboarding modal always appears, always Esc'd.
  rm -rf "$FIXTURE"
  mkdir -p "$FIXTURE"
  git init -q "$FIXTURE" 2>/dev/null || true
  # VHS does not expand env vars: stamp absolute paths into a work copy.
  stamped="$WORK/$base.tape"
  sed -e "s|@BIN@|$NIKI_BIN|g" -e "s|@FIXTURE@|$FIXTURE|g" -e "s|@FRAMES@|$FRAMES|g" "$tape" > "$stamped"
  echo "== tape $base"
  if ! vhs "$stamped" >"$FRAMES/$base.log" 2>&1; then
    echo "   VHS FAILED ($base) — see $FRAMES/$base.log"
    fail=1
  fi
done

if [ "${REGEN:-0}" = "1" ]; then
  echo "REGEN=1: blessing reference/ from frames/ — review the PNGs before committing."
  mkdir -p "$REF"
  for png in "$FRAMES"/[0-9]*.png; do
    [ -e "$png" ] || continue
    cp "$png" "$REF/$(basename "$png")"
  done
  exit "$fail"
fi

python3 "$HERE/compare.py" "$FRAMES" "$REF" || fail=1
exit "$fail"
