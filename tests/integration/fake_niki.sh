#!/usr/bin/env bash
# A stand-in for the `niki` binary, for testing `scripts/mega-e2e.sh` itself.
#
#   FAKE_NIKI_MODE=good      a correct branch, artifacts, a clean report
#   FAKE_NIKI_MODE=broken    a branch whose code does not work
#   FAKE_NIKI_MODE=no-branch a run that completes and hands back nothing
#   FAKE_NIKI_MODE=no-model  a catalogue the provider cannot read
#
# Why this exists: a gate that has only ever been seen green is not known to
# be a gate. `scripts/mega-e2e.sh` asserts that the branch NIKI produces
# actually passes the fixture's tests — the one assertion in this repository
# that could catch a well-formed patch which does not compile — and nothing
# had ever demonstrated that assertion firing. Wiring the real product in
# proves the plumbing; it cannot prove the check, because the product does
# what it does and the check always agreed.
#
# So the check is run against a binary that deliberately misbehaves, and it
# has to notice. If a change to the script stops it noticing, that is a
# regression in the only gate that would have caught a plausible-looking
# useless product.
#
# This is a test fixture, not a mock of anything the product depends on. It
# never talks to a network and never pretends to be a language model; it only
# plays the part of "a program that leaves a git repository in some state".
set -uo pipefail

MODE="${FAKE_NIKI_MODE:-good}"
PROJECT="."
prev=""
for arg in "$@"; do
  [ "$prev" = "--project" ] && PROJECT="$arg"
  prev="$arg"
done

case "${1:-}" in
  providers)
    # `niki providers models --provider X --plain`
    if [ "$MODE" = "no-model" ]; then
      echo "Error: could not reach http://127.0.0.1:1/models" >&2
      exit 1
    fi
    printf 'mock-model\nanthropic/claude-sonnet-4\n'
    exit 0
    ;;
  recommend)
    if [ "$MODE" = "no-model" ]; then
      # What the report looks like when nothing could be checked. The
      # product now says so out loud; if a change makes it silent again,
      # this is the string the script must catch.
      cat <<'OUT'
# NIKI Model Recommendations

Preference: `balanced` · est. tokens/run: 1000 in / 500 out

> **Unverified.** 1 provider is configured, but no catalogue could be read from
> any of them (no API key, or no `/models` endpoint). The models below were **not**
> checked; treat them as a static opinion, not as advice for this account.

## coder  (`openai`)
  - Recommended now: **claude-opus-4** (`anthropic`)
OUT
    else
      cat <<'OUT'
# NIKI Model Recommendations

Preference: `balanced` · est. tokens/run: 1000 in / 500 out

## coder  (`anthropic`)
  - Recommended now: **claude-sonnet-4** (`anthropic`)
  - Est. cost/run: $0.0012
OUT
    fi
    exit 0
    ;;
  run)
    ;;
  *)
    echo "fake niki: unrecognised command '${1:-}'" >&2
    exit 64
    ;;
esac

cd "$PROJECT" || exit 1

if [ "$MODE" = "no-branch" ]; then
  # A run that completes cleanly and produces nothing. The shape of failure
  # that is easiest to mistake for success.
  mkdir -p .niki
  echo "run complete"
  exit 0
fi

BRANCH="niki/$(date +%s)"
git checkout -q -b "$BRANCH" || exit 1

if [ "$MODE" = "broken" ]; then
  # Plausible, well-formed, and wrong: it defines the function the task asked
  # for, it has a doc comment, it raises on empty — and it returns the wrong
  # middle value for an even-length sequence. Only running the fixture's own
  # tests can tell this from a correct answer, which is the entire argument
  # for running them.
  cat >>src/stats.py <<'PY'


def median(values):
    """Middle value of a sequence."""
    ordered = sorted(values)
    n = len(ordered)
    if n == 0:
        raise ValueError("median() of an empty sequence is undefined")
    return ordered[n // 2]
PY
else
  cat >>src/stats.py <<'PY'


def median(values):
    """Middle value of a sequence, or the mean of the two middle values.

    Raises ValueError on an empty sequence. The input is not sorted in
    place; a copy is taken first.
    """
    ordered = sorted(values)
    n = len(ordered)
    if n == 0:
        raise ValueError("median() of an empty sequence is undefined")
    mid = n // 2
    if n % 2 == 1:
        return ordered[mid]
    return (ordered[mid - 1] + ordered[mid]) / 2
PY
fi

git add -A && git commit -q -m "niki: fake" || exit 1

TASKDIR=".niki/tasks/fake"
mkdir -p "$TASKDIR"
# A real `git diff` of the change this run made, rather than a hand-written
# approximation. The mega leg checks `changes.patch` is a unified diff, and a
# fixture that emitted something the product would never emit would make that
# check look broken instead of making the fixture look wrong.
git diff "master..$BRANCH" --no-color >"$TASKDIR/changes.patch" 2>/dev/null \
  || git diff "main..$BRANCH" --no-color >"$TASKDIR/changes.patch" 2>/dev/null \
  || git diff HEAD~1 --no-color >"$TASKDIR/changes.patch"
echo "# Report" >"$TASKDIR/report.md"
echo '{"agent_metrics":[]}' >"$TASKDIR/task.json"

echo "run complete on $BRANCH"
exit 0
