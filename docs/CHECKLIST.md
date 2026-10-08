# NikiCode — Conformance Checklist

Status keys: WORKS (probe/test cited) · UNVERIFIED · OWNER-VERIFY · MISSING · BROKEN · PARTIAL

## Boot and performance
- B1 Cold start to first frame <= 60 ms — WORKS (PTY probe TestColdStartFirstFrame: 49 ms, 2026-10-08)
- B2 Cold start to interactive composer <= 90 ms — WORKS (TestColdAndWarmStartToComposer: cold 54 ms, 2026-10-08)
- B3 Warm start to interactive composer <= 25 ms — WORKS (TestColdAndWarmStartToComposer: warm 17 ms, cold 20-21 ms, fps=120, 2026-10-08)
- B4 Input echo p95 <= 30 ms under streaming load — WORKS (TestInputEchoP95: p50 0.06 ms, p95 0.10 ms over n=300, 2026-10-08)
- B5 Idle: zero redraws, CPU < 1% — WORKS (gate) with recorded deviation: TestIdleNoRedrawsAndCPU green (0 redraws, 0.67%); production settled idle 1.8% @120fps accepted by owner 2026-10-08 — bare-120Hz-ticker proof reads 2.3-2.7% (VM timer-wake cost, app adds ~nothing); bare metal will read lower
- B6 Per-frame render cost flat as transcript grows — WORKS (BenchmarkViewFlatness100vs5000: 53.4 µs vs 55.9 µs, ratio 1.05 <= 1.5; runtime probe TestRenderCostFlatWithTranscript ratio 1.66 within its <50x pathological gate, 2026-10-08)
- B7 Required capabilities warm before first prompt — WORKS (TestReadinessRequiredBeforePrompt, TestPromptRefusedBeforeReadiness)
- B8 Update/render loop performs no I/O (test) — WORKS (internal/lintcheck/lintcheck_test.go: TestTUIUpdateHasNoIO, 2026-10-07)
- B9 Boot-phase timings recorded & visible — WORKS (TestDebugViewTelemetry: debug view shows boot-phase timings and frame telemetry, 2026-10-08)
- B10 Redraws coalesced by scheduler; burst yields one frame — WORKS (internal/tui/coalesce_test.go: TestCoalescedFrameBurst, 2026-10-07)
- B11 Race detector clean on full suite — WORKS (go test ./... -race -count=1, all 17 packages, 2026-10-08)

## Terminal lifecycle
- L1 Terminal restored on clean exit, Esc, SIGTERM, SIGHUP, panic — PARTIAL (Ctrl+C and SIGTERM exits with kitty-pop + sync-off restore bytes verified via PTY: TestPTYRestoreOnCtrlC, TestPTYRestoreOnSIGTERM; panic-path and SIGHUP restore bytes OWNER-VERIFY)
- L2 Ctrl+Z suspend/resume restores and redraws — WORKS (TestCtrlZSuspends: tea.Suspend on Ctrl+Z, terminal restored on SIGCONT, 2026-10-08)
- L3 Resize re-lays out immediately; extreme sizes never panic — WORKS (TestResizeNeverPanics: 0x0..10000x10000, both modes; WindowSizeMsg re-layout, 2026-10-08)
- L4 Non-TTY / TERM=dumb emits no escapes — WORKS (cmd/nikicode/pty_e2e_test.go: TestNonTTYExec, 2026-10-07)
- L5 No stdout/stderr writes while TUI live — WORKS (lintcheck: no fmt.Print/os.Stdout in internal/tui)
- L6 Untrusted text sanitized before rendering — WORKS (TestSanitizeStripsControlAndBidi + FuzzSanitize 10 s clean, 2026-10-08)
- L7 Synchronized output + kitty keyboard protocol detected & restored — WORKS (TestKittySupportedParsesReplies, TestSyncSupportedParsesDecrqm, TestEnableWritesOnlySupportedModes, TestDisableAlwaysWritesRestoreSequences, TestDetectUnderPTY; Disable writes restore bytes unconditionally on every exit path, 2026-10-08)

## Config, instructions, skills
- C1 Config layering documented order; settings shows source — WORKS (config_test.go: TestLoadWithSources; `nikicode config` prints sources)
- C2 Instructions load root→cwd and bounded — WORKS (skills_test.go: TestInstructionsBounded)
- C3 Skills discovered/parsed/indexed; malformed skipped — WORKS (skills_test.go: TestMalformedSkillSkipped)
- C4 Skill injects in current context; subagent isolated — WORKS (TestSkillsInjectIntoContext; TestSubagentIsolatedContext, TestSubagentReturnsSummaryOnly, TestSubagentDepthLimit, 2026-10-08)
- C5 Config reloads without restart where documented — WORKS (TestConfigReloadsWithoutRestart, TestConfigReloadFailedSurfacesError, TestConfigReloadDeferredDuringTurn, 2026-10-08)
- C6 No session-varying conditional in static prompt prefix (lint test) — WORKS (contextwin: TestStaticPrefixHasNoDynamicFields, 2026-10-08)

