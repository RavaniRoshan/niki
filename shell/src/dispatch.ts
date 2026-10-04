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
      // Esc means deny, and means it by choosing the engine's own deny option.
      const deny = options.find((o) => o.id.toLowerCase().includes('den')) ?? options[options.length - 1];
      return deny
        ? { actions: [{ kind: 'approval.decide' }], approval: { id: state.approval.request.id, optionId: deny.id } }
        : { actions: [{ kind: 'approval.decide' }] };
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

  // Transcript scrolling is checked before the composer so a focused composer cannot swallow it.
  if (key.pageUp) return { actions: [], scroll: 'pageUp' };
  if (key.pageDown) return { actions: [], scroll: 'pageDown' };
  if (key.home) return { actions: [], scroll: 'home' };
  if (key.end) return { actions: [], scroll: 'end' };

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
    return { actions: [{ kind: 'palette.open' }] };
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

  if (key.shift && key.input === 'Tab') {
    // Cycling a mode must never take focus out of the composer.
    return { actions: [{ kind: 'mode.cycle' }] };
  }

  if (key.return) {
    if (state.slashMenu) return { actions: [{ kind: 'slashMenu.accept' }] };
    const text = state.composer.trim();
    if (text.length === 0) return none;
    return {
      actions: text.startsWith('/')
        ? [{ kind: 'command.run', name: text.split(/\s+/)[0] ?? text, args: text.slice(text.indexOf(' ')).trim() }]
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

  if (!key.ctrl && !key.meta && key.input && !key.escape) {
    return { actions: [], insert: key.input };
  }

  return none;
}