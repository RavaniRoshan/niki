#!/usr/bin/env bash
# Git/worktree integrity assertions for the product acceptance suite.
#
# Extracted so `run_scenarios_selftest.sh` can run exactly these checks against
# deliberately broken repositories and assert that each one goes red. A gate
# that has only ever been green is not known to be a check — this file exists so
# that claim is testable rather than aspirational.
#
# Contract
#   assert_git_integrity <repo_dir> <base_ref> <start_branch> <start_branch_sha>
#
# `base_ref`, `start_branch` and `start_branch_sha` must have been captured
# BEFORE the run under test. NIKI checks out the branch it creates, so reading
# `HEAD` afterwards yields the new branch's own commit and every "did anything
# change?" comparison compares a commit to itself.
#
# Prints a one-line summary on success. On failure, prints
# `INTEGRITY FAILURE: <reason>` to stderr and returns non-zero. It does not
# exit — the caller decides whether a failure is fatal.

assert_git_integrity() {
    local repo="$1" base_ref="$2" start_branch="$3" start_sha="$4"
    local fail reason branches count niki_branch ahead diff_files start_now

    fail() {
        echo "INTEGRITY FAILURE: $1" >&2
        return 1
    }

    # `git branch --list` prefixes the checked-out branch with `*`. NIKI checks
    # out the branch it creates, so that marker is always present; a refname of
    # `*niki/abc` is not resolvable and every comparison below would fail open.
    # `--format` emits the refname alone.
    branches="$(git -C "$repo" for-each-ref --format='%(refname:short)' refs/heads/niki/)"
    count="$(printf '%s\n' "$branches" | grep -c . || true)"
    if [ "${count:-0}" -lt 1 ]; then
        fail 'no niki/<id> branch was created'
        return 1
    fi
    if [ "$count" -gt 1 ]; then
        fail "expected exactly one niki/<id> branch, found $count"
        return 1
    fi
    niki_branch="$(printf '%s\n' "$branches" | head -1)"

    # The branch must carry a commit of its own. An empty branch means the
    # pipeline reported success while writing nothing.
    ahead="$(git -C "$repo" rev-list --count "$base_ref..$niki_branch" 2>/dev/null || echo 0)"
    if [ "${ahead:-0}" -lt 1 ]; then
        fail "$niki_branch has no commit beyond the starting point"
        return 1
    fi

    # The starting commit must remain an ancestor. A rebased or orphaned history
    # is the failure this check exists to catch, and it is invisible to a
    # "did it exit 0" test.
    if ! git -C "$repo" merge-base --is-ancestor "$base_ref" "$niki_branch"; then
        fail "$niki_branch does not descend from the starting commit (history was rewritten)"
        return 1
    fi

    # A commit that changes nothing is a green run that delivered nothing.
    diff_files="$(git -C "$repo" diff --name-only "$base_ref..$niki_branch" | grep -c . || true)"
    if [ "${diff_files:-0}" -lt 1 ]; then
        fail "$niki_branch carries a commit but changes no files"
        return 1
    fi

    # The starting branch must not have been repointed. "Committed branches are
    # never rewritten" is a headline promise; it has to be measured against the
    # SHA recorded before the run, not against itself.
    start_now="$(git -C "$repo" rev-parse "$start_branch" 2>/dev/null || echo missing)"
    if [ "$start_now" != "$start_sha" ]; then
        fail "the starting branch $start_branch was moved"
        return 1
    fi

    reason="branch $niki_branch, $ahead commit(s), $diff_files file(s) changed"
    echo "INTEGRITY OK: $reason"
    return 0
}
