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
# The gate rebuilds the crate once per canary. Linking a 66k-LOC test binary is
# the single largest allocation in the whole project and is what actually
# exhausts memory -- not the test run. So the gate defaults to a single job:
# two concurrent rustc/linker processes on a 7.5 GiB host is how the box died
# the first time this ran unattended.
#
# Override only on a machine with headroom: NIKI_CANARY_JOBS=2 ./scripts/canary-gate.sh
export CARGO_BUILD_JOBS="${NIKI_CANARY_JOBS:-1}"
MIN_FREE_MB="${NIKI_MIN_FREE_MB:-2600}"
# Free memory required before a canary starts, and how long to wait for it.
RECOVER_MB="${NIKI_CANARY_RECOVER_MB:-2600}"
RECOVER_WAIT_S="${NIKI_CANARY_RECOVER_WAIT:-180}"
# Hard ceiling on a single cargo invocation, so a runaway build dies with a
# clear message instead of taking the whole machine down with it.
CARGO_MEMORY_LIMIT_MB="${NIKI_CANARY_MEM_LIMIT:-5000}"

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

# Free RAM in MiB, portable across Linux and macOS.
free_mb() {
  if command -v free >/dev/null 2>&1; then
    free -m | awk '/^Mem:/ {print $7}'
  else
    vm_stat | awk '/Pages free/ {gsub("\\.", "", $3); print $3/256}'
  fi
}

# Block until free memory clears the threshold, or give up. Waiting is the
# right response: cargo's page cache holds a large share of "used" memory
# after a build and releases it within seconds, so refusing outright would
# fail on a machine that is merely still settling.
wait_for_memory() {
  local need="$1" waited=0
  while :; do
    local avail
    avail="$(free_mb)"
    if [ "${avail:-0}" -ge "$need" ]; then
      return 0
    fi
    if [ "$waited" -ge "$RECOVER_WAIT_S" ]; then
      red "only ${avail} MiB free after ${waited}s (need ${need} MiB)"
      return 1
    fi
    [ "$waited" -eq 0 ] && info "waiting for memory: ${avail} MiB free, need ${need} MiB"
    sleep 5
    waited=$((waited + 5))
  done
}

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
# Restore ONLY the files this script mutates. An earlier trap did
# `git checkout -- .`, which is broader than the script's footprint: it also
# reverted canaries/results.json to whatever was committed, silently
# overwriting the report the run had just written. The gate was destroying
# its own output, and the console showed 8 canaries while the file on disk
# said 0.
#
# The canary target files are read from the corpus itself, so the restore set
# cannot drift from the set the script actually touched.
CANARY_FILES="$(python3 -c "
import sys, tomllib
d = tomllib.load(open(sys.argv[1], 'rb'))
seen = []
for c in d['canary']:
    if c['file'] not in seen:
        seen.append(c['file'])
print(' '.join(seen))
" "$CANARIES" 2>/dev/null)"

restore() {
  # shellcheck disable=SC2086
  [ -n "$CANARY_FILES" ] && git checkout -- $CANARY_FILES 2>/dev/null
  return 0
}

trap 'rm -f "$TMP_OUT" "$OUTCOMES"; restore' EXIT

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
    restore
    exit 5
  fi

  # Every canary rebuilds. Check memory before each one rather than only at
  # the start: the first canary is cheap, the fifth is where the page cache
  # from four prior builds is still resident.
  if ! wait_for_memory "$RECOVER_MB"; then
    red "aborting before $id rather than risking the machine"
    restore
    exit 4
  fi
  info "  memory before build: $(free_mb) MiB free"

  # shellcheck disable=SC2086
  if timeout 900 cargo test $probe -j 1 -- --test-threads=1 > /tmp/canary-run.log 2>&1; then
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
  info "  memory after run: $(free_mb) MiB free"
  restore
done < <(python3 -c "
import json,sys
for c in json.load(open(sys.argv[1])):
    print('\t'.join([c['id'], c['file'], c['patch'], c['replace_with'], c['probe']]))
" "$TMP_OUT")

# ── Report ────────────────────────────────────────────────────────────────
END_TREE="$(git status --porcelain; git rev-parse HEAD)"
if [ "$START_TREE" != "$END_TREE" ]; then
  red "the working tree was not restored — refusing to report a result"
  restore
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
