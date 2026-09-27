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
trap 'rm -f "$TMP_OUT" "$OUTCOMES"; git checkout -- . 2>/dev/null' EXIT

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

OUTCOMES="$(mktemp)"
: > "$OUTCOMES"

while IFS=$'\t' read -r id file patch replace_with probe; do
  [ -n "$id" ] || continue
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
    red "  SURVIVED — the suite passed with the defect injected"
  else
    outcome="killed"
    green "  killed"
  fi

  # Record the outcome as a TSV line. Earlier versions carried the whole
  # accumulating record as a JSON string in a shell variable and re-parsed it
  # with Python on every canary. The quoting broke silently: the assignment
  # went empty, and results.json reported zero canaries for a run that had
  # just printed eight. A TSV line cannot be mangled that way.
  printf '%s\t%s\n' "$id" "$outcome" >> "$OUTCOMES"
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
# Totals and per-canary metadata are read from the corpus and the outcomes
# file. Nothing is carried across the loop in a shell variable, so the report
# cannot contradict the run that produced it.
python3 -c "
import json, sys, tomllib

corpus = {c['id']: c for c in tomllib.load(open(sys.argv[1], 'rb'))['canary']}
outcomes = {}
for line in open(sys.argv[2]):
    line = line.strip()
    if line:
        cid, outcome = line.split('\t', 1)
        outcomes[cid] = outcome

missing = [cid for cid in corpus if cid not in outcomes]
if missing:
    sys.exit('canaries declared in the corpus never ran: ' + ', '.join(sorted(missing)))

canaries = []
for cid, outcome in outcomes.items():
    c = corpus[cid]
    canaries.append({
        'id': cid, 'category': c['category'], 'invariant': c['invariant'],
        'split': c.get('split', 'gate'), 'probe': c.get('probe', '--lib'),
        'outcome': outcome, 'expect_kill': c.get('expect_kill', True),
        'known_surviving': c.get('known_surviving', False),
        'equivalent': c.get('equivalent', False),
    })

holdout = [c for c in canaries if c['split'] == 'holdout' and not c['equivalent']]
print(json.dumps({
    'schema': 1,
    'canaries': canaries,
    'totals': {
        'total': len(canaries),
        'killed': sum(1 for c in canaries if c['outcome'] == 'killed'),
        'survived': sum(1 for c in canaries if c['outcome'] == 'survived'),
        'holdout_total': len(holdout),
        'holdout_killed': sum(1 for c in holdout if c['outcome'] == 'killed'),
    },
}, indent=2))
" "$CANARIES" "$OUTCOMES" > "$RESULTS" || exit 8

python3 -c "
import json,sys
t = json.load(open(sys.argv[1]))['totals']
print(f\"  canaries: {t['total']}   killed: {t['killed']}   survived: {t['survived']}\")
if t['holdout_total'] == 0:
    print('  no held-out canaries — the gate would be measuring nothing')
    sys.exit(7)
print(f\"  HELD-OUT kill rate: {t['holdout_killed']/t['holdout_total']:.2f} ({t['holdout_killed']}/{t['holdout_total']})\")
" "$RESULTS" || exit 7
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
