#!/usr/bin/env bash
# W1 — prove a real model answers, end to end, with no key faked and no network faked.
#
#   ./scripts/smoke_real_model.sh                      # real provider, needs your key
#   ./scripts/smoke_real_model.sh --provider fixture   # deterministic, no key, no network
#
# What this is: the first-answer leg. Install → provider → key → question → streamed answer,
# with the answer checked rather than eyeballed. It is deliberately the *smallest* thing that
# proves W1, because everything else in the product sits on top of this and a failure here is
# a failure everywhere.
#
# What this is not: it is not a quality measurement. One prompt against one model says nothing
# about how good the answers are, and nothing printed here may be quoted as though it did.
#
# The fixture mode exists because the CI leg has to run on every pull request and cannot carry
# a secret. It drives the identical assertions against `tests/integration/mock_llm.py`, a
# scripted OpenAI-compatible server. It proves the plumbing and the failure handling. It does
# not prove that any model is reachable — only the real-provider mode does that, and only on a
# machine that has a key.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

BIN="${NIKI_BIN:-$REPO_ROOT/target/release/niki}"
PROVIDER="${NIKI_SMOKE_PROVIDER:-}"   # openai | anthropic | google | fixture
WORK="$(mktemp -d)"
OUT="$WORK/answer.txt"
ERR="$WORK/err.txt"
MOCK_PID=""
FAILURES=0

cleanup() {
    [ -n "$MOCK_PID" ] && kill "$MOCK_PID" 2>/dev/null
    rm -rf "$WORK"
}
trap cleanup EXIT

ok()   { printf '  \033[32mok\033[0m    %s\n' "$1"; }
bad()  { printf '  \033[31mFAIL\033[0m  %s\n' "$1"; FAILURES=$((FAILURES + 1)); }
info() { printf '        %s\n' "$1"; }

die() { printf '\nsmoke: %s\n' "$1" >&2; exit 2; }

while [ $# -gt 0 ]; do
    case "$1" in
        --provider) PROVIDER="$2"; shift 2 ;;
        --provider=*) PROVIDER="${1#*=}"; shift ;;
        --bin) BIN="$2"; shift 2 ;;
        -h|--help) sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) die "unknown flag: $1" ;;
    esac
done

[ -x "$BIN" ] || die "no executable at $BIN
Build one first:  cargo build --release -j 2
Or point NIKI_BIN at an existing binary."

printf '\nNIKI first-answer smoke\n'
info "binary: $BIN"

# ── 1. The binary runs at all ────────────────────────────────────────────────
printf '\n1. the binary starts\n'
VERSION="$("$BIN" --version 2>&1)" || die "the binary did not start: $VERSION"
ok "$VERSION"
case "$VERSION" in *"panicked"*) bad "the version banner contains a panic" ;; esac

# ── 2. A provider is reachable ───────────────────────────────────────────────
printf '\n2. a provider answers one question\n'

case "$PROVIDER" in
    fixture)
        PORT="${NIKI_SMOKE_PORT:-8123}"
        info "starting the scripted server on :$PORT (no key, no network)"
        MOCK_LLM_PORT="$PORT" python3 tests/integration/mock_llm.py >"$WORK/mock.log" 2>&1 &
        MOCK_PID=$!
        for _ in $(seq 1 50); do
            if curl -fsS -m 1 -o /dev/null "http://127.0.0.1:$PORT/v1/models" 2>/dev/null; then
                break
            fi
            sleep 0.2
        done
        curl -fsS -m 2 -o /dev/null "http://127.0.0.1:$PORT/v1/models" \
            || die "the scripted server never came up; see $WORK/mock.log"
        BASE_URL="http://127.0.0.1:$PORT"
        MODEL="${NIKI_SMOKE_MODEL:-mock-model}"
        KEY="fixture-key-not-used"
        AGENT_PROVIDER="openai"
        ;;
    "")
        BASE_URL="${NIKI_BASE_URL:-}"
        MODEL="${NIKI_MODEL:-}"
        AGENT_PROVIDER="${NIKI_SMOKE_PROVIDER_NAME:-openai}"
        case "$AGENT_PROVIDER" in
            anthropic) KEY="${ANTHROPIC_API_KEY:-}" ;;
            google)    KEY="${GOOGLE_API_KEY:-}" ;;
            *)         KEY="${OPENAI_API_KEY:-${NIKI_API_KEY:-}}" ;;
        esac
        [ -n "$BASE_URL" ] || die "no base URL. Set NIKI_BASE_URL, or pass --provider fixture."
        [ -n "$MODEL" ]    || die "no model. Set NIKI_MODEL, or pass --provider fixture."
        [ -n "$KEY" ]      || die "no API key in the environment for provider '$AGENT_PROVIDER'."
        info "provider=$AGENT_PROVIDER model=$MODEL base_url=$BASE_URL"
        ;;
    *)
        die "unknown --provider '$PROVIDER' (expected 'fixture' or leave it unset)"
        ;;
