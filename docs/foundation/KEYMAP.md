<!-- GENERATED FILE. DO NOT EDIT BY HAND. -->
<!-- Regenerate: `cd shell && npx tsx scripts/gen-keymap.ts` (or `npx tsx shell/scripts/gen-keymap.ts` from the repo root) -->
<!-- Sources: shell/src/components/footer.tsx (COMMANDS, HINTS) and shell/src/dispatch.ts (handleKey). -->
<!-- shell/test/keymap.test.ts fails if this file is stale, names a key handleKey does not, -->
<!-- or names a command that is not in COMMANDS. -->

# NIKI Shell — KEYMAP

Generated from the registry. 30 key bindings, 21 commands,
28 contextual hints, 2 advertised hints that are not bindings.

The rule this file exists to hold is the owner's: nothing is advertised that the dispatcher
does not handle. The tables below are read out of the two sources rather than transcribed,
so a key added to the shell appears here on the next generator run and a key removed from the
shell disappears here rather than lingering as a promise.

<!-- keymap:begin -->

## Key bindings

Every key `handleKey` matches, in the order the dispatcher tests it. `Where` says whether the
binding is live while an approval prompt owns the keyboard or in the ordinary composer.

| Key | Effect | Where | Condition in handleKey |
| --- | --- | --- | --- |
| `esc` | action: approval.decide → action: confirm.cancel → action: dismissError → action: overlay.close → action: slashMenu.close → action: turn.interrupt | approval prompt, composer | `(key.escape) {`<br>`(key.escape) return { actions: [{ kind: 'confirm.cancel' }] };`<br>`(key.escape) return { actions: [{ kind: 'slashMenu.close' }] };`<br>`(key.escape) return { actions: [{ kind: 'overlay.close' }] };` |
| `paste` | insert the key as literal text | approval prompt, composer | `(key.paste) {` |
| `enter` | action: approval.decide → action: command.run → action: confirm.accept → action: overlay.accept → action: slashMenu.accept → action: turn.submit → insert a newline | approval prompt, composer | `(key.return) {`<br>`(key.return) return { actions: [{ kind: 'confirm.accept' }] };`<br>`(key.return) return { actions: [{ kind: 'slashMenu.accept' }] };`<br>`((key.shift \|\| key.meta) && key.return) {`<br>`(key.tab \|\| key.return) return { actions: [{ kind: 'overlay.accept' }] };` |
| `k` | action: approval.focus | approval prompt | `(key.upArrow \|\| (key.input === 'k' && !key.ctrl)) {` |
| `up` | action: approval.focus → action: overlay.move → action: overlay.scroll → action: slashMenu.move → scroll: the target chosen in handleKey | approval prompt, composer | `(key.upArrow \|\| (key.input === 'k' && !key.ctrl)) {`<br>`(key.upArrow) return { actions: [{ kind: 'slashMenu.move', index: moveIndex(commandRows(state.slashMenu.query).length, state.slashMenu.selected, -1) }] };`<br>`((key.upArrow \|\| key.downArrow) && !isMultiLine(state)) {`<br>`(key.upArrow \|\| key.downArrow) {` |
| `j` | action: approval.focus | approval prompt | `(key.downArrow \|\| (key.input === 'j' && !key.ctrl)) {` |
| `down` | action: approval.focus → action: overlay.move → action: overlay.scroll → action: slashMenu.move → scroll: the target chosen in handleKey | approval prompt, composer | `(key.downArrow \|\| (key.input === 'j' && !key.ctrl)) {`<br>`(key.downArrow) return { actions: [{ kind: 'slashMenu.move', index: moveIndex(commandRows(state.slashMenu.query).length, state.slashMenu.selected, 1) }] };`<br>`((key.upArrow \|\| key.downArrow) && !isMultiLine(state)) {`<br>`(key.upArrow \|\| key.downArrow) {` |
| `pgup` | action: overlay.scroll → scroll: the target chosen in handleKey | composer | `(key.pageUp \|\| key.home \|\| key.end) return { actions: [], scroll: key.pageUp ? 'pageUp' : key.home ? 'home' : 'end' };`<br>`(key.pageUp) return { actions: [{ kind: 'overlay.scroll', by: 'pageUp' }] };` |
| `home` | scroll: the target chosen in handleKey | composer | `(key.pageUp \|\| key.home \|\| key.end) return { actions: [], scroll: key.pageUp ? 'pageUp' : key.home ? 'home' : 'end' };` |
| `end` | scroll: the target chosen in handleKey | composer | `(key.pageUp \|\| key.home \|\| key.end) return { actions: [], scroll: key.pageUp ? 'pageUp' : key.home ? 'home' : 'end' };` |
| `pgdn` | action: overlay.scroll → scroll: pageDown | composer | `(key.pageDown) return { actions: [], scroll: 'pageDown' };`<br>`(key.pageDown) return { actions: [{ kind: 'overlay.scroll', by: 'pageDown' }] };` |
| `tab` | action: overlay.accept → action: slashMenu.complete | approval prompt, composer | `(key.tab) return { actions: [{ kind: 'slashMenu.complete' }] };`<br>`(key.tab \|\| key.return) return { actions: [{ kind: 'overlay.accept' }] };` |
| `g` | action: clearChord → action: setChord → insert "g" → scroll: home | composer | `(key.input === 'g' && !key.ctrl && !key.meta) {` |
| `G` | action: clearChord → scroll: end | composer | `(key.input === 'G' && !key.ctrl && !key.meta && state.pendingChord === 'g') {` |
| `ctrl+c` | action: composer.set → action: exit.arm → action: turn.interrupt | composer | `(key.ctrl && key.input === 'c') {` |
| `ctrl+d` | action: exit.now | composer | `(key.ctrl && key.input === 'd' && state.composer.length === 0) {` |
| `ctrl+o` | action: details.toggle | composer | `(key.ctrl && key.input === 'o') {` |
| `ctrl+t` | action: stages.toggle | composer | `(key.ctrl && key.input === 't') {` |
| `ctrl+k` | action: overlay.close → action: palette.open | composer | `(key.ctrl && key.input === 'k') {` |
| `ctrl+g` | action: editor.open | composer | `(key.ctrl && key.input === 'g') {` |
| `ctrl+r` | action: history.open | composer | `(key.ctrl && key.input === 'r') {` |
| `ctrl+m` | action: mouse.toggle | composer | `(key.ctrl && key.input === 'm') {` |
| `shift+tab` | action: mode.cycle | composer | `(key.shift && key.tab) {` |
| `backspace` | action: composer.set → action: overlay.setQuery | composer | `(key.backspace \|\| key.delete) {` |
| `delete` | action: composer.set → action: overlay.setQuery | composer | `(key.backspace \|\| key.delete) {` |
| `shift+enter` | insert a newline | composer | `((key.shift \|\| key.meta) && key.return) {` |
| `alt+enter` | insert a newline | composer | `((key.shift \|\| key.meta) && key.return) {` |
| `?` | action: overlay.open | composer | `(key.input === '?' && !key.ctrl && !key.meta && state.composer.length === 0) {` |
| `left` | action: settings.edit | composer | `(state.overlay === 'settings' && (key.leftArrow \|\| key.rightArrow)) {` |
| `right` | action: settings.edit | composer | `(state.overlay === 'settings' && (key.leftArrow \|\| key.rightArrow)) {` |

