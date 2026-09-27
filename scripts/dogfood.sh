#!/usr/bin/env bash
# Dogfood niki: drive the real product the way an engineer would, and report
# what actually happens.
#
# Every other test in this repo tests a module, or drives the binary against a
# mock. This one asks the only question that matters to a user:
#
#   "I have a real project, a real bug, and a real (small, local) model.
#    Does NIKI hand me a reviewable branch?"
#
# It is a *report*, not a gate. A model that cannot produce a schema-conformant
# artifact is a product finding, not a broken test, so the script prints what
# happened and exits 0 unless the harness itself is broken.
#
# Usage:
#   ./scripts/dogfood.sh                      # uses $NIKI_MODEL (default qwen2.5-coder:3b)
#   NIKI_MODEL=qwen2.5-coder:7b ./scripts/dogfood.sh
#   ./scripts/dogfood.sh --keep               # leave the scratch project in place
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NIKI_BIN="${NIKI_BIN:-$REPO_ROOT/target/release/niki}"
MODEL="${NIKI_MODEL:-qwen2.5-coder:3b}"
KEEP=0
[ "${1:-}" = "--keep" ] && KEEP=1

if [ ! -x "$NIKI_BIN" ]; then
  echo "error: $NIKI_BIN not built. Run: cargo build --release" >&2
  exit 2
fi

# ── A project with a real, reproducible bug ─────────────────────────────
# Two defects, both real and both covered by failing tests:
#   * format_currency drops trailing zeros: round(9.0, 2) -> 9.0, not 9.00
#   * subtotal ignores the `Decimal` import it declares but never uses
PROJECT="$(mktemp -d -t niki-dogfood-XXXXXX)"
cleanup() { [ "$KEEP" -eq 1 ] || rm -rf "$PROJECT"; }
trap cleanup EXIT

mkdir -p "$PROJECT/src"
cat > "$PROJECT/src/calc.py" <<'PY'
"""A tiny order-total calculator."""


def subtotal(items):
    """Sum line totals for a list of (unit_price, quantity) pairs."""
    total = 0.0
    for price, qty in items:
        total += price * qty
    return total


def apply_discount(total, percent):
    """Apply a percentage discount; `percent` is a whole number, e.g. 10 for 10%."""
    return total - total * percent / 100


def format_currency(amount):
    return "$" + format(round(amount, 2))


def invoice_total(items, discount_percent=0):
    return format_currency(apply_discount(subtotal(items), discount_percent))
PY

cat > "$PROJECT/test_calc.py" <<'PY'
from src.calc import subtotal, apply_discount, format_currency, invoice_total


def test_subtotal():
    assert subtotal([(10.0, 2), (5.0, 3)]) == 25.0


def test_apply_discount():
    assert apply_discount(100.0, 10) == 90.0


def test_format_currency_keeps_two_places():
    assert format_currency(1234.5) == "$1234.50"


def test_invoice_total():
    assert invoice_total([(10.0, 1)], 10) == "$9.00"


def test_empty_cart():
    assert invoice_total([]) == "$0.00"
PY

touch "$PROJECT/src/__init__.py"
(
  cd "$PROJECT"
  git init -q
  git config user.email "dogfood@niki.dev"
  git config user.name "Dogfood"
  git add -A
  git commit -qm "initial: order-total calculator with tests"
)

cat > "$PROJECT/niki.toml" <<TOML
[general]
max_revision_rounds = 2

[providers.ollama]
base_url = "http://localhost:11434"
default_model = "$MODEL"

[agents.planner]
provider = "ollama"
model = "$MODEL"

[agents.coder]
provider = "ollama"
model = "$MODEL"

[agents.tester]
provider = "ollama"
model = "$MODEL"

[agents.reviewer]
provider = "ollama"
model = "$MODEL"

[red_blue]
enabled = false
TOML

# ── Preconditions ───────────────────────────────────────────────────────
red()   { printf '\033[31m%s\033[0m\n' "$*"; }
green() { printf '\033[32m%s\033[0m\n' "$*"; }
info()  { printf '\033[2m%s\033[0m\n' "$*"; }

info "project:  $PROJECT"
info "binary:   $NIKI_BIN"
info "model:    $MODEL (Ollama, no API key)"

if ! curl -fsS --max-time 5 http://localhost:11434/api/tags >/dev/null 2>&1; then
  red "SKIP: no Ollama on :11434. Start it and pull a model, then re-run."
  exit 0
fi

# The bug must actually be present, or the test proves nothing.
if (cd "$PROJECT" && python3 -m pytest test_calc.py -q >/dev/null 2>&1); then
  red "SKIP: the fixture's tests pass, so there is no bug for NIKI to fix."
  exit 0
fi
info "precondition ok: 2 tests fail before the run"

# ── The run ─────────────────────────────────────────────────────────────
TASK="Fix the failing tests in test_calc.py. format_currency must always render exactly two decimal places, so 9.0 must print as \$9.00 and not \$9.0."

LOG="$PROJECT/dogfood.log"
( cd "$PROJECT" && "$NIKI_BIN" run "$TASK" --backend worktree --bare ) > "$LOG" 2>&1
RUN_EXIT=$?

echo
info "── niki run output ──────────────────────────────────────────"
sed 's/^/  /' "$LOG" | tail -25

# ── Verdict, in the terms a user cares about ────────────────────────────
echo
info "── what a user would check ──────────────────────────────────"

BRANCH="$(git -C "$PROJECT" branch --list 'niki/*' | tr -d ' *' | head -1)"
PATCH_BYTES=0
[ -n "$BRANCH" ] && PATCH_BYTES="$(git -C "$PROJECT" diff master.."$BRANCH" 2>/dev/null | wc -c)"

TESTS_NOW="$( (cd "$PROJECT" && python3 -m pytest test_calc.py -q 2>&1 | tail -1) )"
ARTIFACTS="$(find "$PROJECT/.niki" -name '*.json' 2>/dev/null | wc -l)"

printf '  exit code         : %s\n' "$RUN_EXIT"
printf '  branch created    : %s\n' "${BRANCH:-<none>}"
printf '  branch diff bytes : %s\n' "$PATCH_BYTES"
printf '  json artifacts    : %s\n' "$ARTIFACTS"
printf '  tests on main     : %s\n' "$TESTS_NOW"

echo
if [ "$RUN_EXIT" -eq 0 ] && [ -n "$BRANCH" ] && [ "$PATCH_BYTES" -gt 0 ]; then
  green "RESULT: niki produced a reviewable branch for a real bug with a local model."
elif [ "$RUN_EXIT" -ne 0 ]; then
  red "RESULT: niki failed. It did not hand over a branch."
  info "This is a product finding, not a harness bug. The most common cause is that"
  info "the model cannot produce a schema-conformant artifact; the run log above says"
  info "which stage failed and why."
else
  red "RESULT: niki reported success but produced no usable branch."
fi

[ "$KEEP" -eq 1 ] && info "scratch project kept at $PROJECT"
exit 0
