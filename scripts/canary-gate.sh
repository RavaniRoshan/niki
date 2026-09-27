#!/usr/bin/env bash
# Canary gate — does the suite actually fail when the product is broken?
#
# A harness that only reports a pass rate cannot answer that. This script
# injects one known defect at a time from `mutants/canaries.toml`, runs the
# probe target, and records whether the suite noticed. The gate is computed on
# the HELD-OUT canaries alone, because optimising on the whole corpus reaches
# ~98% in-sample and ~95% held-out — the gap is real and reporting only the
# in-sample number is how a harness ends up flattering itself.
#
# It is a script rather than a test on purpose: it mutates the working tree,
# and a test that rewrites the source it is checking is not a test.
#
#   ./scripts/canary-gate.sh              # run every canary
#   ./scripts/canary-gate.sh --list       # print the corpus
#   ./scripts/canary-gate.sh --record     # also rewrite canaries/results.json
#
# Safety: refuses to start on a dirty tree, reverts after every canary, and
# verifies the tree is byte-identical to the starting state before reporting.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

CANARIES="mutants/canaries.toml"
RESULTS="canaries/results.json"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
MIN_FREE_MB="${NIKI_MIN_FREE_MB:-1200}"

red()   { printf '\033[31m%s\033[0m\n' "$*"; }
green() { printf '\033[32m%s\033[0m\n' "$*"; }
info()  { printf '\033[2m%s\033[0m\n' "$*"; }

if [ "${1:-}" = "--list" ]; then
  python3 - "$CANARIES" <<'PY'
import sys, tomllib
d = tomllib.load(open(sys.argv[1], 'rb'))
for c in d['canary']:
    eq = " (equivalent)" if c.get('equivalent') else ""
    print(f"{c['id']:<38} {c['category']:<22} {c.get('split','?'):<8}{eq}")
    print(f"    rule: {c['invariant']}")
PY
  exit 0
fi

# ── Preflight ─────────────────────────────────────────────────────────────
if [ -n "$(git status --porcelain 2>/dev/null)" ]; then
  red "refusing to run: the working tree is dirty."
  red "The gate mutates source files; commit or stash first."
  exit 4
fi
if [ ! -f "$CANARIES" ]; then
  red "missing $CANARIES"
  exit 4
fi
avail="$(free -m | awk '/^Mem:/ {print $7}')"
if [ "${avail:-0}" -lt "$MIN_FREE_MB" ]; then
  red "refusing to run: ${avail} MiB available, ${MIN_FREE_MB} MiB required."
  red "Each canary triggers a rebuild."
  exit 4
fi

START_TREE="$(git status --porcelain; git rev-parse HEAD)"

info "canary gate: preflight ok (${avail} MiB free, tree clean)"

# ── Run ───────────────────────────────────────────────────────────────────
TMP_OUT="$(mktemp)"
trap 'rm -f "$TMP_OUT"; git checkout -- . 2>/dev/null' EXIT

python3 - "$CANARIES" > "$TMP_OUT" <<'PY'
import json, sys, tomllib
d = tomllib.load(open(sys.argv[1], 'rb'))
print(json.dumps([
    {
        "id": c["id"],
        "file": c["file"],
        "patch": c["patch"],
        # Optional: an equivalence canary declares a patch with no
        # replacement, i.e. the "mutation" changes nothing observable. Default
        # to the patch itself so the entry still round-trips.
        "replace_with": c.get("replace_with", c["patch"]),
        "category": c["category"],
        "invariant": c["invariant"],
        "expect_kill": c.get("expect_kill", True),
        "equivalent": c.get("equivalent", False),
        "split": c.get("split", "gate"),
        "probe": c.get("probe", "--lib"),
        "known_surviving": c.get("known_surviving", False),
        "equivalent": c.get("equivalent", False),
    }
    for c in d["canary"]
]))
PY

total=0; killed=0; survived=0; build_failed=0
holdout_killed=0; holdout_total=0
records="[]"

while IFS=$'\t' read -r id file patch replace_with probe; do
  [ -n "$id" ] || continue
  total=$((total + 1))
  info "--- $id ($probe) ---"

  # Apply the textual patch, refusing an ambiguous match. A patch that matches
  # nothing is a hard error, not a pass: it would otherwise be scored as
  # "survived" and quietly weaken the gate.
  python3 - "$file" "$patch" "$replace_with" <<'PY'
import sys
path, patch, repl = sys.argv[1], sys.argv[2], sys.argv[3]
s = open(path).read()
n = s.count(patch)
if n != 1:
    print(f"MATCH_COUNT={n}")
    sys.exit(9)