## Commands

The `COMMANDS` registry, in declaration order. `tier` decides whether the command is usable
while a run is in flight: `always`, `immediateUi`, `sideEffectFree` and `queued`.

| Command | Aliases | Tier | Description | Keywords |
| --- | --- | --- | --- | --- |
| `/help` | `/?` | `always` | Show this help | `keys` |
| `/model` | — | `always` | Choose the model | `llm` `switch` |
| `/theme` | — | `always` | Choose the theme | `colour` `color` `palette` |
| `/clear` | — | `always` | Clear the transcript | `reset` `wipe` |
| `/copy` | — | `sideEffectFree` | Copy the last response | `clipboard` |
| `/context` | — | `sideEffectFree` | Show context usage | `tokens` `window` |
| `/cost` | — | `sideEffectFree` | Show spend for this run | `money` `usd` `spend` |
| `/tokens` | — | `sideEffectFree` | Show token counts | `usage` |
| `/effort` | — | `always` | Choose reasoning effort | `thinking` |
| `/threads` | `/sessions` | `always` | Sessions and resume | `history` `resume` |
| `/prompts` | — | `always` | Search prompt history | `history` `recall` |
| `/editor` | — | `always` | Edit the prompt in $EDITOR | `external` |
| `/version` | — | `always` | Show the version | `about` |
| `/reload` | — | `always` | Reconnect to the engine | `reconnect` |
| `/quit` | `/q` | `always` | Quit | `exit` `bye` |
| `/auto` | — | `always` | Permission mode: auto | `permissions` |
| `/manual` | — | `always` | Permission mode: manual | `permissions` |
| `/yolo` | — | `always` | Permission mode: bypass (asks first) | `permissions` `bypass` |
| `/scrollbar` | — | `sideEffectFree` | Toggle the scrollbar | `scroll` |
| `/timestamps` | — | `sideEffectFree` | Toggle timestamps | `time` `clock` |
| `/line-numbers` | — | `sideEffectFree` | Toggle diff line numbers | `diff` `numbers` |

## Footer hints

What the footer shows per phase, from `HINTS` and `SEARCH_HINTS` in the same file. These are
advertised keys, not necessarily bindings — the section below is the difference between the two.

| Phase | Hint key | Label |
| --- | --- | --- |
| `booting` | — | connecting to the engine |
| `idle` | `/` | commands |
| `idle` | `?` | help |
| `idle` | `ctrl+k` | everything |
| `composing` | `/` | commands |
| `composing` | `?` | help |
| `composing` | `ctrl+k` | everything |
| `thinking` | `esc` | interrupt |
| `thinking` | `ctrl+o` | details |
| `thinking` | `?` | help |
| `streaming` | `esc` | interrupt |
| `streaming` | `ctrl+o` | details |
| `streaming` | `?` | help |
| `toolRunning` | `esc` | interrupt |
| `toolRunning` | `ctrl+o` | details |
| `toolRunning` | `?` | help |
| `awaitingApproval` | `↑↓` | choose |
| `awaitingApproval` | `enter` | confirm |
| `awaitingApproval` | `esc` | deny |
| `error` | `esc` | dismiss |
| `error` | `?` | help |
| `interrupted` | `type` | to continue |
| `interrupted` | `?` | help |
| `done` | `/` | commands |
| `done` | `?` | help |
| `done` | `ctrl+k` | everything |
| `overlayOpen` | `enter` | select |
| `overlayOpen` | `esc` | close |

## Advertised hints that are not key bindings

Each of these is shown by the footer and matched by no key in `handleKey`.

| Hint key | Labels | What actually happens |
| --- | --- | --- |
| `/` | `commands` | an ordinary character. The slash menu opens in the reducer, from the composer text, not from a key binding. |
| `type` | `to continue` | prose in the hint, not a key. |

`?` is bound in `dispatch.ts`, so a keypress opens the overlay the reducer names.

Phases with hints: `booting` · `idle` · `composing` · `thinking` · `streaming` · `toolRunning` · `awaitingApproval` · `error` · `interrupted` · `done` · `overlayOpen`.

<!-- keymap:end -->
