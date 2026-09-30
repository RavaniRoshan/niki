#!/usr/bin/env bash
# Is every required CI context green on the latest run?
#
# `scripts/check-required-contexts.py` proves the *offline* property — that the
# required list has no orphans — and can prove nothing about whether anything is
# currently green. This asks GitHub.
set -uo pipefail
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)" || exit 1

run_id=$(gh run list --workflow=CI --limit 1 --json databaseId --jq '.[0].databaseId' 2>/dev/null)
[ -n "$run_id" ] || { echo "no CI run found for this branch"; exit 1; }
echo "latest CI run: $run_id"

# Every context the repo declares as required must be present AND successful.
required=$(python3 scripts/check-required-contexts.py --list 2>/dev/null)
status=$(gh run view "$run_id" --json jobs -q '.jobs[] | "\(.conclusion)\t\(.name)"')
bad=0
while IFS= read -r ctx; do
  [ -z "$ctx" ] && continue
  line=$(grep -F "$ctx" <<<"$status" || true)
  if [ -z "$line" ]; then
    echo "MISSING  $ctx"; bad=1; continue
  fi
  conclusion=${line%%$'\t'*}
  if [ "$conclusion" = "success" ]; then
    echo "ok       $ctx"
  else
    echo "$conclusion  $ctx"; bad=1
  fi
done <<< "$required"
exit $bad
