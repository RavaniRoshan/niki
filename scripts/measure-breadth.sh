#!/usr/bin/env bash
# Does NIKI handle a VARIETY of engineering work, or one shape of task?
#
#   ./scripts/measure-breadth.sh [runs-per-task]
#
# A harness measured only on "add a function that sums a slice" proves almost
# nothing. Codex and Claude Code are general engineering agents: bug fixes,
# refactors, tests, docs, migrations, build breakage. If NIKI only does the
# first thing well, it is a demo, not a product — and the difference is invisible
# until someone asks for something else.
#
# Each task below is seeded into the same throwaway repo, so every run is a real
# run: real agents, real diffs, real branch. What differs is the *kind* of work.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"
RUNS="${1:-1}"
BIN="${NIKI_BIN:-$REPO_ROOT/target/debug/niki}"
[ -x "$BIN" ] || { echo "no binary at $BIN — cargo build --bin niki" >&2; exit 1; }
TIMEOUT="${NIKI_MEASURE_TIMEOUT:-600}"
PROJECT="${NIKI_BREADTH_PROJECT:-/tmp/niki-breadth}"

# name | file to seed | seed content | the task
TASKS=(
"add-function|src/lib.rs|pub fn add(a: i32, b: i32) -> i32 { a + b }|Add a public function named total that sums a slice of integers, and document it."
"fix-bug|src/broken.rs|pub fn first(xs: &[i32]) -> i32 { xs[0] }
|src/broken.rs calls first on an empty slice and panics. Make it return 0 for an empty slice instead, and handle it in one place."
"refactor|src/dupe.rs|pub fn norm(x: f64) -> f64 { let y = x * x; y.sqrt() }
pub fn length(x: f64, y: f64) -> f64 { let d = (x * x + y * y).sqrt(); d }
|These two functions compute the same thing. Give them one shared implementation and keep both public names working."
"add-test|src/calc.rs|pub fn double(x: i32) -> i32 { x * 2 }
|There are no tests. Add a test module covering double, including zero and negatives."
"docs|src/calc.rs|pub fn double(x: i32) -> i32 { x * 2 }
|Write a README.md for this crate: what it does, how to use it, with a worked example."
)

seed() {
    local file="$1" content="$2"
    mkdir -p "$(dirname "$PROJECT/$file")"
    printf '%s\n' "$content" >"$PROJECT/$file"
}

pass=0; fail=0
declare -a notes=()
echo "binary: $BIN   runs/task: $RUNS   timeout: ${TIMEOUT}s"
echo
for spec in "${TASKS[@]}"; do
    IFS='|' read -r name file content task <<<"$spec"
    for r in $(seq 1 "$RUNS"); do
        rm -rf "$PROJECT"; mkdir -p "$PROJECT"
        ( cd "$PROJECT" && git init -q && git config user.email b@niki.local && git config user.name breadth )
        seed "$file" "$content"
        ( cd "$PROJECT" && git add -A && git commit -q -m "seed: $name" )
        printf '%-12s run %s: ' "$name" "$r"
        if timeout "$TIMEOUT" "$BIN" run --project "$PROJECT" --backend worktree --bare "$task" \
            >"/tmp/niki-breadth-$name-$r.log" 2>&1; then
            pass=$((pass + 1)); printf 'PASS %s\n' "$(grep -oE 'Latency: [0-9.]+s' "/tmp/niki-breadth-$name-$r.log" | tail -1)"
        else
            fail=$((fail + 1))
            reason="$(grep -oE '(Error: .{0,64})' "/tmp/niki-breadth-$name-$r.log" | head -1)"
            [ -z "$reason" ] && reason="(no error captured — see /tmp/niki-breadth-$name-$r.log)"
            printf 'FAIL %s\n' "$reason"
            notes+=("$name/$r: $reason")
        fi
    done
done

echo
echo "---"
printf 'breadth: %d/%d passed across %d task types\n' "$pass" "$((pass + fail))" "${#TASKS[@]}"
if [ "${#notes[@]}" -gt 0 ]; then printf '  %s\n' "${notes[@]}"; fi
[ "$fail" -eq 0 ]
