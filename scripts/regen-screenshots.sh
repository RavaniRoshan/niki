#!/usr/bin/env bash
# Regenerate the two marketing screenshots in assets/screenshots/.
#
# The committed `cost.png` and `diff.png` are 99.77% identical — the same
# onboarding modal, truncated mid-word — while the Product Hunt gallery
# captions them "honest cost report" and "the reviewable branch output".
# Neither shows a cost or a diff. Against a README that opens with "Proof, not
# promises", that is the one artifact that makes the sentence self-refuting.
#
# ── STATUS: not yet shippable ───────────────────────────────────────────
# This script works and produces genuinely distinct images of the real TUI, but
# the captured runs do not yet populate the pages, so the output must NOT be
# committed in place of the current files:
#
#   * `/diff` navigates, but the mock LLM's role detection is a substring
#     match with a silent default of "planner", so the Coder receives a task
#     spec and the page reads "No diff produced by Coder".
#   * `/cost` does not switch pages from the chat view, so the capture lands
#     on the transcript with session economics at zero.
#
# Shipping a "reviewable branch output" screenshot that says "no output" would
# trade one kind of dishonesty for another. Getting these right needs a mock
# that returns role-appropriate artifacts, which is the same gap the
# `wrong_role_artifact` fault in tests/reverse/ covers.
#
# These capture the real binary, dismiss onboarding, and navigate to the
# actual page. NIKI_FORCE_ONBOARDING is deliberately NOT set: the visual
# regression tapes need the modal for frame stability, but a screenshot of the
# product must not be a screenshot of a modal.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

BIN="${NIKI_BIN:-$REPO_ROOT/target/release/niki}"
OUT="$REPO_ROOT/assets/screenshots"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

if [ ! -x "$BIN" ]; then
  echo "error: $BIN not found or not executable. Run: cargo build --release" >&2
  exit 1
fi

for tool in vhs ttyd; do
  command -v "$tool" >/dev/null 2>&1 || {
    echo "error: $tool is required (see tests/visual/run.sh for the same dependency)" >&2
    exit 1
  }
done

mkdir -p "$OUT"

# The mock LLM exists so the session has REAL numbers to show. A screenshot
# labelled "honest cost report" that reads $0.0000 would be the same
# dishonesty the previous pair of images committed, just with better
# typography.
echo "== starting mock LLM"
python3 "$REPO_ROOT/tests/integration/mock_llm.py" >"$WORK/mock.log" 2>&1 &
MOCK_PID=$!
trap 'kill "$MOCK_PID" 2>/dev/null || true; rm -rf "$WORK"' EXIT
ready=0
for _ in $(seq 1 40); do
  if curl -fsS http://localhost:8080/health >/dev/null 2>&1; then ready=1; break; fi
  sleep 0.5
done
[ "$ready" -eq 1 ] || { echo "error: mock LLM did not become healthy" >&2; cat "$WORK/mock.log" >&2; exit 1; }
echo "   mock LLM ready"

capture() {
  local name="$1" page="$2"
  local fixture="$WORK/$name-fixture"
  rm -rf "$fixture"
  mkdir -p "$fixture"
  git init -q "$fixture"
  git -C "$fixture" config user.email "shots@niki.dev"
  git -C "$fixture" config user.name "Shots"
  cat >"$fixture/src_list.rs" <<'RS'
pub fn paginate(items: &[u32], start: usize, size: usize) -> &[u32] {
    let end = start + size - 1;
    &items[start..end]
}
RS
  cat >"$fixture/niki.toml" <<'TOML'
[general]
max_revision_rounds = 3

[providers.anthropic]
base_url = "http://localhost:8080"
api_key = "mock-key"
default_model = "mock-model"

[agents.planner]
provider = "anthropic"
model = "mock-model"

[agents.coder]
provider = "anthropic"
model = "mock-model"

[agents.tester]
provider = "anthropic"
model = "mock-model"

[agents.reviewer]
provider = "anthropic"
model = "mock-model"
TOML

  git -C "$fixture" add -A
  git -C "$fixture" commit -qm initial

  local tape="$WORK/$name.tape"
  # `Screenshot` is the last command on purpose: VHS emits a GIF when frames
  # follow it, and these are meant to be stills.
  cat >"$tape" <<TAPE
Set Shell "bash"
Set FontSize 15
Set FontFamily "DejaVu Sans Mono"
Set Width 1400
Set Height 860
Hide
Type "NIKI_REDUCED_MOTION=1 NIKI_CI=1 $BIN chat -p $fixture -m 'Fix the off-by-one in paginate'" Enter
Sleep 3s
Show
Sleep 6s
Type "/$page" Enter
Sleep 3s
Screenshot "$OUT/$name.png"
TAPE

  echo "== capturing $name.png (page: /$page)"
  vhs "$tape" >"$WORK/$name.log" 2>&1 || {
    echo "   VHS failed for $name — log follows" >&2
    cat "$WORK/$name.log" >&2
    return 1
  }
  [ -s "$OUT/$name.png" ] || {
    echo "   $name.png was not produced" >&2
    return 1
  }
  echo "   wrote $OUT/$name.png"
}

capture cost cost
capture diff diff

# The whole point of regenerating: prove the two are actually different.
python3 - "$OUT/cost.png" "$OUT/diff.png" <<'PY'
import sys
from PIL import Image, ImageChops
import numpy as np
a = Image.open(sys.argv[1]).convert("RGB")
b = Image.open(sys.argv[2]).convert("RGB")
if a.size != b.size:
    print(f"  ok: different dimensions ({a.size} vs {b.size})")
    raise SystemExit(0)
d = np.asarray(ImageChops.difference(a, b)).sum(axis=2)
pct = (d > 0).sum() / (a.size[0] * a.size[1]) * 100
print(f"  {'ok' if pct > 5 else 'FAIL'}: screenshots differ in {pct:.2f}% of pixels")
if pct <= 5:
    print("  FAIL: the two screenshots are effectively the same image, which is the")
    print("        problem this script exists to fix.")
    raise SystemExit(1)
PY

echo
echo "regenerated:"
ls -la "$OUT"/cost.png "$OUT"/diff.png
