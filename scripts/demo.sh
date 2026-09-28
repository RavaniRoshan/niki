#!/usr/bin/env bash
# NIKI self-serve demo — no API key, no container runtime, under five minutes.
#
#   ./scripts/demo.sh                 # run the demo, leave the branch on screen
#   ./scripts/demo.sh --keep          # keep the scratch project to poke at
#   NIKI_BIN=/path/to/niki ./scripts/demo.sh
#
# What is real here, and what is not, stated up front because the difference is
# the whole point of the demo:
#
#   REAL   the four-agent pipeline, the planner/coder/tester/reviewer handoff,
#          JSON-schema validation of every artifact, the worktree sandbox,
#          diff capture, branch creation, the TUI.
#   Canned the model's answers. `tests/integration/mock_llm.py` is a scripted
#          OpenAI-compatible server; it returns a fixed, schema-valid response
#          per role. Nothing is inferred, nothing is generated.
#
# So this demonstrates the harness — that the plumbing, the contracts, the
# review gate and the branch hand-off work end to end. It does not demonstrate
# model quality, and it should not be read as though it does. For that, point
# NIKI at a real provider with a real key and run the same command.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${NIKI_BIN:-$REPO_ROOT/target/release/niki}"
PORT="${NIKI_DEMO_PORT:-8099}"
WORK="$(mktemp -d)"
MOCK_PID=""
KEEP=0
[ "${1:-}" = "--keep" ] && KEEP=1

cyan() { printf '\033[36m%s\033[0m\n' "$*"; }
grn()  { printf '\033[32m%s\033[0m\n' "$*"; }
red()  { printf '\033[31m%s\033[0m\n' "$*" >&2; }
dim()  { printf '\033[2m%s\033[0m\n' "$*"; }

cleanup() {
  [ -n "$MOCK_PID" ] && kill "$MOCK_PID" 2>/dev/null || true
  if [ "$KEEP" -eq 1 ]; then
    cyan "Scratch project kept at: $WORK/demo-project"
  else
    rm -rf "$WORK"
  fi
}
trap cleanup EXIT

# ── 0. Preflight ──────────────────────────────────────────────────────────
# `required_tools` (src/orchestrator/pipeline.rs) demands git, node, npm and
# python3 unconditionally, and the worktree backend enforces them on the HOST
# with `command -v`. A stranger without Node therefore gets
# `Sandbox (worktree) is missing required tools: node, npm` — a confusing
# first-run failure. CI installs Node for exactly this reason and never says why.
# Say it here, in one line, before anything starts.
missing=()
for t in git node npm python3 curl; do
    command -v "$t" >/dev/null 2>&1 || missing+=("$t")
done
if [ "${#missing[@]}" -gt 0 ]; then
    red "NIKI's worktree backend needs these on your PATH: ${missing[*]}"
    red "git, node, npm and python3 are required by the pipeline's sandbox preflight."
    red "Install them and re-run. On Debian/Ubuntu: sudo apt-get install -y git nodejs npm python3 curl"
    exit 1
fi

if [ ! -x "$BIN" ]; then
    red "niki binary not found at $BIN"
    red "Build it first:  cargo build --release"
    red "or point at one:  NIKI_BIN=/path/to/niki $0"
    exit 1
fi

# ── 1. Scripted LLM ───────────────────────────────────────────────────────
# Scrub the environment first. Env always beats niki.toml, so a developer with
# ANTHROPIC_BASE_URL pointed at an internal proxy would have this "local" demo
# silently leave 127.0.0.1 and spend their tokens on a real endpoint.
for v in $(env | sed -n 's/^\([A-Z0-9_]*\)_BASE_URL=.*/\1_BASE_URL/p'); do
    unset "$v"
    dim "unset $v (env overrides niki.toml; the demo must talk to the local mock)"
done
unset ANTHROPIC_API_KEY OPENAI_API_KEY GOOGLE_API_KEY 2>/dev/null || true

MOCK_LLM_PORT="$PORT" python3 "$REPO_ROOT/tests/integration/mock_llm.py" >"$WORK/mock.log" 2>&1 &
MOCK_PID=$!

# A mock that never came up used to fall through silently here, and every later
# step then failed against a dead port with a misleading error.
ready=0
for _ in $(seq 1 40); do
    if curl -fsS "http://127.0.0.1:$PORT/health" >/dev/null 2>&1; then ready=1; break; fi
    kill -0 "$MOCK_PID" 2>/dev/null || break
    sleep 0.5
done
if [ "$ready" -ne 1 ]; then
    red "the scripted LLM never became ready on port $PORT"
    cat "$WORK/mock.log" >&2
    exit 1
fi

