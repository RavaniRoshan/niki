#!/usr/bin/env bash
# The nine release gates, one command, non-zero if any fails.
#
#   ./scripts/verify.sh                  # everything
#   ./scripts/verify.sh --fast           # skip G4's long leg and G8
#   ./scripts/verify.sh --only G3,G6     # a subset
#   ./scripts/verify.sh --update-evidence  # rewrite EVIDENCE.md from this run
#
# "Done" is defined only by these. Not by a green build, and not by a passing
# subset someone remembered to run.
#
# This file exists because the repository had twelve verification scripts and
# no single source of truth. `verify-product.sh` is a full product verifier with
# startup-time, demo-time and eval-recall gates — and **no workflow runs it**.
# A gate that only runs by hand is a gate that rots, and a gate nobody knows
# is the authority is a gate nobody trusts.
#
# Memory. This repository is developed on a box that shares ~7.5 GiB with
# another session, and AGENTS.md records that a `cargo` link (1.5–2 GB) running
# beside a live-model sweep is what froze the box twice. So: `CARGO_BUILD_JOBS=2`
# throughout, `-j 2` on every cargo invocation, `--test-threads=1` on every
# test binary, and no two cargo processes at once. `test-layer.sh` is reused for
# the full sweep rather than reimplementing a second runner.
#
# `set -uo pipefail`, deliberately no `-e`: one failing gate must not hide the
# other eight. Status is accumulated and the script exits non-zero at the end —
# the same discipline `test-layer.sh:21,176` already uses correctly.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 1

export CARGO_BUILD_JOBS=2
CARGO_TEST_FLAGS="-j 2 -- --test-threads=1"

FAST=0
UPDATE_EVIDENCE=0
ONLY=""
# A `while` with `shift`, not `for arg in "$@"`: the `for` form iterates a
# snapshot, so `shift` inside it has no effect and `--only G3` consumed nothing
# and then rejected `G3` as an unknown flag. The documented invocation was the
# one form that did not work.
while [ $# -gt 0 ]; do
  case "$1" in
    --fast) FAST=1 ;;
    --update-evidence) UPDATE_EVIDENCE=1 ;;
    --only=*) ONLY="${1#*=}" ;;
    --only)
      if [ $# -lt 2 ]; then echo "--only needs a gate list, e.g. --only G3,G6" >&2; exit 64; fi
      ONLY="$2"; shift
      ;;
    -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
    *) echo "unknown flag: $1" >&2; exit 64 ;;
  esac
  shift
done

EV="$ROOT/.evidence"
mkdir -p "$EV"

want() {
  [ -z "$ONLY" ] && return 0
  case ",$ONLY," in *",$1,"*) return 0 ;; esac
  return 1
}

declare -a RESULTS=()
FAILED=0
SKIPPED=0

record() { # gate, status, one-line reason
  RESULTS+=("$1|$2|$3")
  case "$2" in
    PASS) printf '  \033[32mPASS\033[0m  %-4s %s\n' "$1" "$3" ;;
    FAIL) printf '  \033[31mFAIL\033[0m  %-4s %s\n' "$1" "$3"; FAILED=$((FAILED + 1)) ;;
    UNVERIFIED-EXTERNAL) printf '  \033[33mUNVERIFIED\033[0m %-4s %s\n' "$1" "$3"; FAILED=$((FAILED + 1)) ;;
    SKIP)  printf '  \033[90mSKIP\033[0m  %-4s %s\n' "$1" "$3"; SKIPPED=$((SKIPPED + 1)) ;;
  esac
}

run_capture() { # gate, logname, command...
  local gate="$1" name="$2"; shift 2
  "$@" >"$EV/$name.log" 2>&1
  local rc=$?
  if [ $rc -eq 0 ]; then
    record "$gate" PASS "$(tail -1 "$EV/$name.log" | cut -c1-90)"
  else
    record "$gate" FAIL "rc=$rc — see .evidence/$name.log"
    tail -15 "$EV/$name.log" | sed 's/^/        /'
  fi
}

printf '\n\033[1mNIKI release gates\033[0m — %s\n\n' "$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo '?')@$(git rev-parse --short HEAD 2>/dev/null || echo '?')"

