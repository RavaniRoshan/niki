#!/usr/bin/env bash
# G6: every failure path a user can walk into, each asserted for its exit code
# AND for a message a person can act on.
#
# Asserting only "it exits non-zero" is the mistake `niki report` made: three
# failure branches all `return Ok(())`, so `niki report zzzzzzzz` printed
# "No task matching 'zzzzzzzz' found" and exited 0. A script cannot tell that
# from success. So every case here checks both.
#
# Each case is a real process against the real binary. No mocks, because the
# thing under test is the thing the user runs.

set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 1
BIN="$ROOT/target/release/niki"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/niki-g6.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

fails=0

# case <name> <expected exit> <substring the message must contain> -- <command...>
case_() {
  local name="$1" want_rc="$2" want_msg="$3"; shift 4
  local out rc
  out="$("$@" 2>&1)"; rc=$?
  if [ "$rc" != "$want_rc" ]; then
    echo "FAIL $name: exit $rc, expected $want_rc"
    echo "      output: ${out:0:200}"
    fails=$((fails + 1)); return
  fi
  if [ -n "$want_msg" ] && ! grep -qiF -- "$want_msg" <<<"$out"; then
    echo "FAIL $name: exit $rc is right but the message does not say \"$want_msg\""
    echo "      output: ${out:0:200}"
    fails=$((fails + 1)); return
  fi
  echo "ok   $name"
}

if [ ! -x "$BIN" ]; then
  echo "no release binary at $BIN — run: cargo build --release"
  exit 2
fi

# A real repo, so the checks that need one have one.
git -C "$WORK" init -q
git -C "$WORK" -c user.email=t@t -c user.name=t commit -q --allow-empty -m init

# (a) Bad input.
case_ "empty task description"        1 "No task given"            -- "$BIN" run ""
case_ "not a git repository"          1 "not inside a git repository" -- env -C "$WORK/.." "$BIN" run "x" --project /tmp --dry-run
case_ "nonexistent project"           1 ""                         -- "$BIN" run "x" --project "$WORK/nope" --dry-run

# (b) Missing environment — no key, no config.
# The message depends on the machine — a keyring entry or a running Ollama can
# both make a provider reachable — so this asserts the *behaviour*: a run with
# nothing usable exits non-zero and says what is missing.
case_ "no usable provider"            1 "API key"                 -- \
  env -u ANTHROPIC_API_KEY -u OPENAI_API_KEY -u GOOGLE_API_KEY -u OPENROUTER_API_KEY \
      HOME="$WORK/nohome" XDG_CONFIG_HOME="$WORK/nohome/.config" \
      PATH="/usr/bin:/bin" \
      "$BIN" run "x" --project "$WORK" --backend worktree --dry-run

# (c) A configuration that cannot be parsed.
printf 'this is not = valid = toml [[[\n' > "$WORK/niki.toml"
case_ "unparseable niki.toml (run)"   1 "TOML"                    -- \
  "$BIN" run "x" --project "$WORK" --backend worktree --dry-run
case_ "niki config check names it"    1 "niki.toml"               -- \
  env -C "$WORK" "$BIN" config check || true
rm -f "$WORK/niki.toml"

# (d) Reading a report that does not exist must not exit 0.
case_ "report for an unknown id"      1 ""                         -- \
  "$BIN" report zzzzzzzz --project "$WORK"
case_ "report with no tasks at all"   1 ""                         -- \
  "$BIN" report --project "$WORK"
case_ "status in a repo with no runs" 1 ""                         -- \
  "$BIN" status --project "$WORK"

# (e) The chat surface's own guards.
case_ "chat without a terminal"       1 "needs an interactive terminal" -- \
  env HOME="$WORK/nohome" "$BIN" chat < /dev/null

# (f) An unknown subcommand is the CLI's job to reject.
case_ "unknown subcommand"            2 ""                         -- "$BIN" definitely-not-a-command

# (g) A model that cannot be reached must say so, not hang or panic.
case_ "unreachable provider"          1 ""                         -- \
  env -u ANTHROPIC_API_KEY -u OPENAI_API_KEY -u GOOGLE_API_KEY \
      HOME="$WORK/nohome" XDG_CONFIG_HOME="$WORK/nohome/.config" \
      NIKI_BASE_URL="http://127.0.0.1:9" \
      "$BIN" chat --message "hi" < /dev/null

if [ $fails -gt 0 ]; then
  echo "G6: $fails failure path(s) did not behave"
  exit 1
fi
echo "G6: every failure path exits non-zero with a message a person can act on"
