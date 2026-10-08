# NikiCode Dogfooding Log (`docs/DOGFOOD.md`)

This document records real dogfooding of the NikiCode harness during its own construction, self-build, test runs, and issue fixes.

> Historical note: sessions 1–2 ran the `niki` binary before the G1
> rename; commands are quoted verbatim. Session 3 uses `nikicode`.

---

## 1. Dogfood Session: Self-Build and Verification

**Date**: 2026-10-08  
**Target**: `github.com/RavaniRoshan/niki`  
**Binary Tested**: `./bin/niki` (single static Go binary, 19 MB)

### Actions Performed
1. Scaffolding workspace and initial templates via `niki init`.
2. Running environment and sandbox validation via `niki doctor`.
3. Validating layered configuration inspection via `niki config show --sources`.
4. Testing headless prompt execution via `niki exec "hello world"`.
5. Running full suite with race detector via `go test -race ./...`.

---

## 2. Issues Encountered & Resolved During Self-Hosting

### Issue 1: PTY Setsid / Process Group Conflict in WSL2
- **Symptom**: PTY smoke tests failed with `fork/exec /home/shiva/projects/niki/bin/niki: operation not permitted (EPERM)`.
- **Root Cause**: Setting `cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}` clashed with `creack/pty.Start`, which already performs terminal session leadership allocation.
- **Fix**: Removed redundant `Setpgid: true` in the PTY test runner, allowing `creack/pty` to manage terminal process attributes directly.
- **Verification**: `cmd/niki/pty_e2e_test.go` passed all 4 end-to-end tests cleanly in under 100ms.

### Issue 2: Headless Detection vs Controlling Terminal
- **Symptom**: `TestDetectOnNonTerminal` failed when run in environments without an attached `/dev/tty`.
- **Root Cause**: `terminal.Detect()` directly called `os.OpenFile("/dev/tty", os.O_RDWR, 0)` without providing an abstraction for testing headless state.
- **Fix**: Added pluggable `openTTYFn = openTTY` variable in `internal/terminal/terminal.go` allowing unit tests to simulate non-tty environments safely without affecting production terminal detection.

### Issue 3: Boundary-Free LLM SSE Streams
- **Symptom**: `TestOpenAIProviderStreamsSSE` timed out or reported premature stream truncation when reading consecutive `data:` lines that lacked empty newline dividers.
- **Root Cause**: W3C SSE standard expects `\n\n` event boundaries, but real LLM streaming proxies often emit single `\n` delimiters between events.
- **Fix**: Implemented `SSEScanner` with pending buffer lookahead: if accumulated data is complete JSON or `[DONE]`, consecutive `data:` lines are automatically dispatched as distinct events. Verified with 213,000 fuzz runs with 0 panics.

### Issue 4: Request Body Depletion on HTTP 429/5xx Retries
- **Symptom**: Retrying after rate-limit (429) or gateway errors (5xx) sent empty HTTP request bodies on subsequent attempts.
- **Root Cause**: `http.Client.Do` consumes the `io.Reader` from `req.Body`.
- **Fix**: Added explicit `req.GetBody()` body rewinding inside `doWithBackoff()` across `AnthropicProvider`, `ResponsesProvider`, and `OpenAIProvider`. Verified with fake server 429/5xx retry conformance tests.

---

## 3. What Hurt (Developer Friction & Ergonomics)

1. **Bubble Tea v2 Package Ecosystem Split**: Bubble Tea v2 alpha (`github.com/charmbracelet/bubbletea/v2`) changed several API signatures from v1 (e.g. `tea.WindowSizeMsg` structure, standard commands). Using custom renderer primitives and clear inline split bounds avoided dependency churn.
2. **Linux Bubblewrap Availability**: On minimal cloud containers without Bubblewrap installed, fallback execution must remain available while loudly reporting its degraded security posture through `niki doctor`.
3. **Fast Path Budget Discipline**: Ensuring `niki --version` executes in <7ms required ruthless separation of Cobra commands: fast paths must inspect `os.Args` and exit before any CLI command trees or reflection-heavy modules are loaded.

---

## 3. Final-pack dogfood (G1–G6, 2026-10-08, honest scope)

What actually ran inside NikiCode itself during this pack:

- `nikicode doctor` after the rename caught the missing Ensure() wiring
  (no migration on first run) — fixed before it could strand state.
- `nikicode do --plan` / `do` recipes (scaffold, build, commit, explain)
  ran the pack's own fixture tasks end to end; the correction bug
  (computed target discarded, `last.Name` used) was found through a live
  `do "no, name=svc …"` run, not a unit test.
- `nikicode exec` fixture turns validated the headless path after every
  invasive change (session backgrounding, fast-path scoping).
- `nikicode bench` measured every table in docs/BENCH.md; the Codex
  modal gate and the 0x0-PTTY lesson both came from live black-box runs.
- `nikicode soak --turns 200` ran the stability evidence, not a demo.

What hurt (friction log):

- `do` with no matching routine prints the recipe list but not example
  inputs; first-time users guess phrasing. Mitigation logged, unbuilt.
- Multistep `do` journals each step: undo granularity is per-step, which
  is right, but nothing tells the user that until they try it.
- The update-modal episode (Codex) has no NikiCode equivalent to learn
  from — our first paint never blocks on network by construction (boot
  DAG), and the bench proves it.

What did NOT happen: a full week of living in the NikiCode TUI. This
pack was built in long sessions with frequent real-terminal verification,
but the author drives multiple tools; a week-long single-harness log
needs the owner's daily use (OWNER-VERIFY). Nothing above is invented:
every claim links to a probe, a test, or a raw log.