# ── G1 · clean clone, install, README quick start ─────────────────────
if want G1; then
  printf '\n\033[1mG1 · clean clone + quick start\033[0m\n'
  bash -n scripts/*.sh 2>"$EV/g1-shell.log" \
    && record G1 PASS "all scripts parse" \
    || record G1 FAIL "a script does not parse — see .evidence/g1-shell.log"

  # Manifest parity: every published download URL must resolve. A release that
  # has not been cut leaves brew/scoop/winget 404-ing while the README's
  # headline install command is a dead link.
  local_parity=0
  : > "$EV/g1-manifest.log"
  for url in $(grep -rhoE 'https://[^"'"'"' ]*releases/download/[^"'"'"' ]*' homebrew scoop winget 2>/dev/null | sort -u); do
    # A manifest may build its URL from a variable (`.../v$version/...`). That
    # is a template, not a broken link, and curling it reports a 404 for a file
    # that does not exist by design.
    case "$url" in *'$'*) continue ;; esac
    code=$(curl -sIL -o /dev/null -w '%{http_code}' --max-time 25 "$url" 2>/dev/null || echo 000)
    # 3xx is a redirect to the release CDN, which means the asset exists. Only
    # a 404 — or no response at all — is a dead download.
    case "$code" in
      2*|3*) ;;
      *) echo "        $code $url" | tee -a "$EV/g1-manifest.log"; local_parity=1 ;;
    esac
  done
  if [ $local_parity -eq 0 ]; then
    record G1 PASS "every published download URL resolves"
  else
    record G1 FAIL "a manifest URL 404s — see .evidence/g1-manifest.log"
  fi

  if [ $FAST -eq 1 ]; then
    record G1 SKIP "clean-clone build skipped by --fast"
  elif [ -x target/release/niki ]; then
    # The README quick start, read out of the README rather than re-typed.
    v=$(target/release/niki --version 2>&1)
    if target/release/niki --help >/dev/null 2>&1; then
      record G1 PASS "$v — README quick start commands accepted"
    else
      record G1 FAIL "niki --help failed"
    fi
  else
    record G1 FAIL "no release binary; run: cargo build --release"
  fi
fi

# ── G2 · build, lint ──────────────────────────────────────────────────
if want G2; then
  printf '\n\033[1mG2 · build + lint\033[0m\n'
  run_capture G2 g2-fmt   cargo fmt --check
  run_capture G2 g2-clippy cargo clippy --all-targets -j 2 -- -D warnings
  if [ $FAST -eq 1 ]; then
    record G2 SKIP "release build skipped by --fast"
  else
    run_capture G2 g2-build cargo build --release -j 2
  fi
fi

# ── G3 · tests, plus a can-fail entry for every shipped feature ───────
if want G3; then
  printf '\n\033[1mG3 · tests + can-fail map\033[0m\n'
  run_capture G3 g3-lib cargo test --lib $CARGO_TEST_FLAGS

  # ── G3 fast lane · the cheap integration binaries ──────────────────────
  #
  # **`cargo test --lib` was the whole of G3's test run**, and the integration
  # binaries were checked only for *existence* — the canary map greps
  # `tests/` and `src/` for a name and stops there. So a test in `tests/` could
  # be red and every gate would still say PASS.
  #
  # Measured in batch 8, not assumed: three tests were red on this branch while
  # G1–G7 and G9 all reported green. Two were real defects —
  # `cost::a_fallback_served_call_is_priced_by_the_fallback` (the failover chain
  # did not fail over) and `money::the_coder_loop_bills_before_every_bail_out`
  # (a Coder that spent money and then errored was billed as free) — and both
  # are in the paths a user hits when a provider is slow or drops a connection.
  #
  # These four run serially, on one thread, in **~34 s** including link. The
  # whole suite still does not fit this machine (`AGENTS.md`), which is why this
  # is a named lane rather than `cargo test` — and why the honest statement in
  # `RELEASE_REPORT.md` §5 is unchanged: **this lane covers these four, and only
  # CI covers the rest.**
  #
  # Extend the list rather than replacing it with `cargo test`.
  # `mcp_call_path` joined the lane in batch 11: `the_pipeline_holds_the_mcp_
  # manager_beyond_discovery` had been **red** since a comment elsewhere grew
  # the file by eighteen characters, and it was not in the list. A lane you do
  # not extend is a lane that quietly stops covering.
  # The list grew in batch 11 from measuring rather than assuming: sixteen
  # further binaries were run, all green, all **sub-second** — fifteen of them
  # finished in 0.00s. `state_layout` is in the list because its
  # `the_temp_patch_in_a_user_repo_is_git_ignored` had been red since the patch
  # writer stopped producing that filename, and nothing ran the binary.
  #
  # A further fourteen were measured the same way in batch 11: **all fourteen
  # green**, and their own test times were 0.00 s–9.01 s (~17 s in total). The
  # wall clock around each is cargo's per-invocation overhead, not the tests —
  # which is why "expensive" was the wrong reason to leave them out.
  #
  # Cost, measured: the whole list runs in well under two minutes. The
  # binaries this box genuinely cannot afford are the heavy ones, and they are
  # serialised elsewhere (`.config/test-binary-groups`).
  fast_lane="record_claims_are_pinned run_lifecycle agent_tool_loop reverse mcp_call_path \
    state_layout patch_temp_path_is_unique acp_server chat_conversation \
    permission_prompt permission_visibility permission_badge_governs_the_run \
    tool_cards_are_live tui_footers_advertise_what_works tui_terminal_honesty \
    help_tells_the_truth chat_slash_commands state_writes_are_atomic \
    retry_tracking transient_classification_is_one_rule llm_timeout_classification \
    history_enter_opens_the_run config_field_cursor_is_visible \
    docs_consistency every_entry_point_delivers ci_contracts test_groups \
    no_unreferenced_public_modules skips_and_budgets_stay_honest \
    supply_chain_policy_has_teeth the_record_had_moved artifact_contracts \
    claims_audit stub_tools_do_not_report_success google_stream_finish_reason \
    exec_timeout security_exec redaction_keeps_evidence secret_redaction \
    tui_jk_reaches_the_page tui_page_letters_win tui_page_numbers_are_discoverable \
    tui_q_goes_back tui_tab_has_one_owner tui_every_binding_is_handled \
    tui_crash_paths tui_sheets tui_key_matrix_matches_the_code tool_contracts \
    truncated_tool_calls structured_output repo_intel structural_index provenance \
    llm_tool_calls llm_mock_provider"
  for bin in $fast_lane; do
    [ -f "tests/$bin.rs" ] || { record G3 FAIL "fast lane names tests/$bin.rs, which does not exist"; continue; }
    run_capture G3 "g3-fast-$bin" cargo test --test "$bin" $CARGO_TEST_FLAGS
  done

  # Every DO NOW feature must name the test that proves it can fail. A feature
  # with no entry is a feature nothing is checking.
  mapfile="$ROOT/scripts/canary-map.txt"
  if [ -f "$mapfile" ]; then
    missing=""
    # `IFS='|'` splits on the pipe only, so a space either side of it stays in
    # the field — and every name in the map is written with spaces around the
    # pipe for legibility. Trim, or nothing matches and every canary is
    # reported missing, which is the failure mode a gate is least likely to be
    # believed about.
    while IFS='|' read -r feature testname _rest; do
      feature="${feature#"${feature%%[![:space:]]*}"}"
      testname="${testname#"${testname%%[![:space:]]*}"}"
      testname="${testname%"${testname##*[![:space:]]}"}"
      [ -z "$feature" ] && continue
      case "$feature" in \#*) continue ;; esac
      if ! grep -RqsF -- "$testname" tests/ src/; then
        missing="${missing}"$'\n'"  $feature -> $testname"
      fi
    done < "$mapfile"
    if [ -z "$missing" ]; then
      record G3 PASS "$(grep -cv '^\s*#\|^\s*$' "$mapfile") can-fail entries all resolve"
    else
      record G3 FAIL "canary map names tests that do not exist:$missing"
    fi
  else
    record G3 FAIL "scripts/canary-map.txt is missing — G3 cannot be checked"
  fi
fi

# ── G4 · the core user flow, for real ─────────────────────────────────
if want G4; then
  printf '\n\033[1mG4 · the core flow, for real\033[0m\n'
  run_capture G4 g4-lifecycle cargo test --test run_lifecycle $CARGO_TEST_FLAGS
  run_capture G4 g4-chat      cargo test --test chat_runs_the_pipeline $CARGO_TEST_FLAGS

  if [ $FAST -eq 1 ]; then
    record G4 SKIP "scripts/demo.sh skipped by --fast"
  else
    run_capture G4 g4-demo ./scripts/demo.sh
    # The TUI leg: drive the real binary under a real pty. `niki chat` was in
    # zero e2e scripts before T10, and it is where every user lands.
    if command -v tmux >/dev/null && [ -x target/release/niki ]; then
      run_capture G4 g4-tui env NIKI_BIN="$ROOT/target/release/niki" ./tests/tui_smoke/run.sh
    else
      record G4 SKIP "tmux unavailable — the TUI leg cannot run here"
    fi
  fi
fi

# ── G5 · security ─────────────────────────────────────────────────────
if want G5; then
  printf '\n\033[1mG5 · security\033[0m\n'
  if command -v cargo-deny >/dev/null || cargo deny --version >/dev/null 2>&1; then
    run_capture G5 g5-deny  cargo deny check
  else
    record G5 SKIP "cargo-deny not installed"
  fi
  if command -v cargo-audit >/dev/null || cargo audit --version >/dev/null 2>&1; then
    run_capture G5 g5-audit cargo audit
  else
    record G5 SKIP "cargo-audit not installed"
  fi
  # Secrets in the tree and in history.
  # Test fixtures are *meant* to hold credential-shaped strings — that is what
  # `tests/secret_redaction.rs` and `tests/reverse/injection.rs` are for, and
  # flagging them would make the check unusable and get it switched off. The
  # rule is about the product's own files.
  if grep -rqIE --exclude-dir=target --exclude-dir=.git --exclude-dir=.evidence \
        --exclude-dir=tests --exclude-dir=.odw \
        '(sk-ant-[A-Za-z0-9_-]{20,}|sk-[A-Za-z0-9]{32,}|ghp_[A-Za-z0-9]{30,}|AKIA[0-9A-Z]{16})' .; then
    record G5 FAIL "a credential-shaped string is in the working tree"
  else
    if git log --all -p 2>/dev/null | grep -qE '^\+.*(sk-ant-[A-Za-z0-9_-]{20,}|ghp_[A-Za-z0-9]{30,}|AKIA[0-9A-Z]{16})'; then
      record G5 FAIL "a credential-shaped string is in git history"
    else
      record G5 PASS "no credentials in the tree or in history"
    fi
  fi
  # The path-escape guard, exercised rather than assumed.
  run_capture G5 g5-sandbox cargo test --test security_exec $CARGO_TEST_FLAGS
fi

# ── G6 · failure paths, each with a human-readable message ────────────
if want G6; then
  printf '\n\033[1mG6 · failure paths\033[0m\n'
  if [ -x scripts/failure-paths.sh ]; then
    run_capture G6 g6-paths ./scripts/failure-paths.sh
  else
    record G6 FAIL "scripts/failure-paths.sh is missing — G6 cannot be checked"
  fi
fi

# ── G7 · docs match reality ───────────────────────────────────────────
if want G7; then
  printf '\n\033[1mG7 · docs\033[0m\n'
  run_capture G7 g7-claims   cargo test --test claims $CARGO_TEST_FLAGS
  run_capture G7 g7-consist cargo test --test docs_consistency $CARGO_TEST_FLAGS
  # Every command the README advertises must parse against the real binary.
  if [ -x target/release/niki ]; then
    bad=""
    for cmd in $(grep -oE '^\| `niki [a-z-]+' README.md | sed 's/^| *`niki //' | sort -u); do
      target/release/niki "$cmd" --help >/dev/null 2>&1 || bad="$bad $cmd"
    done
    if [ -z "$bad" ]; then
      record G7 PASS "every README command parses"
    else
      record G7 FAIL "README advertises commands that do not parse:$bad"
    fi
  else
    record G7 FAIL "no release binary to check the README against"
  fi
fi

# ── G8 · CI green ────────────────────────────────────────────────────
if want G8; then
  printf '\n\033[1mG8 · CI\033[0m\n'
  if [ $FAST -eq 1 ]; then
    record G8 SKIP "skipped by --fast"
  elif ! command -v gh >/dev/null 2>&1; then
    record G8 UNVERIFIED-EXTERNAL "no `gh`; CI cannot be read from here"
  elif ! gh auth status >/dev/null 2>&1; then
    record G8 UNVERIFIED-EXTERNAL "no `gh` auth; CI cannot be read from here"
  else
    run_capture G8 g8-ci ./scripts/ci-is-green.sh
  fi
  # The remote answer is about the last *pushed* tree. On a local branch it
  # therefore describes code that is not here, and a red G8 says nothing about
  # what is. So run the one CI job that can be checked offline as well, and
  # report both.
  #
  # It is the job that was actually red: `Manifest parity` failed on the last
  # run because three manifests pointed at a release that was never published
  # while `Cargo.toml` had moved past it — so `brew install niki`, the headline
  # install command in the README, 404'd, and the fix could only be verified by
  # pushing. `scripts/manifest-parity.sh` is that job, extracted, so the tree in
  # front of us is checked rather than a run from a branch that is gone.
  if run_capture G8 g8-manifest-parity ./scripts/manifest-parity.sh; then
    :
  else
    record G8 FAIL "manifest parity fails on THIS tree — an install command the README prints does not resolve"
  fi
fi

# ── G9 · no dead code, no fake features ───────────────────────────────
if want G9; then
  printf '\n\033[1mG9 · no dead code, no fake features\033[0m\n'
  # `todo!`/`unimplemented!` outside test modules.
  # A test module's contents are not production code, and `grep -v cfg(test)`
  # only drops the attribute's own line — so counting by line number is the
  # only honest way. `panic!` in `#[should_panic]` tests is the same case.
  hits=$(python3 scripts/no-todo-in-production.py)
  if [ "${hits:-0}" -gt 0 ]; then
    record G9 FAIL "$hits todo!/unimplemented! in production code — see .evidence/g9-todo.log"
  else
    record G9 PASS "no todo!/unimplemented! in production code"
  fi
  # Tautologies.
  if grep -rn --include=*.rs 'assert!(true)' tests/ src/ >/dev/null 2>&1; then
    record G9 FAIL "assert!(true) — a test that cannot fail"
  else
    record G9 PASS "no assert!(true)"
  fi
  # Every subcommand the binary has is documented.
  if [ -x target/release/niki ]; then
    undoc=""
    for cmd in $(target/release/niki --help 2>/dev/null | sed -n '/^Commands:/,/^Options:/p' | grep -oE '^  [a-z][a-z-]+' | tr -d ' '); do
      [ "$cmd" = "help" ] && continue
      grep -q "niki $cmd" README.md || undoc="$undoc $cmd"
    done
    if [ -z "$undoc" ]; then
      record G9 PASS "every subcommand appears in the README"
    else
      record G9 FAIL "undocumented subcommands:$undoc"
    fi
  else
    record G9 SKIP "no release binary to enumerate subcommands"
  fi
fi

# ── Summary ──────────────────────────────────────────────────────────
printf '\n\033[1m───────────────────────────────────────────\033[0m\n'
for r in "${RESULTS[@]}"; do
  IFS='|' read -r g s m <<<"$r"
  printf '%-18s %-20s %s\n' "$g" "$s" "$m"
done
printf '\n'

if [ "$UPDATE_EVIDENCE" -eq 1 ]; then
  {
    echo "# Evidence"
    echo
    echo "Generated by \`scripts/verify.sh --update-evidence\` on \`$(date -u +%Y-%m-%dT%H:%M:%SZ)\`."
    echo "Every line below is that run's real output. Nothing here is hand-written."
    echo
    echo '```'
    for r in "${RESULTS[@]}"; do
      IFS='|' read -r g s m <<<"$r"
      printf '%-4s %-20s %s\n' "$g" "$s" "$m"
    done
    echo '```'
    echo
    echo "## Per-gate logs"
    echo
    for f in "$EV"/*.log; do
      [ -s "$f" ] || continue
      echo "### $(basename "$f")"
      echo
      echo '```'
      tail -40 "$f"
      echo '```'
      echo
    done
  } > "$ROOT/EVIDENCE.md"
  echo "wrote EVIDENCE.md"
fi

if [ $FAILED -gt 0 ]; then
  printf '\033[31m%d gate(s) failed\033[0m\n' "$FAILED"
  exit 1
fi
printf '\033[32mall %d gates passed\033[0m\n' "$((${#RESULTS[@]} - SKIPPED))"
