#!/usr/bin/env bash
# The first command. Checks the machine, says what is missing, then runs the
# pipeline — or tells you precisely which stage stopped and what to do about it.
#
#   ./run.sh              run the exercise
#   ./run.sh --check      check the setup and stop
#   ./run.sh --model X    use model X for every agent
#
# Nothing here is required by NIKI. It exists so that a first run has one
# command, and so that a failure has one explanation.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$HERE"

TASK='Implement GET /health so it returns 200 with a JSON body of exactly { status: "ok" }, and add tests for it'

cyan() { printf '\033[36m%s\033\0m\n' "$*"; }
grn()  { printf '\033[32m%s\033[0m\n' "$*"; }
red()  { printf '\033[31m%s\033[0m\n' "$*" >&2; }
dim()  { printf '\033[2m%s\033[0m\n' "$*"; }

CHECK_ONLY=0
MODEL=""
for arg in "$@"; do
    case "$arg" in
        --check) CHECK_ONLY=1 ;;
        --model) shift; MODEL="${1:-}" ;;
        --model=*) MODEL="${arg#--model=}" ;;
        -h|--help)
            sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
    esac
done

problems=0

# ── Preflight ───────────────────────────────────────────────────────────
# NIKI's sandbox preflight requires git, node, npm and python3, on the host as
# well as in the container. A stranger without Node gets a confusing first-run
# failure, so it is said here, in one line, before anything starts.
echo
cyan "Checking your setup"
echo
missing=()
for tool in git node npm python3 curl; do
    command -v "$tool" >/dev/null 2>&1 || missing+=("$tool")
done
if [ "${#missing[@]}" -gt 0 ]; then
    red "  missing: ${missing[*]}"
    dim  "  NIKI's worktree backend needs these on your PATH."
    dim  "  Debian/Ubuntu:  sudo apt-get install -y git nodejs npm python3 curl"
    problems=1
else
    grn "  tools            git, node, npm, python3, curl"
fi

if command -v niki >/dev/null 2>&1; then
    grn "  niki             $(niki --version 2>/dev/null | head -1)"
else
    red "  niki is not installed."
    dim  "  macOS:           brew install niki"
    dim  "  Linux/macOS:     curl -fsSL https://raw.githubusercontent.com/RavaniRoshan/niki/master/scripts/install.sh | bash"
    problems=1
fi

# Ollama: the configured provider. Reachable and with a model is the state
# where a run can actually happen.
if curl -s --max-time 2 http://localhost:11434/api/tags >/dev/null 2>&1; then
    pulled="$(curl -s --max-time 2 http://localhost:11434/api/tags 2>/dev/null \
        | grep -o '"name":"[^"]*"' | sed 's/"name":"//;s/"//' | head -3 | tr '\n' ' ')"
    if [ -n "$pulled" ]; then
        grn "  ollama           running — models: $pulled"
    else
        red "  ollama is running but has no models."
        dim  "  pull one:        ollama pull qwen2.5-coder:7b"
        problems=1
    fi
else
    red "  ollama is not reachable on 127.0.0.1:11434."
    dim  "  start it:        ollama serve"
    dim  "  then pull a model: ollama pull qwen2.5-coder:7b"
    problems=1
fi

if [ ! -f niki.toml ]; then
    red "  niki.toml is missing from $(pwd)"
    problems=1
else
    grn "  config          niki.toml (worktree backend, local model)"
fi

echo
if [ "$problems" -ne 0 ]; then
    red "Something above needs fixing first. Nothing has been run."
    dim "  TROUBLESHOOTING.md has the details."
    exit 1
fi

# ── The starting state ──────────────────────────────────────────────────
# Shown before the run, not after. When NIKI is done you will run this again,
# and the only interesting thing about that result is that it changed — which
# you cannot tell without having seen the before.
echo
cyan "Before"
dim  "────────"
if node --test test/ >/tmp/niki-starter-before.log 2>&1; then
    grn "  node --test test/  PASSED"
    red  "  This starter is supposed to start with failing tests."
    red  "  See TROUBLESHOOTING.md — something is wrong with your copy."
    exit 1
else
    dim "  node --test test/  fails, as it should:"
    grep -E '^✖|^# fail|not ok' /tmp/niki-starter-before.log 2>/dev/null | head -5 | sed 's/^/    /'
fi

if [ "$CHECK_ONLY" -eq 1 ]; then
    echo
    grn "Setup looks complete. Run ./run.sh to start the pipeline."
    exit 0
fi

# ── Run ─────────────────────────────────────────────────────────────────
if [ -n "$MODEL" ]; then
    echo
    dim "Using model '$MODEL' for every agent (niki.toml updated)."
    tmp="$(mktemp)"
    sed "s/^model = \".*\"$/model = \"$MODEL\"/" niki.toml > "$tmp" && mv "$tmp" niki.toml
fi

echo
cyan "Running the pipeline"
dim  "────────"
echo
dim "  $TASK"
echo

niki run "$TASK"

status=$?
echo

# ── Explain what happened ───────────────────────────────────────────────
if [ "$status" -eq 0 ]; then
    grn "Done."
    echo
    echo "  A branch was created. Find it with:"
    dim   "    git branch --list 'niki/*'"
    echo
    echo "  Read what the agents decided — this is the interesting part:"
    dim   "    niki report          REPORT-GUIDE.md explains every section"
    echo
    echo "  Then check the result yourself:"
    dim   "    node --test test/"
    echo
else
    red "The run did not finish. That is usually the model, not the setup."
    echo
    echo "  What to read:"
    dim   "    niki report          the stage that stopped, and why"
    dim   "    ls .niki/tasks/*/artifacts/   what the model actually returned"
    echo
    echo "  The most common cause, and the fix:"
    dim   "    TROUBLESHOOTING.md   § 'The run stops at the Coder stage'"
    dim   "    HONESTY.md           § 1, which models work and which do not"
    echo
    dim "  Your working tree was not modified. Nothing to clean up."
fi

exit $status
