#!/usr/bin/env bash
# Run the tests you actually need, on a box that cannot run the whole suite
# twice at once.
#
# Why this exists: `cargo test` on the full suite links every test binary at
# once, and linking is the largest single memory event in the build. On this
# machine — ~7.5 GiB shared with another agent session — that is what produces
# the OOM kills and freezes.
#
# The full suite is not gone and should not be run here: it runs in
# `.github/workflows/ci.yml` and gates the PR. This is for the inner loop, and
# `--all` is a last resort before pushing rather than a default.
#
#   scripts/test-fast.sh                  # unit tests only — the inner loop
#   scripts/test-fast.sh --filter review  # tests whose name contains "review"
#   scripts/test-fast.sh --lib <name>     # one library test, exactly
#   scripts/test-fast.sh --all            # LAST RESORT before pushing; CI is where
#                                        # the full suite belongs
#   scripts/test-fast.sh --mem            # how much room is actually left
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# Matches the concurrency the rest of the tooling assumes. A `-j` derived from
# the core count is what fills memory on a shared box.
JOBS="${NIKI_TEST_JOBS:-2}"
MIN_FREE_MB="${NIKI_MIN_FREE_MB:-1800}"

mem_free_mb() { free -m | awk '/^Mem:/ {print $7}'; }

usage() { sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'; }

need_nextest() {
    if ! command -v cargo-nextest >/dev/null 2>&1; then
        cat >&2 <<'MSG'
cargo-nextest is not installed. Without it this script cannot cap per-test
concurrency or serialise the heavy binaries, which is the whole point.

  cargo install cargo-nextest --locked
MSG
        exit 127
    fi
}

# Refuse to *start* a link when there is not room. Starting and dying is what
# takes the other session down with it.
guard() {
    local what="$1" free
    free="$(mem_free_mb)"
    if [ "$free" -lt "$MIN_FREE_MB" ]; then
        echo "Not starting $what: only ${free} MiB free, floor is ${MIN_FREE_MB} MiB." >&2
        echo "The full suite runs in CI; run the tests you touched, or free memory." >&2
        exit 2
    fi
}

[ $# -eq 0 ] && { need_nextest; guard "the unit tests"; exec cargo nextest run --lib -j "$JOBS"; }

case "$1" in
    --help|-h) usage; exit 0 ;;
    --mem)
        echo "free: $(mem_free_mb) MiB  (floor ${MIN_FREE_MB})"
        free -m | awk '/^Mem:/ {print "total: " $2 " MiB"}'
        exit 0
        ;;
    --filter)
        [ -z "${2:-}" ] && { echo "--filter needs a name" >&2; exit 2; }
        need_nextest; guard "tests matching '$2'"
        exec cargo nextest run -j "$JOBS" -E "test($2)"
        ;;
    --lib)
        [ -z "${2:-}" ] && { echo "--lib needs a test name" >&2; exit 2; }
        need_nextest; guard "library test '$2'"
        exec cargo nextest run --lib -j "$JOBS" -E "test($2)"
        ;;
    --all)
        need_nextest; guard "the full local suite"
        # The heavy and heap binaries serialise via .config/nextest.toml, so
        # this is the same suite CI runs, bounded to two threads.
        exec cargo nextest run -j "$JOBS"
        ;;
    -*)
        echo "unknown option: $1" >&2; usage >&2; exit 2
        ;;
    *)
        echo "unknown option: $1" >&2; usage >&2; exit 2
        ;;
esac
