#!/usr/bin/env bash
# Run the whole test suite, and keep this box alive while doing it.
#
# With `cargo-nextest` installed (the fast path) this is one command: the full
# 1503-test suite in ~64s warm, with the heavy and heap binaries serialised by
# `.config/nextest.toml` and everything else parallel.
#
# Without it, the loop below runs the same tests one binary at a time. Slower,
# and it is the fallback rather than the default because of what this box is:
# ~7.5 GiB shared with another agent session, where a `cargo test` link running
# alongside a live-model sweep holding a 1.9 GB model is what crashed it twice.
# So the fallback watches free memory and *refuses to start* a link below the
# floor, and a skip is reported as a failure rather than as a pass.
#
#   ./scripts/verify-suite.sh            # everything
#   ./scripts/verify-suite.sh lib        # just the library tests
#   ./scripts/verify-suite.sh diff_scope # one integration binary
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# Free memory to require before starting a new test binary. A debug link of this
# crate peaks around 1.5-2 GB; the floor leaves headroom for the other session.
MIN_FREE_MB="${NIKI_MIN_FREE_MB:-2600}"
JOBS="${NIKI_TEST_JOBS:-2}"

mem_free_mb() { free -m | awk '/^Mem:/ {print $7}'; }

check_floor() {
    local what="$1" free
    free="$(mem_free_mb)"
    if [ "$free" -lt "$MIN_FREE_MB" ]; then
        echo "STOP: only ${free} MiB free, floor is ${MIN_FREE_MB} MiB — not starting $what" >&2
        return 1
    fi
    return 0
}

if [ $# -gt 0 ]; then
    targets=("$@")
else
    # Every integration target in the manifest, plus the library.
    targets=(lib)
    for f in tests/*.rs; do
        [ -e "$f" ] || continue
        targets+=("$(basename "$f" .rs)")
    done
fi

# Fast path: nextest, when it is installed.
#
# Measured on this box: the full 1503-test suite in 64s warm, one command,
# all binaries. The loop below runs the same tests serially and takes several
# minutes, and it exists for the case where nextest is absent — which is every
# fresh checkout until `cargo install cargo-nextest --locked`.
#
# nextest is not just parallelism. It runs process-per-test, so a test that
# wedges is killed at the configured timeout instead of hanging the run — which
# is how a real infinite loop I introduced was found: a test binary that had
# been going for ten minutes and should take 1.4 seconds.
if [ $# -eq 0 ] && command -v cargo-nextest >/dev/null 2>&1; then
    exec cargo nextest run -j "${NIKI_TEST_JOBS:-2}"
fi

failures=()
skipped=()
for t in "${targets[@]}"; do
    if [ "$t" = "lib" ]; then
        cmd=(cargo test -j "$JOBS" --lib -- --test-threads=1)
    else
        cmd=(cargo test -j "$JOBS" --test "$t" -- --test-threads=1)
    fi

    check_floor "$t" || { skipped+=("$t"); continue; }

    out="$("${cmd[@]}" 2>&1)"
    status=$?
    line="$(printf '%s\n' "$out" | grep -E '^test result' | head -1)"
    free_after="$(mem_free_mb)"
    printf '%-26s %-52s (free %s MiB)\n' "$t" "${line:-NO RESULT}" "$free_after"
    if [ $status -ne 0 ] || [ -z "$line" ]; then
        failures+=("$t")
        printf '%s\n' "$out" | grep -E '^---- |panicked at' | head -8
    fi
done

echo
# A skip is a failure. The floor exists so a link does not take the box down,
# but a suite that skipped everything and printed "all passed" is worse than
# one that crashed: it is a green result for work that never ran.
if [ "${#skipped[@]}" -gt 0 ]; then
    echo "SKIPPED for low memory: ${skipped[*]}"
    echo "free memory was below ${MIN_FREE_MB} MiB. Free some and re-run; this is NOT a pass."
    exit 2
fi
if [ "${#failures[@]}" -gt 0 ]; then
    echo "FAILED: ${failures[*]}"
    exit 1
fi
echo "all ${#targets[@]} test binaries passed"
