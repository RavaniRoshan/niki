#!/usr/bin/env bash
# Entry point for the tmux-based TUI smoke suite.
#
#   ./run.sh                 # run all cases (binary must already be built)
#   ./run.sh --build         # cargo build --release first
#   ./run.sh --bin PATH      # use a specific binary
#   ./run.sh 03 05           # run only matching cases
#
# Cases live in cases/*.sh; each defines a `run` function. The harness is
# offline/headless-first (no network, no credentials). See lib.sh and
# research/headless-tui-testing.md.

set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$HERE/lib.sh"

BUILD=0
SELECTED=()
while [ $# -gt 0 ]; do
  case "$1" in
    --build) BUILD=1 ;;
    --bin) shift; NIKI_BIN="$1" ;;
    [0-9]*) SELECTED+=("$1") ;;
    -*) echo "unknown flag: $1" >&2; exit 2 ;;
    *) echo "unexpected arg: $1" >&2; exit 2 ;;
  esac
  shift
done

[ "$BUILD" = "1" ] && cargo build --release

NIKI_BIN="${NIKI_BIN:-$HERE/../../target/release/niki}"
if [ ! -x "$NIKI_BIN" ]; then
  echo "binary not found/executable at: $NIKI_BIN" >&2
  echo "build it (run.sh --build) or pass --bin <path>" >&2
  exit 1
fi

if [ "${#SELECTED[@]}" -eq 0 ]; then
  shopt -s nullglob
  CASE_FILES=("$HERE"/cases/*.sh)
  shopt -u nullglob
else
  CASE_FILES=()
  for c in "${SELECTED[@]}"; do
    matched=( "$HERE"/cases/"${c}"*.sh )
    if [ "${#matched[@]}" -eq 0 ]; then echo "no case matching $c" >&2; exit 1; fi
    CASE_FILES+=("${matched[@]}")
  done
fi

PASS=0; FAIL=0; SKIP=0
for cf in "${CASE_FILES[@]}"; do
  [ -f "$cf" ] || continue
  CASENAME="$(basename "$cf" .sh)"
  echo "=== $CASENAME ==="
  set +e
  # The screen is captured from the EXIT trap, not from here.
  #
  # `tui_begin` installs `trap 'tui_kill; ...' EXIT` **inside** this subshell, so
  # the moment `run` returns, the tmux server is dead — which is why the parent
  # used to capture nothing (twelve failure logs, all 0 bytes, and §9.2a
  # stalled with no screen to read). See `tui_finish` in lib.sh.
  #
  # Note what is deliberately *not* done here: `set +e` inside the subshell.
  # That would look like a fix and is the opposite of one — a case whose third
  # assertion fails and whose last command succeeds would report OK, and a
  # suite that can be made green by disabling its own errexit is not a suite.
  ( SMOKE_CASE="$CASENAME"; export SMOKE_CASE; source "$cf"; run )
  rc=$?
  set -e
  if [ "$rc" -eq 0 ]; then PASS=$((PASS+1)); echo "  OK"
  elif [ "$rc" -eq 77 ]; then SKIP=$((SKIP+1)); echo "  SKIP"
  else
    FAIL=$((FAIL+1))
    echo "  FAIL (rc=$rc)"
  fi
done

echo "-----------------------------------"
echo "RESULT  pass=$PASS  fail=$FAIL  skip=$SKIP"
[ "$FAIL" -eq 0 ]
