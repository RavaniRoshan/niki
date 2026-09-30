#!/usr/bin/env bash
# Selftest for the documentation claims gate.
#
# `tests/claims.rs` used to cover three filenames, and `docs/launch-audit.md`
# then sat at the top of this repository for six weeks and five releases
# claiming version 0.4.0, "~30,600 lines", "Hooks: Not implemented" and "No
# `niki init`" — none of which was still true, and it was cited from the README
# as the methodology behind the project's honesty.
#
# It rotted because nothing read it. Widening the gate fixes that once; this
# script is what stops the fix from being undone, or from having been wrong in
# the first place. It injects each class of defect the gate is supposed to
# catch, and requires the gate to fail every time.
#
# It works on temporary copies: the repository is never modified, and if the
# script is interrupted the working tree is left exactly as it was found.
#
#   bash tests/docs_consistency_selftest.sh
#
# Exits non-zero if any injected defect goes undetected.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 1

RED=$'\033[0;31m'
GREEN=$'\033[0;32m'
NC=$'\033[0m'

failures=0
checks=0
ok()  { checks=$((checks+1)); echo -e "[${GREEN}PASS${NC}] $1"; }
bad() { checks=$((checks+1)); failures=$((failures+1)); echo -e "[${RED}FAIL${NC}] $1"; }

WORK="$(mktemp -d)"
BACKUP="$WORK/backup"
mkdir -p "$BACKUP"
# A mirror of the tree, so a failed restore cannot leave a half-edited file.
restore() {
    for f in "${PROTECTED[@]}"; do
        [ -f "$BACKUP/$(basename "$f")" ] && cp "$BACKUP/$(basename "$f")" "$f"
    done
}
PROTECTED=()

protect() {
    for f in "$@"; do
        [ -f "$f" ] || continue
        cp "$f" "$BACKUP/$(basename "$f")"
        PROTECTED+=("$f")
    done
}

trap 'restore; rm -rf "$WORK"' EXIT

echo '===================================='
echo 'DOCS CLAIMS GATE — SELFTEST'
echo '===================================='
echo

# Run one of the gate's test binaries and report whether it failed.
# `$1` is the test binary, the rest are the files already mutated.
gate_fails() {
    local bin="$1"
    CARGO_BUILD_JOBS=2 timeout 900 cargo test --test "$bin" -j 2 -- --test-threads=1 \
        >"$WORK/out.log" 2>&1
    local status=$?
    if [ "$status" -eq 0 ]; then
        return 1   # the gate passed a tree it should have rejected
    fi
    # A compile error is not a detection. The gate must run and fail.
    # cargo prints `error: test failed` when a test fails, which is exactly the
    # outcome wanted here. A compile failure is a different thing entirely and
    # must not be mistaken for a detection.
    if grep -qE '^error\[E[0-9]+' "$WORK/out.log" || grep -q 'could not compile' "$WORK/out.log"; then
        echo "    (the gate failed to compile, which is not a detection)"
        return 2
    fi
    return 0
}

# ── 1 · A false guarantee on a documentation page ────────────────────────
# The original sin, and the one the README quotes back at the project.
protect docs/content/01-overview/03-quickstart.mdx
cat >> docs/content/01-overview/03-quickstart.mdx <<'EOF'

## Note

Your working tree is never modified.
EOF
if gate_fails claims; then
    ok "a false guarantee in a docs page is rejected"
else
    bad "a false guarantee in docs/content/ passed the gate"
fi
restore

# ── 2 · A command that does not exist, in a docs page ───────────────────
protect docs/content/01-overview/03-quickstart.mdx
cat >> docs/content/01-overview/03-quickstart.mdx <<'EOF'

Run `niki frobnicate` to begin.
EOF
if gate_fails claims; then
    ok "a non-existent command in a docs page is rejected"
else
    bad "`niki frobnicate` in docs/content/ passed the gate"
fi
restore

# ── 3 · A sub-subcommand that does not exist ─────────────────────────────
protect docs/content/01-overview/03-quickstart.mdx
cat >> docs/content/01-overview/03-quickstart.mdx <<'EOF'

Run `niki config chek` to verify your setup.
EOF
if gate_fails claims; then
    ok "a mistyped sub-subcommand is rejected (the check is not first-word only)"
else
    bad "`niki config chek` passed the gate — the sub-subcommand check is dead"
fi
restore

# ── 4 · A dead relative link ─────────────────────────────────────────────
protect docs/content/01-overview/03-quickstart.mdx
cat >> docs/content/01-overview/03-quickstart.mdx <<'EOF'

See [the guide](./no-such-page.mdx) for more.
EOF
if gate_fails claims; then
    ok "a dead relative link in a docs page is rejected"
else
    bad "a dead relative link in docs/content/ passed the gate"
fi
restore

# ── 5 · The audit stating a version the crate does not have ──────────────
# The exact failure that ran for six weeks.
protect docs/launch-audit.md
python3 - <<PY
p = "docs/launch-audit.md"
s = open(p).read()
s = s.replace("> **Crate version:** $(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)",
              "> **Crate version:** 0.0.1", 1)
open(p, "w").write(s)
PY
if gate_fails docs_consistency; then
    ok "an audit pinned to a stale version is rejected"
else
    bad "docs/launch-audit.md pinned to a stale version passed the gate"
fi
restore

# ── 6 · The audit's counts disagreeing with the tree ─────────────────────
protect docs/launch-audit.md
python3 - <<'PY'
p = "docs/launch-audit.md"
s = open(p).read()
s = s.replace("| Baseline tools registered | 22",
              "| Baseline tools registered | 99", 1)
open(p, "w").write(s)
PY
if gate_fails docs_consistency; then
    ok "an audit count that disagrees with the tree is rejected"
else
    bad "a wrong count in docs/launch-audit.md passed the gate"
fi
restore

# ── 7 · A broken link in the repository root ─────────────────────────────
protect CONTRIBUTING.md
python3 - <<'PY'
p = "CONTRIBUTING.md"
s = open(p).read()
s = s.replace("](LICENSE)", "](../LICENSE)", 1)
open(p, "w").write(s)
PY
if gate_fails claims; then
    ok "the ../LICENSE link regression is caught"
else
    bad "a broken root-level link passed the gate"
fi
restore

# ── 8 · The control: an unmodified tree must pass ────────────────────────
# Without this, every "rejected" above could be a gate that rejects everything.
if gate_fails claims; then
    bad "the claims gate rejects the unmodified tree — it is failing for the wrong reason"
    gate_fails claims >/dev/null
    tail -25 "$WORK/out.log" | sed 's/^/    /'
else
    ok "control — the unmodified tree passes the claims gate"
fi
if gate_fails docs_consistency; then
    bad "the docs-consistency gate rejects the unmodified tree"
    tail -25 "$WORK/out.log" | sed 's/^/    /'
else
    ok "control — the unmodified tree passes the docs-consistency gate"
fi

echo
if [ "$failures" -gt 0 ]; then
    echo -e "[${RED}$failures of $checks checks failed${NC}]"
    echo "The documentation gate cannot be trusted: it accepted documentation that"
    echo "does not describe this product."
    exit 1
fi
echo -e "[${GREEN}All $checks checks passed${NC}]"
echo "The gate rejected every injected defect and accepted the real tree."