open(path, 'w').write(s.replace(patch, repl, 1))
print("OK")
PY
  if [ $? -ne 0 ]; then
    red "  patch did not apply to exactly one site in $file — aborting"
    git checkout -- .
    exit 5
  fi

  # shellcheck disable=SC2086
  if timeout 900 cargo test $probe -j 2 -- --test-threads=1 > /tmp/canary-run.log 2>&1; then
    outcome="survived"
    survived=$((survived + 1))
    red "  SURVIVED — the suite passed with the defect injected"
  else
    outcome="killed"
    killed=$((killed + 1))
    green "  killed"
  fi

  records=$(python3 -c "
import json,sys
r = json.loads(sys.argv[1])
r.append({'id': sys.argv[2], 'category': sys.argv[3], 'invariant': sys.argv[4],
          'split': sys.argv[5], 'probe': sys.argv[6], 'outcome': sys.argv[7],
          'expect_kill': sys.argv[8] == 'true'})
print(json.dumps(r))
" "$records" "$id" \
    "$(python3 -c "import json,sys;print(next(c['category'] for c in json.load(open(sys.argv[1])) if c['id']==sys.argv[2]))" "$TMP_OUT" "$id")" \
    "$(python3 -c "import json,sys;print(next(c['invariant'] for c in json.load(open(sys.argv[1])) if c['id']==sys.argv[2]))" "$TMP_OUT" "$id")" \
    "$(python3 -c "import json,sys;print(next(c['split'] for c in json.load(open(sys.argv[1])) if c['id']==sys.argv[2]))" "$TMP_OUT" "$id")" \
    "$probe" "$outcome" \
    "$(python3 -c "import json,sys;print(str(next(c['expect_kill'] for c in json.load(open(sys.argv[1])) if c['id']==sys.argv[2])).lower())" "$TMP_OUT" "$id")")

  # Count the held-out split separately — that is the honest number.
  is_holdout=$(python3 -c "import json,sys;print(next(c['split'] for c in json.load(open(sys.argv[1])) if c['id']==sys.argv[2]))" "$TMP_OUT" "$id")
  if [ "$is_holdout" = "holdout" ]; then
    holdout_total=$((holdout_total + 1))
    [ "$outcome" = "killed" ] && holdout_killed=$((holdout_killed + 1))
  fi

  git checkout -- .
done < <(python3 -c "
import json,sys
for c in json.load(open(sys.argv[1])):
    print('\t'.join([c['id'], c['file'], c['patch'], c['replace_with'], c['probe']]))
" "$TMP_OUT")

# ── Report ────────────────────────────────────────────────────────────────
END_TREE="$(git status --porcelain; git rev-parse HEAD)"
if [ "$START_TREE" != "$END_TREE" ]; then
  red "the working tree was not restored — refusing to report a result"
  git checkout -- .
  exit 6
fi

mkdir -p "$(dirname "$RESULTS")"
python3 -c "
import json,sys
print(json.dumps({
  'schema': 1,
  'canaries': json.loads(sys.argv[1]),
  'totals': {
    'total': int(sys.argv[2]), 'killed': int(sys.argv[3]),
    'survived': int(sys.argv[4]),
    'holdout_total': int(sys.argv[5]), 'holdout_killed': int(sys.argv[6]),
  },
}, indent=2))
" "$records" "$total" "$killed" "$survived" "$holdout_total" "$holdout_killed" > "$RESULTS"

echo
info "========================================"
printf '  canaries: %s   killed: %s   survived: %s\n' "$total" "$killed" "$survived"
if [ "$holdout_total" -gt 0 ]; then
  rate=$(python3 -c "print(f'{$holdout_killed / $holdout_total:.2f}')")
  printf '  HELD-OUT kill rate: %s (%s/%s)\n' "$rate" "$holdout_killed" "$holdout_total"
else
  red "  no held-out canaries — the gate would be measuring nothing"
  exit 7
fi
echo "  report: $RESULTS"

# The gate. A survived canary means the suite cannot detect that defect --
# unless the corpus already declares it a known blind spot, in which case it
# is reported rather than counted as a regression.
python3 "$REPO_ROOT/scripts/canary-gate-summary.py" "$RESULTS"
status=$?

if [ "$status" -ne 0 ]; then
  red
  red "GATE FAILED: a canary expected to be killed survived injection."
  red "That is a class of defect this suite cannot detect. Either add the missing"
  red "test, or record it in the corpus as known_surviving with the reason it"
  red "cannot be closed today."
  exit 1
fi

green
green "GATE PASSED: every expected canary was killed; declared blind spots are listed above."
