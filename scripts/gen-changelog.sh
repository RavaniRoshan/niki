#!/usr/bin/env bash
# Generate a conventional-commit changelog section for a release.
#
# Usage:
#   ./scripts/gen-changelog.sh                 # from latest tag to HEAD
#   ./scripts/gen-changelog.sh v0.9.0 v0.10.0   # explicit range
#   ./scripts/gen-changelog.sh --check         # verify git log parses as conventional commits

set -euo pipefail

FROM_TAG="${1:-}"
TO_TAG="${2:-HEAD}"

if [ "$FROM_TAG" = "--check" ]; then
    echo "Checking last 30 commits for conventional commit format..."
    # Verify commit messages follow conventional format: type(scope)?: description
    BAD=0
    while IFS= read -r line; do
        if ! echo "$line" | grep -qE '^(feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert)(\([a-zA-Z0-9_-]+\))?: .+'; then
            # Ignore merge commits
            if ! echo "$line" | grep -qE '^Merge '; then
                echo "Non-conventional commit: $line"
                BAD=$((BAD + 1))
            fi
        fi
    done < <(git log -n 30 --format='%s')
    if [ "$BAD" -gt 0 ]; then
        echo "Found $BAD commits without standard conventional prefix (informational)."
    else
        echo "All checked commits follow conventional format."
    fi
    exit 0
fi

if [ -z "$FROM_TAG" ]; then
    FROM_TAG="$(git describe --tags --abbrev=0 2>/dev/null || echo "")"
    if [ -z "$FROM_TAG" ]; then
        FROM_TAG="$(git rev-list --max-parents=0 HEAD)"
    fi
fi

echo "Generating changelog for range: ${FROM_TAG}..${TO_TAG}"
echo ""

COMMITS="$(git log "${FROM_TAG}..${TO_TAG}" --format='%h|%s' 2>/dev/null || true)"

if [ -z "$COMMITS" ]; then
    echo "No commits found in range ${FROM_TAG}..${TO_TAG}."
    exit 0
fi

FEATS=()
FIXES=()
PERFS=()
DOCS=()
OTHERS=()

while IFS='|' read -r hash msg; do
    case "$msg" in
        feat*:) FEATS+=("- ${msg} ([${hash}](https://github.com/RavaniRoshan/niki/commit/${hash}))") ;;
        feat\(*\):*) FEATS+=("- ${msg} ([${hash}](https://github.com/RavaniRoshan/niki/commit/${hash}))") ;;
        fix*:) FIXES+=("- ${msg} ([${hash}](https://github.com/RavaniRoshan/niki/commit/${hash}))") ;;
        fix\(*\):*) FIXES+=("- ${msg} ([${hash}](https://github.com/RavaniRoshan/niki/commit/${hash}))") ;;
        perf*:) PERFS+=("- ${msg} ([${hash}](https://github.com/RavaniRoshan/niki/commit/${hash}))") ;;
        perf\(*\):*) PERFS+=("- ${msg} ([${hash}](https://github.com/RavaniRoshan/niki/commit/${hash}))") ;;
        docs*:) DOCS+=("- ${msg} ([${hash}](https://github.com/RavaniRoshan/niki/commit/${hash}))") ;;
        docs\(*\):*) DOCS+=("- ${msg} ([${hash}](https://github.com/RavaniRoshan/niki/commit/${hash}))") ;;
        *) OTHERS+=("- ${msg} ([${hash}](https://github.com/RavaniRoshan/niki/commit/${hash}))") ;;
    esac
done <<< "$COMMITS"

if [ ${#FEATS[@]} -gt 0 ]; then
    echo "### Features"
    printf '%s\n' "${FEATS[@]}"
    echo ""
fi

if [ ${#FIXES[@]} -gt 0 ]; then
    echo "### Bug Fixes"
    printf '%s\n' "${FIXES[@]}"
    echo ""
fi

if [ ${#PERFS[@]} -gt 0 ]; then
    echo "### Performance"
    printf '%s\n' "${PERFS[@]}"
    echo ""
fi

if [ ${#DOCS[@]} -gt 0 ]; then
    echo "### Documentation"
    printf '%s\n' "${DOCS[@]}"
    echo ""
fi

if [ ${#OTHERS[@]} -gt 0 ]; then
    echo "### Maintenance & Other"
    printf '%s\n' "${OTHERS[@]}"
    echo ""
fi
