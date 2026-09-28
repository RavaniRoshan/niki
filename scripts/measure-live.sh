#!/usr/bin/env bash
# Measure the live pipeline against a real model, the way a user would.
#
#   ./scripts/measure-live.sh [runs]
#
# Requires a configured project pointed at a reachable model. Against the local
# `qwen2.5-coder:3b` this is the measurement the harness changes were judged on,
# and it is deliberately a *user's* path — no mocks, no test doubles, the real
# worktree backend, the real four agents.
#
# Why a script rather than a test: the number depends on the model, the machine
# and the network, so it cannot gate CI. But it must be repeatable by a person,
# or every claim about live quality is a story rather than a measurement.
#
# The timeout is generous on purpose. A three-round run took 121s on this box;
# an earlier sweep used 280s and killed a run that was still working, which is
# indistinguishable from a failure when you are counting pass rates. Under-
# reporting a failure you caused yourself is the easiest way to make a product
# look worse than it is — and, later, better than it is.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

RUNS="${1:-4}"
BIN="${NIKI_BIN:-$REPO_ROOT/target/debug/niki}"
[ -x "$BIN" ] || { echo "no binary at $BIN — cargo build --bin niki" >&2; exit 1; }

PROJECT="${NIKI_MEASURE_PROJECT:-/tmp/niki-measure}"
TASK="Add a public function named tally that sums a slice of integers, with a doc comment"
TIMEOUT="${NIKI_MEASURE_TIMEOUT:-600}"

# A throwaway project, committed once so every run starts from the same place.
if [ ! -d "$PROJECT/.git" ]; then
    rm -rf "$PROJECT"; mkdir -p "$PROJECT/src"; cd "$PROJECT"
    git init -q
    git config user.email "measure@niki.local"
    git config user.name "niki measure"
    echo 'pub fn add(a: i32, b: i32) -> i32 { a + b }' > src/lib.rs
    git add -A && git commit -q -m base
fi

pass=0; fail=0
declare -a notes=()
echo "model: $("$BIN" doctor --category providers 2>/dev/null | grep -iE 'provider|model' | head -2 | tr '\n' ' ')"
echo "task:  $TASK"
echo "runs:  $RUNS   timeout: ${TIMEOUT}s"
echo

for i in $(seq 1 "$RUNS"); do
    cd "$PROJECT"
    rm -rf .niki
    git branch -D 'niki/*' >/dev/null 2>&1
    printf 'run %s: ' "$i"
    if timeout "$TIMEOUT" "$BIN" run --project "$PROJECT" --backend worktree --bare "$TASK" \
        >"/tmp/niki-measure-$i.log" 2>&1; then
        pass=$((pass + 1))
        printf 'PASS  %s\n' "$(grep -oE 'Latency: [0-9.]+s' "/tmp/niki-measure-$i.log" | tail -1)"
    else
        fail=$((fail + 1))
        stage="$(grep -oE '\[(Coder|Tester|Reviewer|Planner)\] Error' "/tmp/niki-measure-$i.log" | head -1 | tr -d '[]' | cut -d']' -f1)"
        reason="$(grep -oE '(Error: .{0,70})' "/tmp/niki-measure-$i.log" | head -1)"
        if [ -z "$reason" ] && tail -3 "/tmp/niki-measure-$i.log" | grep -q SIGTERM; then
            reason="TIMED OUT after ${TIMEOUT}s — raise NIKI_MEASURE_TIMEOUT; this is not a failure"
        fi
        printf 'FAIL  %s %s\n' "$stage" "$reason"
        notes+=("run $i: $stage $reason")
    fi
done

echo
echo "---"
printf 'live pipeline: %d/%d completed, %d failed\n' "$pass" "$((pass + fail))" "$fail"
if [ "${#notes[@]}" -gt 0 ]; then
    printf '  %s\n' "${notes[@]}"
fi
[ "$fail" -eq 0 ]
