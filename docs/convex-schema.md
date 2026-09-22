# Convex Schema — NIKI mirror (2026-09-18, DRAFT — validate at implementation)

Conventions: `by_a_b` index names; validators on all args; `_id/_creationTime` automatic; ≤32 indexes/table, ≤16 fields/index; no `_`-prefix custom fields; `take/paginate`, never `collect()` unbounded.

## Tables

- `users { tokenIdentifier: string, subject: string, email?: string }` — idx `by_token`.
- `organizations { name: string }`.
- `memberships { orgId: Id<organizations>, userId: Id<users>, role: "admin"|"member" }` — idx `by_org_user`.
- `projects { orgId: Id<organizations>, name: string, repoUrl?: string }` — idx `by_org_name`.
- `runs { projectId: Id<projects>, taskUuid: string (unique), branch?: string, commit?: string, status: "queued"|"planning"|"coding"|"testing"|"reviewing"|"revision_required"|"approved"|"committed"|"failed"|"cancelled", verdict?: string, rounds: number, topology?: string, risk?: string, provider?: string, model?: string, inputTokens: number, outputTokens: number, costUsd: number, latencyMs: number, configHash?: string, repoSha?: string, createdBy: string, createdAt: number, updatedAt: number }` — idx `by_project_status`, `by_taskUuid`.
- `stages { runId: Id<runs>, seq: number, role: string, provider?: string, model?: string, status: string, attempt: number, inputTokens: number, outputTokens: number, costUsd: number, latencyMs: number, retries: number, startedAt: number, endedAt?: number }` — idx `by_run_seq`.
- `events { runId: Id<runs>, seq: number, kind: string, level: string, message: string, createdAt: number }` — idx `by_run` (coarse only; cap count, paginate, archive).
- `revisions { runId: Id<runs>, round: number, fromStage: string, feedbackDigest: string }` — idx `by_run_round`.
- `reviews { runId: Id<runs>, verdict: string, scores?: any, issues?: any }` — idx `by_run`.
- `artifacts { runId: Id<runs>, kind: string, path?: string, storageId?: Id<"_storage">, sha256?: string, size?: number }` — idx `by_run_kind`.
- `branches { runId: Id<runs>, name: string, commit: string, parent: string }` — idx `by_run`.
- `providers { name: string, models: any, priceAsOf?: string }` — catalog only, NO keys.
- `usage { runId: Id<runs>, kind: string, tokens: number, costUsd: number }` — rollups, idx `by_run`.
- `costs { runId: Id<runs>, totalUsd: number, byStage: any }` — idx `by_run`.
- `budgets { orgId?: Id<organizations>, projectId?: Id<projects>, capUsd: number }`.
- `workers { key: string, runId?: Id<runs>, lastSeen: number, isOnline: boolean }` — idx `by_isOnline` (+ Presence component alternative).
- `notifications { runId: Id<runs>, userId: Id<users>, kind: string, status: "pending"|"sent"|"failed", createdAt: number }`.
- `settings { key: string, value: any }` — flags/kill-switches.

## Lifecycle / retention / authz

- Status transitions only via CAS mutation (expected-predecessor check); terminal flips exactly-once; idempotency keys (`taskUuid/stageAttemptId`) set-if-unset; HTTP callers implement key ledger (`begin→inflight ~60s→done ~24h`, per-tenant scope, server expiry).
- Workflow history NOT auto-cleaned → `cleanup()`/cron paginate-clean; events/usage archived to external storage on schedule; list docs <10KB (summary + detail on demand).
- Authz: every public fn `getUserIdentity()` first + membership scoping (`orgId/projectId` via idx); `convex-helpers` wrappers defense-in-depth; sensitive (budgets/costs/usage/providers) internal-only; scheduled targets take explicit `userId/orgId/runId` + re-authorize (auth not propagated).
- Blobs: `Id<"_storage">` refs only; upload `generateUploadUrl`(1h)→POST(2-min)→save Id (large) or HTTP-action `store(blob)` (≤HTTP cap, +CORS); `getUrl` bearer (delete-only revoke; gate sensitive via HTTP action or R2 expiring); metadata via `db.system`; serve ≤20MB via HTTP action.
- Limits snapshot 2026-09-18 (re-verify): txn 16MiB r/w, 32k scanned, 16k written, 16MiB return; args 16MiB (Node 5MiB); doc 1MiB/1024f/16d/8192arr; user-code 1s; 1000 children/mutation; class concurrency S16 16/4MiB … D2048 2048/64MiB.
