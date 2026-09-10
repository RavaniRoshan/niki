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

1. Theme: Ctrl+T (run_tui, non-chat) vs bare `t` (run_chat pages).
2. Quit: `q`/Esc confirm modal (run_tui Run page) vs `q` quit modal (run_chat).
3. Tab: toggles Chat/Run in run_tui; in run_chat toggles Chat/last page.
4. Ctrl+C: cancel/exit cascade only in run_tui; run_chat relies on chat keys.

Unify only via the keybinding table (TUI-003 follow-up), never by copying literals.
