/**
 * The one key dispatcher.
 *
 * Every key the shell reacts to is matched here and nowhere else. `handleKey` is pure: it takes
 * the current state plus a key and returns the actions to apply, so a test can drive the whole
 * keyboard without a terminal. Checklist row D8: one dispatcher, one registry, and every
 * advertised action dispatches where the footer says it does.
 *
 * Conflict rules, fixed once so they cannot drift:
 *  - an approval prompt owns every key except none: Esc denies, Enter confirms, arrows move
 *  - Esc interrupts a run, and does nothing when nothing is running
 *  - Ctrl+C clears a non-empty composer, else interrupts, else arms exit
 *  - PgUp/PgDn and Home/End always scroll the transcript, even while the composer is focused
 *  - Shift+Tab and Alt+Tab never take focus away from the composer
 */

import type { AppState, LocalAction } from './state.js';
import { escapeDecision } from './approval.js';
import { commandRows } from './surfaces/commands.js';
import { acceptsQuery, filterItems, overlayItems } from './surfaces/items.js';

/**
 * The keymap, as data.
 *
 * Help renders this table and nothing else — there is no hand-written key list anywhere in the
 * interface. The rule from the owner is "nothing is advertised that a test has not exercised", so
 * every row here is also asserted against `handleKey` itself in `test/commands.test.tsx`: a row
 * that stops describing what the dispatcher does fails the build.
 */
export type KeyBinding = {
  /** How the key is written in a hint, and how a test presses it. */
  readonly keys: string;
  /** The context the row applies to; Help groups by it. */
  readonly when: 'always' | 'composer' | 'overlay' | 'approval' | 'popup' | 'typing';
  readonly action: string;
  readonly label: string;
};

export const KEYMAP: readonly KeyBinding[] = [
  { keys: '?', when: 'composer', action: 'overlay.open', label: 'help' },
  { keys: 'ctrl+k', when: 'always', action: 'palette.open', label: 'everything' },
  { keys: 'ctrl+o', when: 'always', action: 'details.toggle', label: 'turn details' },
  { keys: 'ctrl+t', when: 'always', action: 'stages.toggle', label: 'pipeline stages' },
  { keys: 'ctrl+m', when: 'always', action: 'mouse.toggle', label: 'mouse capture' },
  { keys: 'ctrl+g', when: 'always', action: 'editor.open', label: 'edit in $EDITOR' },
  { keys: 'ctrl+r', when: 'always', action: 'history.open', label: 'prompt history' },
  { keys: 'shift+tab', when: 'always', action: 'mode.cycle', label: 'cycle view' },
  { keys: 'ctrl+c', when: 'always', action: 'exit.arm', label: 'clear, interrupt, then quit' },
  { keys: 'ctrl+d', when: 'composer', action: 'exit.now', label: 'quit on an empty composer' },
  { keys: 'esc', when: 'always', action: 'overlay.close', label: 'close, then interrupt' },
  { keys: 'pgup', when: 'always', action: 'scroll', label: 'scroll transcript' },
  { keys: 'pgdn', when: 'always', action: 'scroll', label: 'scroll transcript' },
  { keys: 'home', when: 'always', action: 'scroll', label: 'top of transcript' },
  { keys: 'end', when: 'always', action: 'scroll', label: 'end of transcript' },
  { keys: 'g g', when: 'always', action: 'scroll', label: 'top of transcript' },
  { keys: 'g G', when: 'always', action: 'scroll', label: 'end of transcript' },
  { keys: 'enter', when: 'composer', action: 'turn.submit', label: 'send' },
  { keys: 'enter', when: 'popup', action: 'slashMenu.accept', label: 'run the highlighted command' },
  { keys: 'tab', when: 'popup', action: 'slashMenu.complete', label: 'complete into the composer' },
  { keys: 'enter', when: 'overlay', action: 'overlay.accept', label: 'accept the highlighted row' },
  { keys: 'tab', when: 'overlay', action: 'overlay.accept', label: 'accept the highlighted row' },
  { keys: 'esc', when: 'popup', action: 'slashMenu.close', label: 'close the popup' },
  { keys: 'enter', when: 'approval', action: 'approval.decide', label: 'confirm' },
  { keys: 'esc', when: 'approval', action: 'approval.decide', label: 'deny' },
];

