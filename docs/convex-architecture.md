# Convex Architecture — NIKI (2026-09-18)

## Principle

Local-first: Rust execution/sandbox/Git is authoritative and offline-capable. Convex (if enabled) is a best-effort async mirror for visibility/history. No mandatory cloud.

## System diagram

```
┌─ LOCAL (authoritative) ─────────────────┐    ┌─ CONVEX (optional mirror) ──┐    ┌─ FUTURE UI ──────┐
│ Rust CLI/worker                          │    │ convex/: runs/stages/events │    │ Next.js dashboard │
│ run/plan → orchestration → sandbox →     │───▶│ artifacts(meta)/projects/   │───▶│ React client      │
│ git niki/<8hex> → report/audit/costs    │    │ memberships/heartbeats/usage │    │ useQuery live     │
│ .niki/tasks/<uuid>/ = truth; spool+retry │    │ NO logs/blobs/keys          │    │ run/stage/costs   │
└──────────────────────────────────────────┘    └─────────────────────────────┘    └───────────────────┘
```

## Data flow

1. Local commit points (save_to_disk, stage close, branch commit) enqueue mirror ops with idempotency keys (`taskUuid/stageAttemptId`).
2. Rust adapter (`src/control_plane/`, feature `convex-mirror`) sends HTTP `POST /api/mutation` (recommended; `reqwest`, own retry + key ledger) — WS `convex-rs` only for live-tail processes with `set_auth_callback`.
3. TS functions validate + CAS (`read expected-predecessor → patch`, serializable OCC single txn); terminal flips exactly-once (`onComplete`-style).
4. Dashboard subscribes (`useQuery`, `watch_all` where consistent); coarse status only.
5. Failure: warn + spool → backfill on reconnect; CLI wins conflicts; Convex never rewrites `task.json`.

## Modes / deployment

- `LOCAL`: no account/network; full function.
- `CONNECTED` (opt-in `NIKI_CONTROL_PLANE=local|mirror`, `NIKI_CONVEX_URL/_TOKEN` — names TBD vs `config/types.rs` + `niki.example.toml`): mirror + dashboard + history.
- Cloud: dev + one prod (`npx convex deploy` from CI), Clerk-class auth, usage caps.
- Self-host: Compose (`:3210/:3211/:6791`), PG/MySQL+S3 prod, admin key, Auth-manual + CLI gaps — verify GA/parity before committing (unresolved).

## State machine

`queued→planning→coding→testing→reviewing→{revision_required→coding…|approved→committed|failed|cancelled}`. Single-owner transitions; stale workers via heartbeat TOO_OLD sweeper or `@convex-dev/presence` (beats schedule no work, subs fire join/leave); `cancel()` pre-start only; `_scheduled_functions` 7d observable.

## Realtime rules

Coarse only (`run.status`, `stage.status`, `isOnline`, workflow status). Logs/counters batched/aggregated, raw paged. Isolate hot `lastSeen` docs; shared cache-key list queries; `take/paginate`, never `collect()`; Workpool `maxParallelism` caps.

## Security

Sandbox = execution boundary. Per-fn `getUserIdentity` + membership scoping + helper wrappers; sensitive tables internal-only; scheduled targets explicit IDs + re-authz; service = shared-secret-gated public fns or Bearer JWT (no built-in service identity); provider keys NEVER in Convex (refs only); host keys in keyring > env > file; admin keys never on workers.
