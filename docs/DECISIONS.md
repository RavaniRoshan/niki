# NIKI — Decisions (Phase 0)

Recorded per `<decisions>`. These are the recommended defaults; say the word
if any should change before P1.

1. **Boot-readiness contract**: See docs/DESIGN.md §3 table. Required blocks
   the first prompt: config, terminal capabilities, instructions, skills
   catalog, model catalog (cache-valid is acceptable), renderer warm. Optional
   (lazy): git snapshot, model prewarm, MCP beyond cached-catalog required
   servers, syntax highlighting, history hydration.
2. **Viewport**: Inline (native scrollback) with a small live region. A
   fullscreen owned mode stays out of v1 unless a concrete need appears.
3. **Git backend**: Shell out to `git` for correctness; the binary has a
   documented dependency on the `git` CLI. `go-git` revisit only if the
   self-contained binary becomes a hard requirement.
4. **Config format**: TOML via `pelletier/go-toml/v2`; accept the
   comment-preservation limitation. Settings edits write a minimal, explicit
   subset; we do not attempt Rust-`toml_edit`-style comment fidelity.
5. **App-server seam**: Deferred (not built in v1). The core stays a package
   so the seam can be added without restructuring.

Additional recorded choices:
- Module path: `github.com/RavaniRoshan/niki` (existing).
- Existing `internal/engine` will be superseded by `internal/core` +
  `internal/llm` in P1; old code is migrated, not duplicated.
- Fixture runtime behind `//go:build fixture`, never in release.
- `golangci-lint` added as dev-only tooling.