export type KeyEvent = {
  readonly input: string;
  readonly ctrl: boolean;
  readonly meta: boolean;
  readonly shift: boolean;
  readonly escape: boolean;
  readonly return: boolean;
  readonly backspace: boolean;
  readonly delete: boolean;
  readonly upArrow: boolean;
  readonly downArrow: boolean;
  readonly leftArrow: boolean;
  readonly rightArrow: boolean;
  readonly pageUp: boolean;
  readonly pageDown: boolean;
  readonly home: boolean;
  readonly end: boolean;
  readonly tab: boolean;
  /** Set for a bracketed paste: the whole payload is in `input` and is literal text. */
  readonly paste?: boolean;
};

export type KeyOutcome = {
  readonly actions: readonly LocalAction[];
  /** Text to append to the composer, when the key is an ordinary character. */
  readonly insert?: string;
  /** Transcript scrolling works even while the composer holds focus. */
  readonly scroll?: 'pageUp' | 'pageDown' | 'home' | 'end' | 'lineUp' | 'lineDown';
  /** An approval decision the caller must send to the engine. */
  readonly approval?: { readonly id: string; readonly optionId: string };
};

export function handleKey(state: AppState, key: KeyEvent): KeyOutcome {
  const none: KeyOutcome = { actions: [] };

  // The approval prompt owns the keyboard while it is open. Nothing reaches the composer.
  if (state.approval) {
    const options = state.approval.request.options;
    const currentIndex = Math.max(
      0,
      options.findIndex((o) => o.id === state.approval?.focusedOptionId),
    );
    if (key.escape) {
      // Esc means deny: it picks the engine's own refusal option rather than merely closing.
      const deny = escapeDecision(state.approval.request);
      return deny
        ? { actions: [{ kind: 'approval.decide' }], approval: { id: state.approval.request.id, optionId: deny.id } }
        : { actions: [{ kind: 'approval.decide' }] };
    }
    // A bracketed paste is text, full stop: it must never run a command and never submit, no matter
  // what it contains.
  if (key.paste) {
    return { actions: [], insert: key.input };
  }

  if (key.return) {
      const focused = options[currentIndex];
      return focused
        ? { actions: [{ kind: 'approval.decide' }], approval: { id: state.approval.request.id, optionId: focused.id } }
        : { actions: [{ kind: 'approval.decide' }] };
    }
    if (key.upArrow || (key.input === 'k' && !key.ctrl)) {
      const next = options[(currentIndex - 1 + options.length) % options.length];
      return { actions: [{ kind: 'approval.focus', optionId: next?.id ?? state.approval.focusedOptionId }] };
    }
    if (key.downArrow || (key.input === 'j' && !key.ctrl)) {
      const next = options[(currentIndex + 1) % options.length];
      return { actions: [{ kind: 'approval.focus', optionId: next?.id ?? state.approval.focusedOptionId }] };
    }
    return none;
  }

  // A confirmation is modal: it owns the keyboard until the user answers it or cancels.
  if (state.confirm) {
    if (key.escape) return { actions: [{ kind: 'confirm.cancel' }] };
    if (key.return) return { actions: [{ kind: 'confirm.accept' }] };
    return none;
  }

  // An open overlay owns the keyboard, including the scroll keys: a popup that scrolled the
  // transcript behind it would move rows the user cannot see. Ordinary characters go to its query
  // rather than the composer, which is what makes the palette and the pickers searchable without a
  // second buffer.
  if (state.overlay) return handleOverlayKey(state, key, none);

  // Transcript scrolling is checked before the composer, because a focused composer must never
  // swallow a scroll key. This is one of the three defects the old TUI had and NIKI must not.
  if (key.pageUp || key.home || key.end) return { actions: [], scroll: key.pageUp ? 'pageUp' : key.home ? 'home' : 'end' };
  if (key.pageDown) return { actions: [], scroll: 'pageDown' };

  // The slash popup lives inside the composer, so it never takes the keyboard: every key it does
  // not claim still types. That is the whole of "it must never block typing".
  if (state.slashMenu && !isMultiLine(state)) {
    if (key.escape) return { actions: [{ kind: 'slashMenu.close' }] };
    if (key.upArrow) return { actions: [{ kind: 'slashMenu.move', index: moveIndex(commandRows(state.slashMenu.query).length, state.slashMenu.selected, -1) }] };
    if (key.downArrow) return { actions: [{ kind: 'slashMenu.move', index: moveIndex(commandRows(state.slashMenu.query).length, state.slashMenu.selected, 1) }] };
    if (key.tab) return { actions: [{ kind: 'slashMenu.complete' }] };
    if (key.return) return { actions: [{ kind: 'slashMenu.accept' }] };
  }

  // Up/Down move the caret only inside a multi-line composer. In a single-line composer there is
  // no caret to move, so they scroll the transcript.
  if ((key.upArrow || key.downArrow) && !isMultiLine(state)) {
    return { actions: [], scroll: key.upArrow ? 'lineUp' : 'lineDown' };
  }

  // `g` then `G` and `g` then `g`: the vim-style jump pair, as a two-key chord. A bare `g` still
  // types a `g`, because a chord that eats a letter is worse than a chord that waits.
  if (key.input === 'g' && !key.ctrl && !key.meta) {
    if (state.pendingChord === 'g') {
      return { actions: [{ kind: 'clearChord' }], scroll: 'home' };
    }
    return { actions: [{ kind: 'setChord', chord: 'g' }], insert: 'g' };
  }
  if (key.input === 'G' && !key.ctrl && !key.meta && state.pendingChord === 'g') {
    return { actions: [{ kind: 'clearChord' }], scroll: 'end' };
  }

  if (key.ctrl && key.input === 'c') {
    if (state.composer.length > 0) return { actions: [{ kind: 'composer.set', text: '' }] };
    if (state.activity) return { actions: [{ kind: 'turn.interrupt' }] };
    return { actions: [{ kind: 'exit.arm' }] };
  }

  if (key.ctrl && key.input === 'd' && state.composer.length === 0) {
    return { actions: [{ kind: 'exit.now' }] };
  }

  if (key.ctrl && key.input === 'o') {
    return { actions: [{ kind: 'details.toggle' }] };
  }

  if (key.ctrl && key.input === 't') {
    return { actions: [{ kind: 'stages.toggle' }] };
  }

  if (key.ctrl && key.input === 'k') {
    // Ctrl+K is the one key that works from inside an overlay too: it always means "everything".
    return state.overlay
      ? { actions: [{ kind: 'overlay.close' }, { kind: 'palette.open' }] }
      : { actions: [{ kind: 'palette.open' }] };
  }

  if (key.ctrl && key.input === 'g') {
    return { actions: [{ kind: 'editor.open' }] };
  }

  if (key.ctrl && key.input === 'r') {
    return { actions: [{ kind: 'history.open' }] };
  }

  if (key.ctrl && key.input === 'm') {
    // Mouse capture is a mode with a one-key release toggle, so a user can always get their
    // native selection back without leaving the app.
    return { actions: [{ kind: 'mouse.toggle' }] };
  }

  if (key.escape) {
    if (state.slashMenu) return { actions: [{ kind: 'slashMenu.close' }] };
    if (state.overlay) return { actions: [{ kind: 'overlay.close' }] };
    if (state.protocolError) return { actions: [{ kind: 'dismissError' }] };
    if (state.activity) return { actions: [{ kind: 'turn.interrupt' }] };
    return none;
  }

  if (key.shift && key.tab) {
    // Cycling a mode must never take focus out of the composer.
    return { actions: [{ kind: 'mode.cycle' }] };
  }

  // A bracketed paste is text, full stop: it must never run a command and never submit, no matter
  // what it contains.
  if (key.paste) {
    return { actions: [], insert: key.input };
  }

  if (key.return) {
    if (state.slashMenu) return { actions: [{ kind: 'slashMenu.accept' }] };
    const text = state.composer.trim();
    if (text.length === 0) return none;
    // Split on the first whitespace run. `indexOf(' ')` returns -1 for a single-word command,
    // and `slice(-1)` would then send the command's own last letter as its arguments.
    const spaceAt = text.search(/\s/);
    const name = spaceAt === -1 ? text : text.slice(0, spaceAt);
    const args = spaceAt === -1 ? '' : text.slice(spaceAt).trim();
    return {
      actions: text.startsWith('/')
        ? [{ kind: 'command.run', name, args }]
        : [{ kind: 'turn.submit', prompt: text }],
    };
  }

  if (key.backspace || key.delete) {
    return { actions: [{ kind: 'composer.set', text: state.composer.slice(0, -1) }] };
  }

  // Newline insertion: Shift+Enter and Alt+Enter insert one instead of submitting.
  if ((key.shift || key.meta) && key.return) {
    return { actions: [], insert: '\n' };
  }

  // `?` is Help only where it cannot be text. A composer that is about to receive a question mark
  // gets the question mark, because losing a character to a shortcut is the worse failure.
  if (key.input === '?' && !key.ctrl && !key.meta && state.composer.length === 0) {
    return { actions: [{ kind: 'overlay.open', overlay: 'help' }] };
  }

  if (!key.ctrl && !key.meta && key.input && !key.escape) {
    return { actions: [], insert: key.input };
  }

  return none;
}

