#!/usr/bin/env bash
# Does `scripts/mega-e2e.sh` actually fail when it should?
#
#   ./scripts/mega-e2e-selftest.sh
#
# The mega leg's one distinctive claim is that it checks the code the agent
# wrote, not just the artifacts around it. That claim is only worth something
# if the check has been seen firing. Nothing had ever demonstrated it: every
# run of the script was a run in which the product did the right thing, so a
# script that checked nothing at all would have looked identical.
#
# So this runs the script against a stand-in binary in four states and asserts
# the verdict in each. Three of the four are failures. If a change to the
# script makes it pass a run that produced broken code, an empty branch, or
# advice it cannot back up, this goes red — which is the only way the gate
# stays a gate.
#
# Cheap on purpose: no network, no model, no build. The real product's leg is
# `scripts/mega-e2e.sh`; this one is about the script.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FAKE="$REPO_ROOT/tests/integration/fake_niki.sh"
SCRIPT="$REPO_ROOT/scripts/mega-e2e.sh"
[ -x "$FAKE" ] || { echo "fake_niki.sh is not executable" >&2; exit 1; }

PASS=0
FAIL=0

# run_case <mode> <expected: pass|fail> <what it is proving>
run_case() {
  local mode="$1" expect="$2" what="$3"
  local work
  work="$(mktemp -d)"
  local out
  out="$(
    cd "$work" || exit 1
    FAKE_NIKI_MODE="$mode" \
    NIKI_BIN="$FAKE" \
    NIKI_BASE_URL=http://127.0.0.1:9 \
    NIKI_MODEL=mock-model \
    NIKI_PROVIDER=openai \
    NIKI_MEGA_TIMEOUT=120 \
      bash "$SCRIPT" 2>&1
  )"
  local rc=$?
  if [ "$expect" = "pass" ] && [ "$rc" -eq 0 ]; then
    printf '  ok    %-10s %s\n' "$mode" "$what"; PASS=$((PASS + 1))
  elif [ "$expect" = "fail" ] && [ "$rc" -ne 0 ]; then
    printf '  ok    %-10s %s\n' "$mode" "$what"; PASS=$((PASS + 1))
  else
    printf '  FAIL  %-10s %s (exit %s, wanted %s)\n' "$mode" "$what" "$rc" "$expect"
    printf '%s\n' "$out" | sed 's/^/          | /' | tail -25
    FAIL=$((FAIL + 1))
  fi
  rm -rf "$work"
}

echo "── a correct run passes ────────────────────────────────────────"
run_case good pass "a correct branch, checked and accepted"

echo
echo "── and a bad one does not ─────────────────────────────────────"
# These three are the point of this file. Each is a way the product can look
# fine and be useless, and each is invisible to every other gate here.
run_case broken fail "a plausible patch whose code fails the fixture's tests"
run_case no-branch fail "a run that completes and hands back nothing"
run_case no-model fail "a report that admits its advice is unverified"

echo
echo "──"
if [ "$FAIL" -eq 0 ]; then
  echo "mega-e2e selftest: $PASS/$PASS"
  exit 0
fi
echo "mega-e2e selftest: $PASS passed, $FAIL failed"
exit 1
