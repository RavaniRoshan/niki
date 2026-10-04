# GAPS — things NIKI cannot show, and things that do not exist yet

Rule: the shell never invents a value the engine did not report. Every row here is a place
where the honest answer today is an empty state, and where that empty state is logged.

Last updated: 2026-10-04, after Phase 0 reference verification.

## G1 — Reference material that does not exist on this machine

| Missing | What it was needed for | Consequence | Status |
| --- | --- | --- | --- |
| `../niki-agent-ref/` (whole repo) | porting proven behaviour: `command_registry.py` bypass tiers, `tui/widgets/*`, `niki/theme.py`, `niki/motion.py`, `niki/keymap.py`, `docs/niki/KEYMAP.md`, `test_pty_*` scenarios | **Owner decision: rebuild from the written spec.** Every ported behaviour is written fresh and verified by a test written in the same slice. Nothing is copied, so there is no attribution obligation. | RESOLVED — spec rebuild |
| `niki-agent-ref/intro.gif` (800x653, 325 frames, 23.1 s) | frame-accurate study of the Kimi Code CLI intro | Kimi behaviour is reconstructed from the 5 supplied stills plus the owner's written grammar. Frame-accurate claims about Kimi's motion are **not** possible. | OPEN — permanently, unless the GIF is supplied |
| `niki-agent-ref/demo.gif` | the Claude Code demo | **Found** at `/home/shiva/gif/demo.gif` — verified 1552x992, 414 frames, 42.4 s, i.e. the same file the notes describe. Not copied into the repo (11 MB, and `.gitignore` excludes `*.gif` outside `assets/`). Two extra frames extracted by a Pillow scene-change scan are in `reference/frames/claude_demo/EX01_*`, `EX02_*`. | RESOLVED |
| `niki-tui-reference-pack.zip` arrived untracked at the repo root | the reference pack itself | Unpacked into `docs/foundation/reference/`. The zip itself is left in place, untracked. | RESOLVED |

## G2 — Engine facts that force an empty or degraded shell state

| Gap | Evidence | What the shell shows instead | Row |
| --- | --- | --- | --- |
| Context meter numbers only exist **after** a run has reported `StageTotals` | `src/display/tui.rs:193` is the only producer; nothing reports usage before the first stage finishes | No meter until the engine sends real `used`/`limit`. Never a percentage derived from a guess. | E7 |
| Ahead/behind for the branch is only known after a git call | engine does not report it in any event | Footer shows the branch name only, with no arrow, until it is really known | E7, E8 |
| Cost is only known after usage is priced | `src/cost.rs:173`, priced from a dated table (`PRICE_TABLE_AS_OF`) | No cost until priced; unpriced models report `is_unpriced` rather than 0.00 | B3 |
| Approval cannot cross a process boundary today | `DisplayEvent::PermissionRequest` embeds a `std::sync::mpsc::Sender` inside a **`Clone`** enum (`src/display/tui.rs:201`) | The protocol carries `approval.request { id }` + `approval.reply { id, decision }`; the engine owns the oneshot. Until `niki serve` exists, a remote shell cannot answer an approval, so `niki run` keeps its in-process prompt. | C5 |
| `niki serve` does not exist | `src/main.rs:40-121` registers 28 subcommands, none of them `serve` | Phase 1 S2 builds it. `niki acp` (`src/acp/server.rs`) is the existing JSON-RPC-over-stdio precedent. | A1 |
| The pipeline is bound to a concrete display struct | `execute_pipeline(… display: &mut AgenticDisplay …)` at `src/orchestrator/pipeline.rs:2667` | A `PipelineSink` trait is the first engine edit; until it lands, only one UI system can drive the pipeline. | A3 |

## G3 — Behaviour deliberately not built in this phase

Per the owner's PARITY instruction, these are **not** built until every P0/P1 row is WORKS and the
owner approves. Listed here so the omission is a decision on the record, not a silent gap.

Configurable keymap · Vim composer mode · `@` file mention honouring `.gitignore` · image paste ·
transcript search · fork and resume · OSC 9;4 progress. Also unbuilt: the `niki-contrast` and
`niki-dim` palettes are **derived** from the visual spec's rules rather than ported from
`theme.py`, and are marked as derived wherever they appear.
## G4 — Derived tokens, and why they are not in the owner's table

The owner's palettes are transcribed verbatim except for four tokens. Each exists because the
owner's value **does not** clear the contrast floor the same spec demands, measured not guessed:

| Token | Palette | Owner's value | Measured | Derived value | Measured |
| --- | --- | --- | --- | --- | --- |
| `errorOnSurface` | `niki` | `#D2605C` | 3.92:1 on `panel`, 4.42:1 on `surface` | `#DE6C68` | 4.54:1 on `panel`, 5.12:1 on `surface` |
| `errorOnSurface` | `niki-light` | `#A83232` | — | `#8F2A2A` | 7.40:1 on `surface` |
| `accentOnPanel` | `niki-light` | `#1F7A70` | 4.24:1 on `panel` | `#177268` | 4.74:1 on `panel` |
| `successOnPanel` | `niki-light` | `#2E7D4F` | 4.15:1 on `panel` | `#267547` | 4.65:1 on `panel` |

The owner's values are unchanged and still used on the base background, where they all clear
4.5:1. The derived steps are used only where text is drawn on a raised surface. `niki-contrast` and
`niki-dim` pass every pair with no derivation at all.

## G5 — Defects found in the existing TUI, recorded because they are real

| Defect | Evidence | Consequence |
| --- | --- | --- |
| **A failed tool's error is captured but never rendered.** `ToolCard::set_failed` stores the message in `ToolStatus::Failed { error }` and sets `expanded = true` with the comment "Auto-expand to show error" (`src/display/components/tool_card.rs:67-72`), but the expanded body renders `card.output` only (`:144-172`), and the detail overlay reads `card.output` only (`src/display/components/tool_detail.rs:29-33`). `timing()` covers only `Running` and `Success` (`:90-96`). | confirmed by reading the source and by the before-frame capture | The reference frames show this inline in the error colour (C06). NIKI must render it. Not yet done — shell row E6 is still MISSING. |
| The tool-card box is a fixed 50 characters wide at both terminal sizes. | `docs/foundation/before/*.txt` | Lost at wide terminals. |
| The permission and help overlays paint over the surface without clearing it, so input-box and transcript text bleeds through. | `docs/foundation/before/permission_80x24.txt` | A layout bug the rebuild must not inherit. |
| A `DisplayEvent` exists for every tool and stage event except help: `?` toggles `show_help` inside a private function (`src/display/tui.rs:404-406`) and no event drives it. | `src/display/tui.rs:48-238` | The old seam could not express "user opened help". The new protocol does not need to: help is a local overlay. |
| `render_activity_spinner` (`src/display/tui.rs:1993`) is only drawn when a pipeline stage is running, so no before-frame contains it. | before-frame capture | The activity strip was never captured "before". Stated rather than papered over. |

## G6 — Known limitation in the new shell

| Limitation | Evidence | When it bites |
| --- | --- | --- |
| `reduce` copies the message array on every streamed token, so building a transcript is O(tokens x messages). The **render** path is O(visible rows) and measured flat (ratio 0.93 across a 10x transcript), but state construction is not. | `shell/src/state.ts`, `turn.delta` arm; `shell/test/perf.test.tsx` | Only when replaying thousands of turns in one test. A long live session streams into one message per turn, so the practical cost is bounded by turns, not tokens. Fixing it means a mutable tail buffer, which would give up the "state is a value" property the reducer tests rely on. Deliberate trade, recorded rather than hidden. |
