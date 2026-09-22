# NIKI State Layout (Phase 4.1 — file contract)

Every persistent store, its writer, its readers, and its schema version.
`<output_dir>` defaults to `.niki/` (`[general] output_dir`). Everything
below is git-ignored; nothing here is a second source of truth — the files
are the truth, `store/` is a rebuildable derived cache.

| Store | Path | Writer | Readers | Schema |
|---|---|---|---|---|
| Role memory | `.niki/memory/{planner,coder,tester,reviewer,synthesizer,security_auditor,red,critic}.json` | `memory::save_memory` (atomic temp+rename, 0600 where restricted) | `load_memory`, prompt injection (`render_*`), `store` scan | `RoleMemory.schema_version = 1`; mismatch warns loudly |
| User memory | `.niki/memory/user.json` | `save_user_memory` (atomic) | `load_user_memory`, hierarchical render, `memory recall` | `MemoryEntry` serde defaults; unparsable → empty + warning |
| Team memory | `.niki/memory/team.json` | `save_team_memory` (atomic) | `load_team_memory`, hierarchical render, `memory recall` | same as user memory |
| Compressed knowledge | `.niki/memory/compressed/{role}.json` | `compress_context` (atomic) | `load_compressed_knowledge` via `render_memory_with_budget` | `CompressedKnowledge.schema_version = 1` |
| Learnings | `<output_dir>/learnings.jsonl` | `append_learning` (O_APPEND + fsync) | `tail_learnings` (skips malformed lines), `store` scan | `LearningEntry` serde defaults |
| Task records | `<output_dir>/tasks/<id>/task.json` | `TaskRecord::save_to_disk` | `status`/`report`, `store` scan | serde defaults on newer fields |
| Run manifest | `<output_dir>/tasks/<id>/manifest.json` | `provenance::write_manifest` | `report`, eval harness | snapshot id `niki-task-<8hex>` |
| Context record | `<output_dir>/tasks/<id>/context.json` | `update_context_budget` | `report` (dropped-section accounting) | — |
| Trace | `<output_dir>/tasks/<id>/trace.jsonl` | `output/report.rs` | `report`, dashboard | append-only |
| Sessions | `.niki/sessions/*.json` (0600) + `*.jsonl` journal | `SessionManager::save_current` / `append_journal` | `session` CLI (rewind/undo), chat rehydrate | `Session.schema_version = 1` |
| KB | `<output_dir>/kb/{manifest.json,architecture.md,history.md,dependencies.json,entities/*.md}` | `kb::write_atomic` / `write_kb_markdown` (snapshot stamp first line) | `context_pack`, architecture CLI | `KbManifest.schema_version = 1` |
| Structural index | `<output_dir>/structural_index/` + manifest | `structural::build_index` | `context_pack` symbol excerpts | `IndexManifest` versioned |
| **Converged store (cache)** | `<output_dir>/store/{index.json,manifest.json}` | `store::build_index` (atomic) | `store::query_store`, `query_learnings` | `STORE_SCHEMA_VERSION = 1`; mismatch → ignored + live-scan fallback |
| **Skills** | `<output_dir>/skills/<name>/{SKILL.md,metadata.json}`, `skills-staging/<id>/`, `skills-lock.json` | `skills::stage_candidate` / `promote_candidate` / `retire_skill` (atomic) | `niki skills` CLI, `skill_list`/`skill_load` | `SkillMetadata.version`; lock pins content hash |
| Audit trail | `.niki/audit/<task_id>.jsonl` | `audit::append_audit_entry` (O_APPEND) | `audit` CLI bundle | one JSON object per line |

Conventions: JSON state writes go through `kb::write_atomic` (or
`write_atomic_restricted` for 0600 files); append-only logs use O_APPEND;
corrupt files warn loudly and degrade to empty, never abort the run.