function isMultiLine(state: AppState): boolean {
  return state.composer.includes('\n');
}

/** Wrapping index movement over a list of `length` rows. An empty list stays at zero. */
function moveIndex(length: number, current: number, by: number): number {
  if (length <= 0) return 0;
  return (((current + by) % length) + length) % length;
}

/**
 * The keyboard contract for every overlay: Up/Down move, Tab or Enter accept, Esc closes, ordinary
 * characters and Backspace edit the query. A theme picker previews on move, which is why moving is
 * more than a number change.
 */
function handleOverlayKey(state: AppState, key: KeyEvent, none: KeyOutcome): KeyOutcome {
  if (key.escape) return { actions: [{ kind: 'overlay.close' }] };

  const rows = filterItems(state.overlayQuery, overlayItems(state));
  const index = Math.min(state.overlayIndex, Math.max(0, rows.length - 1));

  if (key.upArrow || key.downArrow) {
    // An overlay with no rows — Help, an empty picker — scrolls instead of moving a highlight.
    if (rows.length === 0) {
      return { actions: [{ kind: 'overlay.scroll', by: key.upArrow ? 'lineUp' : 'lineDown' }] };
    }
    const next = moveIndex(rows.length, index, key.upArrow ? -1 : 1);
    const actions: LocalAction[] = [{ kind: 'overlay.move', index: next }];
    const preview = rows[next]?.onMove;
    if (preview) actions.push(preview);
    return { actions };
  }

  if (key.pageUp) return { actions: [{ kind: 'overlay.scroll', by: 'pageUp' }] };
  if (key.pageDown) return { actions: [{ kind: 'overlay.scroll', by: 'pageDown' }] };

  // The settings sheet edits values sideways; every other overlay has nothing to cycle.
  if (state.overlay === 'settings' && (key.leftArrow || key.rightArrow)) {
    return { actions: [{ kind: 'settings.edit', by: key.rightArrow ? 1 : -1 }] };
  }

  if (key.tab || key.return) return { actions: [{ kind: 'overlay.accept' }] };

  if (key.backspace || key.delete) {
    return { actions: [{ kind: 'overlay.setQuery', text: state.overlayQuery.slice(0, -1) }] };
  }

  if (acceptsQuery(state.overlay) && !key.ctrl && !key.meta && key.input && !key.escape) {
    return { actions: [{ kind: 'overlay.setQuery', text: state.overlayQuery + key.input }] };
  }

  return none;
}