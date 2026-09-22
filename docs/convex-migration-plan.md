# Convex Migration Plan — NIKI (2026-09-18)

Rule: incremental, dual-read, backfill, verify, narrow. Risky changes behind `settings` flags. Existing gates (`fmt --check`, `clippy --all-targets` warning-free, `test --verbose`, `deny`/`audit`, mock-LLM worktree e2e) stay green every phase.

## P0 Audit — DONE

This report + `research/niki-convex-integration-deep.md`. Confirm via `rg` (daemon/queue/serve/Convex refs), read loop-body/sandbox-lifecycle/Tester-exec path, `[session]/[compaction]/[mcp]` wiring truth.

## P1 PoC (run-header mirror)

- Files: new `convex/schema.ts` (runs/events minimal), `convex/runs.ts` (create/update/complete CAS), new Rust `src/control_plane/{mod.rs,adapter.rs,spool.rs}` (feature `convex-mirror`, `reqwest`), `convex/*.test.ts`.
- Deps: `convex` npm (TS fns), `reqwest/serde/uuid` (Rust; add `convex` crate only if WS needed).
- Config: `NIKI_CONTROL_PLANE=local|mirror` + `NIKI_CONVEX_URL/_TOKEN` (verify vs `config/types.rs` + `niki.example.toml` first).
- Tests: `convex-test` (schema-required, `withIdentity` ±authz) + Rust wiremock (duplicate/offline/reconnect) + existing suite.
- Risks: adapter retry/auth; rollback: flag off, delete `convex/`, zero core diffs.
- Done: local run mirrors header with key dedup; dashboard query reads it.

## P2 Run metadata + projects/auth

Add `projects/memberships/users`, wire Clerk/custom-JWT (product call), `ConvexProviderWithClerk` later for dashboard. Tests: owner/other/unauth negative probes. Rollback: flag off.

## P3 Stage/event state

Add `stages/revisions/reviews/events` + CAS transitions + terminal-once; batch coarse events (no per-token rows). Tests: transition matrix, double-complete no-op, cancel-during-review. Rollback: stop writing stages (runs mirror continues).

## P4 Realtime dashboard (read-only)

New `dashboard/` Next.js: `useQuery(api.runs.list/get)` list/detail, `skip` when idle; WS `.convex.cloud`, HTTP actions `.convex.site`. Tests: manual sub-update checks (`convex-test` has no cron/limits). Rollback: dashboard offline, CLI unaffected.

## P5 History/retention

Pagination + archival cron + `workflow.cleanup()` equivalent for history tables; retention policy doc; storage-vs-S3 split (1MiB/20MB caps). Use `@convex-dev/migrations` (batches ~100, resumable, `dryRun/reset/getStatus/cancel/runToCompletion`; shrink batch on OCC conflicts). Rollback: pause cron.

## P6 Coordination (DEFERRED until fleet demand)

Scheduler/Workpool (`maxParallelism`, retries+jitter, `NonRetryableError`) / Workflow durable steps (IDs-only, stable impl) / presence heartbeat + sweeper (`by_isOnline` 100/batch `runAfter(0)`). Tests: TOO_OLD sweeper, resume-from-journal, at-most-once cancel. Rollback: disable schedules; local still complete.

## P7 Teams/RBAC (DEFERRED)

Orgs/RBAC, Clerk webhook (Svix) or post-login mutation + `backfillUsers`, custom roles (Business) / SSO (Business/Enterprise) / team audit (paid) only if needed; Enterprise S3 audit if compliance demands. Rollback: single-user mode.