esac

mkdir -p "$WORK/proj"
cat >"$WORK/proj/niki.toml" <<TOML
# Written by smoke_real_model.sh. Hand-written on purpose: the wizard is a different row, and
# a smoke test that depends on the wizard proves the wizard and the provider at the same time.
[docker]
backend = "worktree"

[agents.coder]
provider = "${AGENT_PROVIDER}"
model = "${MODEL}"

[providers.${AGENT_PROVIDER}]
base_url = "${BASE_URL}"
api_key = "${KEY}"
default_model = "${MODEL}"

[permissions]
mode = "manual"
TOML

"$BIN" chat -m "In one sentence: what does a build system do?" \
    -p "$WORK/proj" >"$OUT" 2>"$ERR"
STATUS=$?
COMBINED="$(cat "$OUT")$(cat "$ERR")"

[ -s "$OUT" ] || bad "the provider produced no output at all"
# A check that passes on an error message is not a check. The answer has to be long enough to
# be prose, because every failure mode in this product also "produces output".
if [ "$(wc -c <"$OUT")" -lt 20 ]; then
    bad "the output is $(( $(wc -c <"$OUT") )) bytes — too short to be an answer"
    info "$(head -c 300 "$OUT")"
fi
case "$COMBINED" in
    *"panicked at"*)  bad "the run panicked" ;;
    *"stack backtrace"*) bad "the run printed a stack backtrace" ;;
esac
if printf '%s' "$COMBINED" | grep -q $'\033'; then
    bad "the output contains raw escape sequences — a terminal program is running with no terminal"
else
    ok "no escape sequences in the answer"
fi
if [ "$STATUS" -ne 0 ]; then
    bad "chat exited $STATUS"
    info "$(printf '%s' "$COMBINED" | head -c 400)"
else
    ok "chat exited 0"
fi
info "answer: $(printf '%s' "$COMBINED" | tr -s '[:space:]' ' ' | head -c 160)"

# ── 3. The key is not echoed back ────────────────────────────────────────────
printf '\n3. the answer does not leak the key\n'
if printf '%s' "$COMBINED" | grep -qF "$KEY"; then
    bad "the API key appears in the output"
else
    ok "no key in the output"
fi

# ── 4. The same config drives a headless run ────────────────────────────────
printf '\n4. the same provider drives a headless JSON run\n'
cd "$WORK/proj"
git init -q . 2>/dev/null && git -c user.email=smoke@example.invalid -c user.name=smoke \
    commit -q --allow-empty -m initial 2>/dev/null
ENVELOPE="$("$BIN" run "Say hello in one sentence" --backend worktree --quiet \
    --output-format json 2>>"$ERR")"
if printf '%s' "$ENVELOPE" | python3 -c 'import json,sys; json.loads(sys.stdin.read())' 2>/dev/null; then
    ok "stdout is exactly one JSON envelope"
else
    bad "stdout was not a single JSON envelope"
    info "$(printf '%s' "$ENVELOPE" | head -c 300)"
fi

cd "$REPO_ROOT"
printf '\n'
if [ "$FAILURES" -eq 0 ]; then
    if [ "$PROVIDER" = "fixture" ]; then
        printf '\033[32mSMOKE PASSED\033[0m (fixture provider — plumbing proven, model reachability NOT proven)\n'
    else
        printf '\033[32mSMOKE PASSED\033[0m (real provider: %s / %s)\n' "$AGENT_PROVIDER" "$MODEL"
    fi
    exit 0
fi
printf '\033[31mSMOKE FAILED\033[0m — %d check(s) failed\n' "$FAILURES"
exit 1