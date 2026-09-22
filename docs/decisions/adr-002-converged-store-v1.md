# ADR-002: Converged Store v1 — File-Backed Index (Phase 4.1 Decision)

Date: 2026-09-21. Status: implemented (`src/store/mod.rs`).
Decides the open question from ADR-001 (Phase 2.5 spike).

## Decision

Implement convergence as a **file-backed derived index**, not embedded
SQLite. Deviation reason (per ADR-001's "deviate only with a written reason"):

- Zero new dependencies: the `deny.toml` allow-list is untouched
  (`cargo deny check`: bans/licenses/sources pass), and `cargo dist`
  builds gain no native extension (no `libsqlite3-sys` C build, no
  pure-Rust pager to audit).
- Local-first/offline by default; no network dependency in any path.
- The existing writers (learnings JSONL append, role-memory atomic JSON,
  task records) are already crash-safe and human-inspectable; replacing them
  wholesale would churn every reader for no query-power gain.

## What "converged" means in v1

- One query API — `store::query_store(project, config, task, limit)` —
  answers "all learnings + memory + run records for task X" across
  `learnings.jsonl`, `.niki/memory/*.json`, and `<output_dir>/tasks/*/task.json`.
- `<output_dir>/store/` holds a rebuildable cache: `index.json` (the docs)
  plus `manifest.json` (the version table: `schema_version`, `built_at`,
  `doc_count`, `sources`). Deleting `store/` is always safe; the next query
  live-scans the same sources with the same ranking (deterministic fallback).
- Retrieval (`hybrid_rank`) scores keyword overlap (0.45) + trigram-vector
  cosine (0.35) + recency (0.10) + authority tier (0.10). The trigram vector
  is the paraphrase signal: no shared keywords still matches. Pure Rust,
  deterministic (ties broken by id), offline.
- Index size honors `[repo_intel] disk_budget_mb` (previously unread key):
  the serialized index is capped, oldest docs dropped first, at least one
  retained.
- Atomicity boundary: each source write is atomic (temp + rename); the
  index/manifest/lock writes are atomic. There is **no cross-file
  transaction** — documented, not ACID. Concurrent memory appends can still
  interleave at the file level (dedupe by content hash makes repeats safe).

## What v1 does not do (explicit non-goals)

- No embedding model, no vector extension: the trigram TF-cosine is the
  "vector" surface. A real embedding index would need a model dependency
  (network or large binary) — rejected for the default path.
- No cutover of durable writers: JSONL/JSON remain the source of truth.
  Stage-artifact bodies are not yet indexed (task records carry
  description/verdict/risk); trace JSONL indexing is future work.
- SQLite stays an option if cross-file transactions become necessary; this
  ADR would then be superseded with a migration tested on fixtures.

## Verification

`cargo test store_migrate store_query` (module tests: `store::tests::*`),
`cargo test context_pack retrieval`, `cargo deny check` (bans/licenses/sources).