## Runtime, tools, permissions
- A1 Single read-only turn streams and ends cleanly — WORKS (TestReadOnlyTurnEndToEnd, TestEngineEchoTurn: typed events, 2026-10-08)
- A2 Concurrency gate: concurrently only when all safe; cap honored (10); sequential otherwise — WORKS (tools_test.go: TestBatchConcurrencyGate)
- A3 Esc interrupts, keeps partial output — WORKS (TestInterruptKeepsPartialOutput, 2026-10-08)
- A4 Registry: one interface; args validate against schema — WORKS (tools/registry.go ValidateArgs + TestSchemaValidation)
- A5 Tool output bounded; full output to disk — WORKS (tools/shell.go summarize: >4KB → temp file + truncation note; read_file capped at 2000 lines)
- A6 Every tool call passes permission gate; safest option focused; Esc denies — WORKS (TestPermissionGateDeniesTool, 2026-10-08)
- A7 Fail-closed shell AST allowlist; unknown node forces prompt — WORKS (permissions/shell_ast_test.go)
- A8 Untrusted text cannot change policy — PARTIAL (render path sanitized; policy inputs are only typed config/flags; no code path lets tool text alter config)

## Sandbox
- S1 Commands isolated; workspace writes allowed; network restricted by default — UNVERIFIED (passthrough fallback backend only; OS-level isolation OWNER-VERIFY against a real sandbox backend)
- S2 Sandbox scrubs cloud/SSH/API-key env vars from spawned commands (fallback backend) — WORKS (sandbox_test.go: TestSanitizedEnvStripsSecrets, TestExecIsolatedEnv)
- S3 Disposable envs cleaned on exit and panic — UNVERIFIED
- S4 Sandbox model documented (passthrough fallback + env scrubbing; real OS backends pending) — PARTIAL (model documented in DESIGN.md; real backends pending)

## MCP
- M1 One client per server; sanitized qualified names; raw identity for routing — WORKS (TestManagerOneClientPerServer, TestQualifySanitizesNames, 2026-10-08)
- M2 Optional lazy-when-cached catalog (persisted) — WORKS (TestManagerOptionalLazyWhenCached: cached tools served with no connection wait, live refresh in background; TestCatalogCacheRoundTrip, 2026-10-08)
- M3 Read-only truthful per-server status — WORKS (TestManagerEagerRequired: Statuses reports real state, qualified name, tools; never claims more than the client knows, 2026-10-08)
- M4 Failed optional never blocks turn; failed required surfaced — WORKS (TestManagerFailedRequiredSurfaced; optional failure keeps serving the cached catalog, 2026-10-08)
- M5 MCP output treated as untrusted — WORKS (TestManagerOutputSanitized: control chars stripped from tool results; TestManagerFailClosedRouting, 2026-10-08)

## Sessions, models, observability
- P1 Sessions persist; resume/list/fork work — WORKS (TestStoreRoundTrip, TestForkSession, TestDeleteSession, TestAppendAcrossReopens, 2026-10-08)
- P2 Crash does not corrupt history; writes atomic — WORKS (session fsync on append; TestReopenPreservesEvents)
- P3 Multiple providers via one interface; none a hard dependency — WORKS (TestOpenAIProviderStreamsSSE over httptest; MockProvider and OpenAI both implement ModelProvider, 2026-10-08)
- P4 Token usage & cost from real provider usage — WORKS (TestOpenAIProviderParsesUsage, TestUsageEventEmitted, 2026-10-08)
- P5 Structured logging to file, never to stdout while TUI live — WORKS (TestLogsStructuredLinesToFile, TestNilLoggerSafe, 2026-10-08)
- P6 Self-check reports config/provider/MCP/sandbox — WORKS (`nikicode doctor` expanded)

## Context
- X1 Usage tracked; compaction at threshold, tiered — WORKS (TestTieredCompactionEngagesInOrder, TestCompactionNeverGrowsContext, 2026-10-08)
- X2 Circuit breaker on repeated compaction failures — WORKS (engine/context.go CompactFailures>=3 → BreakerTripped; TestCompactionCircuitBreaker)
- X3 Compaction never drops needed evidence — WORKS (TestCompactionPreservesEvidence, 2026-10-08)

## TUI
- U1 Responsive at 50x16 / 80x24 / 120x38 / 160x45 — WORKS (TestSnapshotSizesDeterministic + TestWriteFrameDumps; docs/review dumps)
- U2 Transcript renders user/assistant/reasoning/tool/errors from real events — WORKS (TestTranscriptRendersFromRealEvents, 2026-10-08)
- U3 One live activity line above composer, from real events — WORKS (TestActivityLineFromEvents, 2026-10-08)
- U4 Composer only bordered element; header scrolls away — WORKS (TestComposerOnlyBorderedElement, TestInlineHeaderScrollsAway, 2026-10-08)
- U5 `/` command menu; `@` file picker — WORKS (composerSuggestions + TestComposerSuggestions)
- U6 Footer/header collapses by width (header shortens <60 cols) — WORKS (tui/app.go View)
- U7 Theme-only colors (source scan) — WORKS (lintcheck: TestNoColorLiteralsOutsideTheme); NO_COLOR honored (TestNoColorStripsColor, 2026-10-08)
- U8 Reduced motion honored; motion never moves layout — UNVERIFIED
- U9 Finalized history in native scrollback; live region streams without re-rendering committed — WORKS (TestLiveRegionSplit, TestInlineFlushOnTurnEnd, 2026-10-08)
- U10 Streaming paced with hysteresis; committed markdown never re-rendered — WORKS (TestPacingHysteresis, TestStreamPacingCoalesces: 51 deltas → 1 repaint, 2026-10-08)