# ── 2. The fixture ────────────────────────────────────────────────────────
# Two constraints, both load-bearing, both learned the hard way:
#
#   * `index.js` must contain `console.log("hello");` verbatim. The mock's only
#     edit is a search/replace pair on exactly that string, and apply_patch
#     writes NOTHING unless every block matches — so a fixture without it
#     produces a failed stage and no branch.
#   * NO manifest file. `autodetect_test_command` maps package.json -> npm test
#     and Cargo.toml -> cargo test. With no manifest the Tester reports "no
#     verification was possible"; WITH one, the demo runs a real `npm test`
#     that fails in a project with no tests.
FIX="$WORK/demo-project"
mkdir -p "$FIX"
cat >"$FIX/index.js" <<'JS'
const http = require('http');

const server = http.createServer((req, res) => {
  console.log("hello");
  res.end('ok');
});

server.listen(3000);
JS
cat >"$FIX/README.md" <<'MD'
# demo-project

A throwaway Node server. NIKI's four agents are about to add a `GET /health`
route to `index.js` and hand you a branch with the change on it.
MD

# Skip the first-run modal, which otherwise swallows the first several
# keypresses — the same modal the committed screenshots are stuck on.
mkdir -p "$FIX/.niki"
printf '{"onboarded":true}\n' >"$FIX/.niki/state.json"

git -C "$FIX" init -q
git -C "$FIX" config user.email "demo@niki.local"
git -C "$FIX" config user.name "niki demo"
git -C "$FIX" add -A
git -C "$FIX" commit -q -m "initial commit"

# ── 3. Config ─────────────────────────────────────────────────────────────
# Three settings that decide whether the demo shows what it claims to.
#
#   provider = "openai"   NOT "mock". A provider literally named `mock` is the
#       file-backed MockScriptProvider, which reads a JSON script off disk —
#       naming it that while pointing base_url at a port gives
#       "Failed to read mock script http://127.0.0.1:PORT/v1: No such file or
#       directory". The scripted *server* is reached through a normal provider
#       name, which is what CI's own niki.test.toml does.
#
#   topology = "multiagent"  Without it the mock's TaskSpec says
#       `estimated_complexity: low`, the default threshold collapses Auto to the
#       single-agent fast path, and the stranger watches ONE agent do the work —
#       which is the opposite of the pitch.
#
#   risk is left at its default. The canned spec touches one small file, so it
#       classifies Low and no SecurityAuditor/Critic is injected. (The mock can
#       answer for both — see `ROLE_MARKERS` in mock_llm.py — so this is about
#       keeping the demo to four agents, not about a limitation.)
cat >"$FIX/niki.toml" <<TOML
[docker]
backend = "worktree"

[pipeline]
topology = "multiagent"

[providers.openai]
base_url = "http://127.0.0.1:$PORT"
api_key = "demo-key-not-used"
default_model = "mock-model"

[providers.anthropic]
base_url = "http://127.0.0.1:$PORT"
api_key = "demo-key-not-used"
default_model = "mock-model"

[agents.planner]
provider = "anthropic"
model = "mock-model"

[agents.coder]
provider = "anthropic"
model = "mock-model"

[agents.tester]
provider = "openai"
model = "mock-model"

[agents.reviewer]
provider = "anthropic"
model = "mock-model"
TOML

# ── 4. Run ────────────────────────────────────────────────────────────────
cat <<'BANNER'

  NIKI demo — four agents, a real pipeline, a scripted model.

  Watch: the planner writes the spec, the coder edits index.js, the tester
  reports, the reviewer signs off, and the run ends on a real git branch you
  can read, diff and throw away.

  The model's answers are canned. The plumbing is not.

BANNER

set +e
"$BIN" run --project "$FIX" --backend worktree \
    "Add a GET /health endpoint to index.js that returns {\"status\":\"ok\"} with HTTP 200"
status=$?
set -e

# ── 5. Report ─────────────────────────────────────────────────────────────
echo
# `git branch --list` prefixes the checked-out branch with "* " and indents the
# rest with two spaces, so the raw output is "* niki/abc" / "  niki/def". Reading
# it verbatim put a literal `*` into the `git diff` command printed below, which
# then failed. `git branch --format` has neither decoration.
branches="$(git -C "$FIX" branch --format='%(refname:short)' --list 'niki/*')"
branch="$(printf '%s\n' "$branches" | head -1)"

if [ -z "$branch" ]; then
    red "The run produced no branch (exit $status)."
    dim "Task artifacts, including report.md and the raw responses, are in:"
    dim "  $FIX/.niki/tasks/"
    dim "The scripted LLM's log is at $WORK/mock.log"
    exit 1
fi

# Print what actually changed rather than making the user go and look. A demo
# that ends by telling you where to look has made you do the last step yourself.
dim "  $(git -C "$FIX" diff --stat "master...$branch" | tail -1)"

grn "Done. niki handed you a branch: $branch"
echo
echo "  Read what it did:"
echo "    cd $FIX"
echo "    git log -1 --stat"
echo "    git diff master...$branch"
echo
echo "  The full transcript, the review verdict and the cost are in:"
echo "    $FIX/.niki/tasks/"
echo
