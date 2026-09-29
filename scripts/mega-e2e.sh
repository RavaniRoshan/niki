#!/usr/bin/env bash
# The whole product, through the path a person actually takes, judged on
# whether the result works.
#
#   NIKI_BASE_URL=http://127.0.0.1:11434/v1 NIKI_MODEL=qwen2.5-coder:3b \
#     ./scripts/mega-e2e.sh
#
#   OPENROUTER_API_KEY=sk-... NIKI_BASE_URL=https://openrouter.ai/api/v1 \
#   NIKI_MODEL=anthropic/claude-sonnet-4 ./scripts/mega-e2e.sh
#
# Every other suite in this repository proves that a *module* behaves. This
# one proves the *product* does, and it is deliberately built so that the only
# thing that can make it pass is a working change:
#
#   1. A real HTTP server on a real socket. Not the in-process mock provider,
#      which is a Rust struct that returns a canned `CompletionResponse` and
#      therefore never exercises URL building, headers, status codes, SSE
#      framing, or a connection that drops mid-stream.
#   2. A real fixture with a real test suite that passes before the run and
#      must still pass after. A change that breaks the build is a failure even
#      if every artifact validates.
#   3. The branch is checked out and *the fixture's own tests are run against
#      it*. This is the assertion that is missing everywhere else. Every other
#      gate in this repo checks that NIKI produced artifacts of the right
#      shape; none of them check that the code it wrote does anything. A
#      harness that emits a perfectly-formed patch that does not compile is
#      indistinguishable from one that works, right up until a user tries it.
#   4. The task is specified by a test that does not exist yet, so "did it
#      work" has a mechanical answer rather than a reviewer's opinion.
#
# Why a script and not a test: the outcome depends on the model. It cannot
# gate a PR, and a test that fails 40% of the time on a small model teaches
# everyone to ignore red. So the *plumbing* leg runs in CI against the scripted
# server (see `tests/ci_contracts.rs` and the `mega-e2e` job), and this script
# is the leg a person runs against a model they trust.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${NIKI_BIN:-$REPO_ROOT/target/release/niki}"
BASE_URL="${NIKI_BASE_URL:-}"
MODEL="${NIKI_MODEL:-}"
PROVIDER="${NIKI_PROVIDER:-openai}"
# Uppercase, because that is how the key is looked up. `PROVIDER` is the
# config's spelling (`[providers.openai]`, provider name `openai`) and the
# env var is `OPENAI_API_KEY`; deriving one from the other without
# uppercasing produced `openai_API_KEY`, which nothing reads — so the run
# died with "OpenAI API key not configured" against a local endpoint that
# needs no key at all, and the script blamed the provider.
KEY_ENV="${NIKI_KEY_ENV:-$(printf '%s' "$PROVIDER" | tr '[:lower:]' '[:upper:]')_API_KEY}"
TIMEOUT="${NIKI_MEGA_TIMEOUT:-900}"
WORK="${NIKI_MEGA_WORK:-$(mktemp -d)}"
KEEP=0
[ "${1:-}" = "--keep" ] && KEEP=1

if [ -z "$BASE_URL" ] || [ -z "$MODEL" ]; then
  cat >&2 <<'USAGE'
usage: NIKI_BASE_URL=<url> NIKI_MODEL=<id> ./scripts/mega-e2e.sh [--keep]

  NIKI_BASE_URL   an OpenAI-compatible base URL, e.g. http://127.0.0.1:11434/v1
  NIKI_MODEL      a model id that endpoint serves
  NIKI_PROVIDER   the [providers.*] name to write (default: openai)
  NIKI_KEY_ENV    env var holding the key (default: <PROVIDER>_API_KEY)
USAGE
  exit 2
fi

cyan() { printf '\033[36m%s\033[0m\n' "$*"; }
grn()  { printf '\033[32m%s\033[0m\n' "$*"; }
red()  { printf '\033[31m%s\033[0m\n' "$*" >&2; }
dim()  { printf '\033[2m%s\033[0m\n' "$*"; }
FAILURES=0
check() { # check <description> <command...>
  local what="$1"; shift
  if "$@" >/dev/null 2>&1; then grn "  ok    $what"
  else red "  FAIL  $what"; FAILURES=$((FAILURES + 1)); fi
}

cleanup() {
  if [ "$KEEP" -eq 1 ]; then cyan "kept: $WORK/fixture"
  else rm -rf "$WORK"; fi
}
trap cleanup EXIT

[ -x "$BIN" ] || { red "no binary at $BIN — cargo build --release"; exit 1; }

