# Niki TUI key matrix (TUI-014)

Source of truth for defaults: `src/display/keybindings.rs` (`BINDING_TABLE`).
User overrides: `[ui.keybindings]` in `niki.toml` (see `niki.example.toml`).
`?` overlay is generated from the table — this doc mirrors it plus context.

## Global (keybinding table, both loops unless noted)

| Action | Default | run_tui | run_chat |
|---|---|---|---|
| Toggle help | `?` | yes | yes |
| Toggle mouse capture | Ctrl+E | yes | yes |
| Cancel stage / exit | Ctrl+C | yes | no (chat owns keys) |
| Command palette | Ctrl+P | yes (non-chat pages) | yes |
| Cycle theme | Ctrl+T | yes (non-chat pages) | NO — bare `t` (see divergences) |
| Chat ↔ page | Tab | yes (non-chat pages) | yes |
| Fleet grid | `g` | yes | via page jumps |
| Session view | `s` | yes | via page jumps |

## Chat composer (receiver-scoped, `pages/chat.rs`, `input.rs`)

Enter submit · Esc menu/modal/close · arrows history · Ctrl+A/E line ends ·
Ctrl+W word kill · Ctrl+U/K line kill · Ctrl+Y/Alt+Y yank/pop · Ctrl+Z undo ·
Ctrl+R history search · Ctrl+O thinking · Ctrl+S steer · Ctrl+F transcript
search · Tab apply @-file / complete `/model` arg · Shift+Tab permission mode
(or thinking with text) · `@` files · `/` commands · `!` shell.

## Overlays / modals

- Permission: Up/Down or j/k move · Enter/Y confirm · Esc/N deny · Tab scope ·
  Ctrl+D detail. Clicks hit-test shared geometry (`permission.rs`).
- Command palette (Ctrl+P): Up/Down navigate · Enter run · Esc close.
- Slash menu: Up/Down/Enter/Esc, Tab complete, live filter narrows the box.
- Tool detail: j/k or wheel scroll · y copy · Esc or outside-click close.
- Help (`?`): any click or Esc closes.

## Known run_tui vs run_chat divergences (do not "fix" without unification)

**Empty.** The four that were listed here are all resolved, and the list was
sending the next maintainer after work that had already been done:

1. ~~Theme: Ctrl+T vs bare `t`.~~ There is no bare `t` handler anywhere in
   `src/` — the only `Char('t')` arms are guarded on `CONTROL`. The stale
   letter also survived in the command palette's `theme: cycle` row, which
   advertised a key that did nothing.
2. ~~Quit: `q` confirm on one loop, `q` quit on the other.~~ `q` on a
   sub-page goes back on both loops, and a page that declines it falls back to
   the confirm modal on both.
3. ~~Tab: Chat/Run vs Chat/last page.~~ Both resolve `Chat → Run`.
4. ~~Ctrl+C: a cascade in one loop, chat keys in the other.~~ Both loops route
   it through the same handler, and
   `both_tui_loops_route_ctrl_c_through_the_shared_handler` says so.

`tests/tui_key_matrix_matches_the_code.rs` checks this section against the
source, so it cannot go stale again: adding a divergence without a matching
code difference fails the build, and a stale entry fails it too.

Unify only via the keybinding table (TUI-003 follow-up), never by copying
literals.
