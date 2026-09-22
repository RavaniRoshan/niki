# Convex Sequence Diagrams — NIKI (2026-09-18)

Conventions: `CLI` = Rust worker (authoritative); `CVX` = Convex mirror; `DASH` = future Next.js dashboard. All mirror writes best-effort + idempotent; local always completes.

## niki run (mirror)

```
CLI: Task{Uuid} → save_to_disk → CVX:createRun{taskUuid} (keyed, dedup) ──▶ runs[queued]
CLI: planning/coding/testing/reviewing → CVX:stageTransition{runId,seq,role,status} (CAS)
CLI: branch niki/<8hex> + report → CVX:completeRun{committed|failed} (terminal-once)
DASH: useQuery(runs/stages) ← live updates (coarse only)
```

## Remote run visibility / dashboard realtime update

```
DASH mounts → subscribe(api.runs.get{taskUuid}) → CVX pushes on stageTransition
CLI writes artifact meta {storageId,sha256} → DASH fetches bytes via storage URL (≤cap) else Git/S3
```

## Reconnect (CLI offline 2h)

```
CLI offline → local completes, spool outbox (persist high-water AFTER flush)
CLI online → flush spool with same keys → CVX replays originals (no dupes) → DASH catches up
Conflict rule: CLI wins; CVX never rewrites task.json
```

## Cancellation (user cancels during review)

```
CLI: cancel → local persist Cancelled → CVX:cancelRun (CAS to cancelled; pre-start scheduler.cancel)
In-flight action finishes, children suppressed; late stage result discarded (terminal-once)
```

## Failed worker (dies after running)

```
CLI dies in `running` → heartbeat lastSeen stalls → CVX sweeper cron (by_isOnline, 100/batch, runAfter(0))
→ marks stale/failed (never auto-commits) → operator/loop requeues new attempt (new stageAttemptId)
```

## Reviewer revision loop

```
Reviewer RevisionNeeded → CLI loops to Coder (≤max_revision_rounds=3) → CVX:revision{round,feedbackDigest} + stageTransition
Critic once post-loop → at most one Reviewer retry → CVX:review{verdict,scores}
Approved → committed (+branch meta); Rejected/Failed → terminal failed
```

## Completed run

```
CLI: safety_proof + report + costs → CVX:completeRun{commit,totals} + artifacts(meta) + usage rollup
DASH: status committed, costs/stages/artifacts visible; raw logs via local/Git/storage (not CVX docs)
```