# ── 1. The fixture ────────────────────────────────────────────────────────
# Python, not Rust: this box is RAM-constrained and a `cargo build --release`
# here is the single largest memory event in the whole project. A Python
# fixture also means the language the agent writes is not the language it is
# being tested in, which is closer to a user's real request.
#
# The starting test suite is non-trivial on purpose — three tests, one of them
# failing if the module is empty — so "the change did nothing" is a failure,
# not a pass.
FIX="$WORK/fixture"
rm -rf "$FIX"; mkdir -p "$FIX/src" "$FIX/tests"
cd "$FIX"
git init -q
git config user.email "mega@niki.local"
git config user.name "niki mega"

cat >src/stats.py <<'PY'
"""Small statistics helpers."""


def mean(values):
    """Arithmetic mean of a non-empty sequence of numbers."""
    if not values:
        raise ValueError("mean() of an empty sequence is undefined")
    return sum(values) / len(values)
PY

cat >tests/test_stats.py <<'PY'
import sys, os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))
from stats import mean


def test_mean_of_a_single_value():
    assert mean([4]) == 4


def test_mean_of_several_values():
    assert mean([1, 2, 3, 4]) == 2.5


def test_median_is_required_too():
    from stats import median
    assert median([3, 1, 2]) == 2


def test_median_of_an_even_count_averages_the_middle_two():
    # This assertion is the load-bearing one, and it arrived late.
    #
    # With only the odd-length case, `median` was satisfiable by
    # `ordered[n // 2]` — which is wrong for every even-length input and
    # right for every odd one. The suite went red before the change and
    # green after, the branch was produced, the artifacts validated, and a
    # function that is wrong half the time passed every check the repository
    # has. The harness self-test found it by asking whether a deliberately
    # wrong implementation is rejected, which no test of the product could
    # ever ask.
    from stats import median
    assert median([1, 2, 3, 4]) == 2.5
PY

cat >niki.toml <<TOML
[docker]
backend = "worktree"

[pipeline]
topology = "multiagent"

[providers.$PROVIDER]
base_url = "$BASE_URL"
default_model = "$MODEL"

[agents.planner]
provider = "$PROVIDER"
model = "$MODEL"

[agents.coder]
provider = "$PROVIDER"
model = "$MODEL"

[agents.tester]
provider = "$PROVIDER"
model = "$MODEL"
test_command = "python3 -m pytest tests/ -q"

[agents.reviewer]
provider = "$PROVIDER"
model = "$MODEL"
TOML

git add -A && git commit -q -m base
BASE=$(git rev-parse HEAD)

# The key goes in the environment, not the file. niki.toml is git-ignored
# inside a real project but the fixture's is about to be committed on the
# `niki/*` branch, and a secret in an artifact a user may push is the worst
# bug this script could ship.
if [ -z "${!KEY_ENV:-}" ]; then
  # A local endpoint (ollama, a proxy, the scripted server) ignores
  # Authorization, but `niki providers models` still refuses to fetch a
  # catalogue without one and reports "no key" rather than "endpoint said
  # nothing" — which would make the script fail on a perfectly good local
  # model. A placeholder is honest here: the run is unauthenticated either
  # way, and a real hosted provider answers 401 in a second, loudly.
  export "$KEY_ENV=local-endpoint-no-auth"
  dim "\$$KEY_ENV unset — sending a placeholder; a real provider will 401"
else
  dim "key from \$$KEY_ENV"
fi

# ── 2. The fixture's own tests, before ────────────────────────────────────
# If the harness starts from a red suite it can "pass" by accident, and the
# whole run becomes uninterpretable.
cyan "── 0. the fixture must be red before the agent runs ─────────────"
if python3 -m pytest tests/ -q 2>&1 | tail -3; then :; fi
if python3 -m pytest tests/ -q >/dev/null 2>&1; then
  red "the fixture's tests pass before any change — the task cannot be measured"
  exit 1
fi
grn "  ok    it fails for the right reason (no median yet)"

# ── 3. Catalogue, through HTTP ────────────────────────────────────────────
# The first thing a new user does after `doctor` is ask what they can pick.
# This is the only place in the repository that asks that question over a
# socket.
cyan "── 1. the catalogue, over a real socket ─────────────────────────"
if "$BIN" providers models --provider "$PROVIDER" --plain 2>&1 | head -8; then :; fi
if "$BIN" providers models --provider "$PROVIDER" --plain 2>/dev/null | grep -q .; then
  grn "  ok    the endpoint answered a model list"
else
  red "  FAIL  $BASE_URL/models returned nothing niki could parse"
  FAILURES=$((FAILURES + 1))
fi

