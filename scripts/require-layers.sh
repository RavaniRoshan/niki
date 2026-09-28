#!/usr/bin/env bash
# Fail unless every named layer was recorded as having actually run.
#
#   printf '%s\n' 'CLI smoke' 'TUI PTY smoke' > /tmp/passed
#   ./scripts/require-layers.sh /tmp/passed 'CLI smoke' 'TUI PTY smoke'
#
# This exists as its own script so it can be *run* by a test. Its previous form
# lived inline at the bottom of product-verify.sh, where the only thing a test
# could check was that the word `MUST_HAVE_RUN` appeared in the file — which
# passed unchanged when the recording line was deleted, taking the whole guard
# with it. A check that cannot be executed is a comment.
#
# The failure this guards: this repo has shipped a CI job that went green while
# executing nothing. A missing runner script took the `else` branch, printed
# SKIP, and the job passed. "Nothing ran" and "everything passed" have to be
# distinguishable, and that distinction is this script.
set -uo pipefail

ledger="${1:?usage: require-layers.sh <ledger-file> <layer> [layer ...]}"
shift
required=$#

if [ ! -r "$ledger" ]; then
    echo "VACUOUS: ledger '$ledger' is missing or unreadable — nothing can be shown to have run" >&2
    exit 1
fi

missing=0
for layer in "$@"; do
    if ! grep -qxF -- "$layer" "$ledger"; then
        echo "VACUOUS: '$layer' did not run — the suite proved nothing without it" >&2
        missing=1
    fi
done

if [ "$missing" -ne 0 ]; then
    exit 1
fi

echo "all $required required layers ran"
