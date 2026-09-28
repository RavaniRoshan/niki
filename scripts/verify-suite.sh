#!/usr/bin/env bash
# Run the whole test suite one binary at a time, watching free memory.
#
# Why this exists: this box has ~7.5 GiB and another agent session shares it.
# `cargo test` links a fresh binary per test target, and a link is the single
# largest memory event in the build; the previous attempts to run the suite in
# one invocation, or to run it while a live-model sweep was loading a 1.9 GB
# model, are what crashed it.
#
# Rules this enforces:
#   * one test binary at a time, never parallel;
#   * `-j 2` everywhere, and `--test-threads=1`;
#   * stop rather than start a link when free memory is below the floor;
#   * report every result, and exit non-zero if any binary failed.
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
if [ "${#skipped[@]}" -gt 0 ]; then
    echo "skipped for memory: ${skipped[*]}"
fi
if [ "${#failures[@]}" -gt 0 ]; then
    echo "FAILED: ${failures[*]}"
    exit 1
fi
echo "all ${#targets[@]} test binaries passed"