# ── 4. The run ────────────────────────────────────────────────────────────
# Single-quoted, deliberately. The task is prose *about code*: it contains
# backticks and parentheses, which an unquoted heredoc hands to the shell. The
# first version did, and bash duly ran `mean` as a command and choked on
# `median(values)` — so the run was handed a task that had been mangled by
# the harness, and then failed for reasons that had nothing to do with the
# product. A test that corrupts its own input is worse than no test, because
# the failure it produces is real and the cause is invisible.
TASK='Add a `median(values)` function to src/stats.py alongside `mean`. It
returns the middle value of an odd-length sequence and the mean of the two
middle values for an even-length one, raises ValueError on an empty sequence,
and has a doc comment. The test suite in tests/ must pass afterwards.'
cyan "── 2. niki run ─────────────────────────────────────────────────"
rm -rf .niki
START=$(date +%s)
if timeout "$TIMEOUT" "$BIN" run "$TASK" --project "$FIX" --backend worktree --quiet \
     >"$WORK/run.log" 2>&1; then
  grn "  ok    the run completed in $(( $(date +%s) - START ))s"
else
  rc=$?
  red "  FAIL  the run exited $rc (log: $WORK/run.log)"
  tail -25 "$WORK/run.log" >&2
  exit 1
fi

# ── 5. Did it produce a branch with a real change? ────────────────────────
cyan "── 3. the branch ───────────────────────────────────────────────"
BRANCH=$(git branch --format='%(refname:short)' --list 'niki/*' | head -1)
if [ -z "$BRANCH" ]; then
  red "  FAIL  no niki/* branch — the run handed back nothing"
  exit 1
fi
grn "  ok    $BRANCH"
STAT=$(git diff --stat "$BASE...$BRANCH" | tail -1)
dim "  $STAT"
check "the branch actually changes files" test -n "$(git diff --name-only "$BASE...$BRANCH")"

TASKDIR=$(find "$FIX/.niki/tasks" -maxdepth 1 -mindepth 1 -type d 2>/dev/null | head -1)
if [ -n "$TASKDIR" ]; then
  for f in changes.patch report.md task.json; do
    check "$f is present and non-empty" test -s "$TASKDIR/$f"
  done
  check "changes.patch is a real unified diff" grep -q 'diff --git' "$TASKDIR/changes.patch"
else
  red "  FAIL  no .niki/tasks/<id>/ — the run left no audit trail"
  FAILURES=$((FAILURES + 1))
fi

# ── 6. The assertion that matters ─────────────────────────────────────────
# Everything above is shape. This is substance: check the branch out and run
# the fixture's own tests against the code NIKI wrote.
cyan "── 4. does the code work? ─────────────────────────────────────"
WT="$WORK/verify"
rm -rf "$WT"
git worktree add -q --detach "$WT" "$BRANCH" 2>/dev/null
if [ ! -d "$WT" ]; then
  red "  FAIL  the branch could not be checked out — a branch nobody can use is not a result"
  FAILURES=$((FAILURES + 1))
else
  if (cd "$WT" && python3 -m pytest tests/ -q >"$WORK/verify.log" 2>&1); then
    grn "  ok    the fixture's tests pass on the branch NIKI produced"
  else
    red "  FAIL  the tests that were red are still red on the branch"
    tail -30 "$WORK/verify.log" >&2
    FAILURES=$((FAILURES + 1))
  fi
  check "the new function is actually present" \
    grep -qrE 'def +median' "$WT/src"
  git worktree remove --force "$WT" 2>/dev/null || true
fi

# ── 7. The report a user reads ────────────────────────────────────────────
# `recommend` is the one command that prints advice. If it prints advice it
# cannot back up, a first-time user has no way to tell.
cyan "── 5. the advice the product gives ─────────────────────────────"
REC=$("$BIN" recommend --project "$FIX" 2>&1)
if grep -q 'Not offered by' <<<"$REC"; then
  grn "  ok    recommend checked the catalogue and flagged what this account cannot run"
else
  dim "  note  no 'not offered' line — either everything checked out, or nothing was checked"
fi
if grep -q 'Unverified' <<<"$REC"; then
  red "  FAIL  recommend says its advice is unverified: the catalogue was not read"
  sed -n '1,8p' <<<"$REC" >&2
  FAILURES=$((FAILURES + 1))
else
  grn "  ok    recommend does not present unchecked advice as checked"
fi

echo
echo "──"
if [ "$FAILURES" -eq 0 ]; then
  grn "mega-e2e: PASS  ($MODEL via $BASE_URL)"
  dim "log: $WORK/run.log"
  exit 0
fi
red "mega-e2e: $FAILURES check(s) failed  ($MODEL via $BASE_URL)"
dim "log: $WORK/run.log"
exit 1
