#!/usr/bin/env bash
# Selftest for the product acceptance suite's git/worktree integrity gate.
#
# The gate exists because `run_scenarios.sh` used to print
# `pass_test 'Git/worktree integrity'` with no assertion behind it. Adding
# assertions is only half the job: an assertion that has never rejected
# anything is indistinguishable from a rubber stamp with better manners. This
# script builds a repository for each way the product could be broken, runs the
# *real* assertions from lib_integrity.sh against it, and asserts that each one
# fails.
#
# It is the same shape as scripts/mega-e2e-selftest.sh, applied to the
# acceptance suite. A green run here means the gate has been shown to be able to
# go red on all five failure modes below.
#
# No LLM, no network, no container runtime. Runs in a few seconds.
#
#   bash tests/product/runners/run_scenarios_selftest.sh

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib_integrity.sh
. "$HERE/lib_integrity.sh"

RED=$'\033[0;31m'
GREEN=$'\033[0;32m'
NC=$'\033[0m'

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

failures=0
checks=0

ok()   { checks=$((checks+1)); echo -e "[${GREEN}PASS${NC}] $1"; }
bad()  { checks=$((checks+1)); failures=$((failures+1)); echo -e "[${RED}FAIL${NC}] $1"; }

# Build a repository in the "product worked" shape: one starting commit on
# `master`, and one `niki/<id>` branch carrying a real commit that changes a
# real file and descends from the start.
#
# HEAD is left on `master`, not on the niki branch, so a fixture can delete or
# force-move that branch — git refuses both on the checked-out branch, and a
# fixture that cannot build its broken state is a fixture that silently stops
# testing anything. The control case checks the niki branch out again, because
# that is what the product actually leaves behind and the `*` current-branch
# marker in `git branch --list` output is what broke the first version of
# these assertions.
#
# Echoes "<dir> <base_ref> <start_branch> <start_sha>" for the caller.
make_good_repo() {
    local dir="$WORK/$1"
    mkdir -p "$dir"
    git -C "$dir" init -q -b master
    git -C "$dir" config user.email t@t.dev
    git -C "$dir" config user.name "T"
    echo 'console.log("hello");' > "$dir/index.js"
    git -C "$dir" add -A
    git -C "$dir" commit -qm initial
    local base; base="$(git -C "$dir" rev-parse HEAD)"

    # The change the product would have made, on its own branch.
    git -C "$dir" checkout -q -b niki/abc123
    printf 'exports.health = () => ({ status: "ok" });\n' > "$dir/index.js"
    git -C "$dir" add -A
    git -C "$dir" commit -qm "NIKI implementation for task abc123"
    git -C "$dir" checkout -q master
    echo "$dir $base master $base"
}

# expect_fail <name> <repo> <base> <branch> <sha> [expected substring]
expect_fail() {
    local name="$1" repo="$2" base="$3" branch="$4" sha="$5" want="${6:-}"
    local out status
    out="$(assert_git_integrity "$repo" "$base" "$branch" "$sha" 2>&1)"
    status=$?
    if [ "$status" -eq 0 ]; then
        bad "$name — assertion returned success on a broken repository"
        return
    fi
    if [ -n "$want" ] && ! printf '%s' "$out" | grep -qi -- "$want"; then
        bad "$name — failed, but not for the expected reason: $out"
        return
    fi
    ok "$name — rejected: ${out#INTEGRITY FAILURE: }"
}

echo '===================================='
echo 'PRODUCT ACCEPTANCE SUITE — INTEGRITY GATE SELFTEST'
echo '===================================='
echo

# ── 0 · The happy path must still pass, or every failure below is vacuous ──
read -r G_DIR G_BASE G_BRANCH G_SHA <<<"$(make_good_repo good)"
git -C "$G_DIR" checkout -q niki/abc123   # the product leaves this checked out
if OUT="$(assert_git_integrity "$G_DIR" "$G_BASE" "$G_BRANCH" "$G_SHA" 2>&1)"; then
    ok "control — a correct repository is accepted (${OUT#INTEGRITY OK: })"
else
    bad "control — a correct repository was REJECTED: $OUT"
fi

# ── 1 · No branch was created ────────────────────────────────────────────
read -r R_DIR R_BASE R_BRANCH R_SHA <<<"$(make_good_repo no_branch)"
git -C "$R_DIR" branch -q -D niki/abc123
expect_fail 'no branch created' "$R_DIR" "$R_BASE" "$R_BRANCH" "$R_SHA" 'no niki/<id> branch'

# ── 2 · Two branches were created ────────────────────────────────────────
read -r R_DIR R_BASE R_BRANCH R_SHA <<<"$(make_good_repo two_branches)"
git -C "$R_DIR" branch niki/def456 "$R_BASE"
expect_fail 'more than one branch' "$R_DIR" "$R_BASE" "$R_BRANCH" "$R_SHA" 'exactly one'

# ── 3 · Branch exists but carries no commit ──────────────────────────────
read -r R_DIR R_BASE R_BRANCH R_SHA <<<"$(make_good_repo no_commit)"
git -C "$R_DIR" branch -q -f niki/abc123 "$R_BASE"
expect_fail 'branch has no commit' "$R_DIR" "$R_BASE" "$R_BRANCH" "$R_SHA" 'no commit beyond'

# ── 4 · History was rewritten: the start commit is not an ancestor ────────
read -r R_DIR R_BASE R_BRANCH R_SHA <<<"$(make_good_repo rewritten)"
git -C "$R_DIR" branch -q -D niki/abc123
# A brand-new root commit on the niki branch; the original start is unreachable.
git -C "$R_DIR" checkout -q --orphan niki/abc123
rm -f "$R_DIR/index.js"
echo 'orphaned' > "$R_DIR/index.js"
git -C "$R_DIR" add -A
git -C "$R_DIR" commit -qm "rewritten root"
expect_fail 'history rewritten' "$R_DIR" "$R_BASE" "$R_BRANCH" "$R_SHA" 'history was rewritten'

# ── 5 · Commit exists but changes nothing ─────────────────────────────────
read -r R_DIR R_BASE R_BRANCH R_SHA <<<"$(make_good_repo empty_diff)"
git -C "$R_DIR" branch -q -f niki/abc123 "$R_BASE"
git -C "$R_DIR" checkout -q niki/abc123
git -C "$R_DIR" commit -q --allow-empty -m "NIKI implementation for task abc123"
expect_fail 'commit changes no files' "$R_DIR" "$R_BASE" "$R_BRANCH" "$R_SHA" 'changes no files'

# ── 6 · The starting branch was repointed ────────────────────────────────
read -r R_DIR R_BASE R_BRANCH R_SHA <<<"$(make_good_repo moved_start)"
git -C "$R_DIR" checkout -q master
git -C "$R_DIR" commit -q --allow-empty -m "an unrelated local commit"
expect_fail 'starting branch moved' "$R_DIR" "$R_BASE" "$R_BRANCH" "$R_SHA" 'was moved'

echo
if [ "$failures" -gt 0 ]; then
    echo -e "[${RED}$failures of $checks checks failed${NC}]"
    echo "The integrity gate cannot be trusted: it accepted a broken repository."
    exit 1
fi
echo -e "[${GREEN}All $checks checks passed${NC}]"
echo "The integrity gate rejected every broken repository it was shown, and"
echo "accepted the correct one. It is a check, not a rubber stamp."
