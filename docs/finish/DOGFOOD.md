# Dogfood Log

Real tasks done with NIKI on NIKI, what hurt, what was fixed, and how each
informed the next slice. Entries are dated and tied to a checklist row.

## 1. Boot phase ordering (2026-10-08) → O4
- **Task:** traced a clean boot to `boot → config → registry → startup → detect
  → program → session-run` and found the `detect` phase was marked *before* the
  detect goroutine was read back from its channel, so `bootMark("detect")`
  stamped the wrong span.
- **Hurt:** the boot trace read "detect = 0.0ms" even when DNS probe was pending.
- **Fixed:** R2-style off-by-one — `bootMark` now runs strictly *after* the
  preceding span's result is materialized (the detect channel read at line 523
  precedes `bootMark("detect")`).
- **Lesson:** phase labels must name completed work, not started work.

## 2. Session JSON round-trip (2026-10-09) → R3
- **Task:** `niki explain` against a freshly exported session JSON bundle.
- **Hurt:** the export wrote the header line but lost the closing event when the
  store iterator hit `context_compacted` (the R2 marker bug), producing 7
  lines where 8 were expected.
- **Fixed:** `BuildExportBundle` now appends a final `turn_completed` boundary
  marker and asserts line count in the golden test.
- **Lesson:** export goldens must count events end-to-end, not trust the store.

## 3. Checkpoint fork diff noise (2026-10-09) → R2
- **Task:** `niki fork` a session, edit one file, rewind to the checkpoint.
- **Hurt:** the diff showed the *base* file as fully added on every rewind,
  tripling the patch.
- **Fixed:** fork now pins `context_compacted` on the R2 marker boundary so the
  checkpoint snapshot is taken before the compacted span.
- **Lesson:** checkpoint boundaries must be explicit, not inferred from "last".

## 4. Git submodule status (2026-10-10) → G4
- **Task:** `niki git status` inside a repo containing two registered submodules
  and one unregistered nested repo.
- **Hurt:** `git.Status` reported only the top-level work; submodule dirty
  state and the unregistered nested repo were invisible.
- **Fixed:** `Submods` enumerates `.gitmodules` entries plus nested `.git` dirs
  that are not themselves registered submodules.
- **Lesson:** nested-repo detection must not conflate "registered" with
  "present".

## 5. Run trace for the trace exporter (2026-10-10) → O2
- **Task:** ran `niki explain` on a session, then wanted the raw event stream
  for offline inspection.
- **Hurt:** there was no single JSON artifact of a run — events lived only in
  the sqlite session log behind an iterator that redacts text.
- **Fixed:** `trace.Recorder` subscribes to `engine.Observe` and `Export` writes
  a schema-versioned JSON trace document.
- **Lesson:** debugging needs a stable, greppable artifact, not a REPL walk.

## 6. Boot trace acceptance (2026-10-11) → O4
- **Task:** gate `NIKI_BOOT_TRACE=1` so the trace file always lists every boot
  task in order.
- **Hurt:** a refactor that dropped `bootMark("startup")` was invisible until a
  user reported a missing span.
- **Fixed:** `TestBootTraceListsEveryTask` reproduces the sequence in-process and
  fails if any expected task is absent from the written log.
- **Lesson:** the spec ("lists every boot task") must be an executable test.
