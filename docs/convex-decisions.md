# Convex Decisions (ADR) — NIKI (2026-09-18)

## 1. Why Convex (if at all)

Only for optional run/stage visibility + history + future collaboration without building custom WS/pub-sub/gateway infra. Reactive queries + TS functions + scheduler/storage in one SDK; Rust HTTP one-shot writes, Next.js subscribes.

## 2. Why not alternatives

- Keep-local: wins today (zero bill/ops, offline, private) — remains the default; forfeits remote visibility (accepted until proven needed).
- SQLite/PocketBase: fine single-user history, weak realtime/collab, single-writer limits.
- Postgres+API/custom Rust: full control + `pg_dump` portability, but N-tool integration/debug/onboarding tax for a realtime need we don't yet have.
- Supabase: default if relational SaaS without realtime moat (PG, RLS, self-host, portable); realtime = manual client updates, higher overhead.
- Firebase: proprietary, no self-host, rewrite-to-exit, per-read billing — wrong for Rust CLI + self-host.
- Redis+API: no evidence for this workload. Supabase-data + Convex-realtime: only if realtime proves a moat (extra deploy/billing/failure).

## 3. What stays local (and why)

Execution, sandbox, Git branches/patches, LLM calls/retries, token metering, safety proofs, full logs/reports, secrets — because they are the security/correctness boundary (sandbox), the output authority (Git), and the cost/secrets truth (providers/env). Moving them adds failure modes without benefit.

## 4. Why Rust remains execution layer

Single-crate Tokio pipeline + sandbox + git2 + provider trait already works offline with hermetic proofs; Convex actions (JS/Node, timeouts, no auto-retry, WS fragility history) are a worse executor. Rust also owns the TUI/ACP/CLI surfaces.

## 5. Optional, not mandatory

Convex outage/network loss/offline must never fail a run: warn + spool + backfill; CLI wins conflicts; flag off = byte-identical local behavior. Making it mandatory would break local-first, self-host, and air-gapped use for unproven benefit.

## 6. Cloud vs self-hosted

Cloud: zero-maint, paid streams/exceptions/audit/regions/SLA; per-call + I/O + egress + compute metering (heartbeats penalized — use presence pattern). Self-host: data-residency/privacy win, but Compose ops (PG/S3/backups/upgrades), Auth-manual + CLI gaps, Discord-only support, deploy-with-WS reject report, no verified GA label. Support all three: no-Convex, Cloud, self-hosted. GA/parity + FSL≠OSI procurement note must be re-verified at build.

## 7. Failure/recovery

Local success + spooled mirror on any Convex/network fault; idempotency keys + CAS + terminal-once; stale workers via TOO_OLD sweeper (no auto-commit); cancel pre-start only; scheduled mutations exactly-once vs actions at-most-once (explicit retry, idempotent-only); HTTP adapter owns retry (WS idempotency does not transfer); keys in keyring, admin keys never on workers; Convex breach = metadata visibility only.
