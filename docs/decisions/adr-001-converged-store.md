# ADR-001: Converged Store Direction (Phase 2.5 — no schema commitment)

Date: 2026-09-20. Status: advisory spike for Phase 4 decision. No dependency added.

## What "converged" must mean for niki

One local-first query surface answering "all learnings + memory + artifacts for
task X" across runs/stages/artifacts/learnings/memories, with FTS for text and
vector search for paraphrase retrieval. Offline by default; `.niki/` stays
git-ignored; no new network dependency in the default path.

## Option A — File stores with the Phase 2.1 contract (current path)

- Stores: `.niki/memory/*.json` (atomic temp+rename, `schema_version: 1`,
  warn-loud on mismatch), `<output_dir>/learnings.jsonl` (append-only,
  malformed lines skipped), `compressed/*.json`, `sessions/*.json` (atomic +
  0600), `kb/*` (atomic + snapshot stamps), `structural_index/` + manifest.
- Pros: zero new deps (deny.toml gate untouched), offline, crash-safe writes,
  human-inspectable, existing readers hardened in 2.1.
- Cons: no ACID across files (read-modify-write races on memory appends remain
  possible under true concurrency), no vector index, keyword-only retrieval,
  one query = many file reads.

## Option B — Embedded SQLite (+ FTS, vector extension only if license-clean)

- Shape: `<output_dir>/store/` with migrations + version table; tables for
  runs/stages/artifacts/learnings/memories; FTS for text; vector extension
  only if it passes `cargo deny check` + license gate. Keep JSONL/JSON writers
  as export format for one release, then cut readers over.
- Pros: ACID/atomicity boundaries explicit, single query API, FTS today,
  vector later, migrations versioned.
- Cons: new dependency through the tight `deny.toml` allow-list (deliberate
  extension + justification required), migration cost from `.niki/` layout
  (fixture-tested), embedding/index size must respect `disk_budget_mb`
  semantics, pure-Rust vs native extension tradeoff for distribution
  (`cargo dist`, vendored builds).

## Option C — Keep the unwired Convex mirror

- `src/control_plane/mod.rs:9-11` documents its own unwired status. Wiring it
  as the converged store would add a network dependency to the default path,
  breaking local-first/offline. Rejected as the default store; implement-or-delete
  is tracked separately in Phase 3.8 per its own P1 requirements (enqueue after
  local durability, idempotency keys, FIFO drain, persisted high-water).

## Recommendation for Phase 4

Enter Phase 4 with A as the running contract and a time-boxed B spike: prove
`store_migrate` (existing `.niki/` fixtures → SQLite) + `store_query` (one API
for task-X learnings/memory/artifacts) + `cargo deny check` clean before
committing. If B fails the gate, document the explicit rejection with
`STATE_LAYOUT.md` (every store, writer, reader, schema version) and keep A.
Do not add a second source of truth by accident.
