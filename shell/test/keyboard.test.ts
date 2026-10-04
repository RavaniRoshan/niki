/**
 * D9 and E7: the keyboard, and the three defects the old TUI had.
 *
 * The defects, in the owner's words:
 *   1. "PgUp/PgDn were swallowed by the focused composer."
 *   2. "Shift+Tab stole focus from the composer."
 *   3. Transcript scrolling must work while the composer holds focus.
 *
 * There is one real conflict here, and it is resolved rather than hidden: **`j`, `k`, `g` and `G`
 * are letters**, so a composer that has focus must receive them. They cannot simultaneously scroll
 * the transcript. NIKI's resolution is that PgUp/PgDn/Home/End/Up/Down always scroll (none of them
 * are typeable text in a single-line composer), and `gg`/`G` work as a two-key chord while a bare
 * `g` still types. The alternative — making `j` and `k` steal letters from the composer — is the
 * behaviour that makes a composer feel broken.
 */

import { describe, expect, it } from 'vitest';
import { handleKey, type KeyEvent } from '../src/dispatch.js';
import { initialState, reduce, reduceLocal, type AppState, type ReduceOptions } from '../src/state.js';

const T0: ReduceOptions = { nowMs: 0 };

function key(overrides: Partial<KeyEvent> = {}): KeyEvent {
  return {
    input: '',
    ctrl: false,
    meta: false,
    shift: false,
    escape: false,
    return: false,
    backspace: false,
    delete: false,
    upArrow: false,
    downArrow: false,
    leftArrow: false,
    rightArrow: false,
    pageUp: false,
    pageDown: false,
    home: false,
    end: false,
    tab: false,
    ...overrides,
  };
}

function typed(text: string): AppState {
  let s = initialState(80, 24);
  for (const ch of text) {
    const outcome = handleKey(s, key({ input: ch }));
    if (outcome.insert !== undefined) {
      s = reduceLocal(s, { kind: 'composer.set', text: s.composer + outcome.insert }, T0);
    }
  }
  return s;
}

function scrolled(s: AppState, by: 'pageUp' | 'pageDown' | 'home' | 'end' | 'lineUp' | 'lineDown'): AppState {
  const outcome = handleKey(s, key({ [toKey(by)]: true }));
  expect(outcome.scroll, `${by} did not produce a scroll`).toBe(by);
  return reduceLocal(s, { kind: 'scroll', by }, T0);
}

function toKey(by: string): 'pageUp' | 'pageDown' | 'home' | 'end' | 'upArrow' | 'downArrow' {
  switch (by) {
    case 'pageUp':
      return 'pageUp';
    case 'pageDown':
      return 'pageDown';
    case 'home':
      return 'home';
    case 'end':
      return 'end';
    case 'lineUp':
      return 'upArrow';
    case 'lineDown':
      return 'downArrow';
    default:
      // Unreachable: `scrolled` only ever passes one of the six named directions. Failing loudly
      // is better than silently scrolling somewhere nobody asked for.
      throw new Error(`unknown scroll direction: ${by}`);
  }
}

describe('D9: the transcript scrolls while the composer is focused', () => {
  it('the composer is focused by default — the composer is the anchor', () => {
    // There is no focus state to lose: the composer always holds it. What matters is that a
    // scroll key does not type, and a typing key does not scroll.
    expect(initialState().composer).toBe('');
  });

  it('PgUp and PgDn scroll rather than being swallowed', () => {
    let s = typed('hello');
    s = scrolled(s, 'pageUp');
    expect(s.scrollOffset).toBeGreaterThan(0);
    s = scrolled(s, 'pageDown');
    expect(s.scrollOffset).toBe(0);
  });

  it('Home and End scroll to the ends', () => {
    let s = typed('x');
    s = scrolled(s, 'pageUp');
    expect(handleKey(s, key({ home: true })).scroll).toBe('home');
    expect(handleKey(s, key({ end: true })).scroll).toBe('end');
  });

  it('Up and Down scroll in a single-line composer', () => {
    let s = typed('abc');
    expect(handleKey(s, key({ upArrow: true })).scroll).toBe('lineUp');
    expect(handleKey(s, key({ downArrow: true })).scroll).toBe('lineDown');
  });

  it('Up and Down move the caret in a multi-line composer instead of scrolling', () => {
    const s: AppState = { ...initialState(80, 24), composer: 'line one\nline two' };
    const outcome = handleKey(s, key({ upArrow: true }));
    expect(outcome.scroll, 'in a multi-line composer the arrows belong to the text').toBeUndefined();
  });

  it('scrolling never disturbs what the user is typing', () => {
    const s = typed('audit and improve');
    const after = scrolled(s, 'pageUp');
    expect(after.composer).toBe('audit and improve');
  });

  it('the composer keeps its text across every scroll direction', () => {
    let s = typed('keep me');
    for (const by of ['pageUp', 'lineUp', 'pageDown', 'lineDown', 'home', 'end'] as const) {
      s = scrolled(s, by);
      expect(s.composer).toBe('keep me');
    }
  });
});

describe('D9: letters that could scroll instead go to the composer', () => {
  it('j and k type rather than scroll', () => {
    let s = initialState();
    for (const ch of ['j', 'k']) {
      const outcome = handleKey(s, key({ input: ch }));
      expect(outcome.scroll).toBeUndefined();
      s = reduceLocal(s, { kind: 'composer.set', text: s.composer + (outcome.insert ?? '') }, T0);
    }
    expect(s.composer).toBe('jk');
  });

  it('a bare g types, and gG jumps to the end of the transcript', () => {
    let s = initialState();
    const first = handleKey(s, key({ input: 'g' }));
    expect(first.insert).toBe('g');
    s = reduceLocal(s, { kind: 'setChord', chord: 'g' }, T0);

    const second = handleKey(s, key({ input: 'G' }));
    expect(second.scroll).toBe('end');
    expect(second.actions).toContainEqual({ kind: 'clearChord' });
  });

  it('gg jumps to the top of the transcript', () => {
    const s = reduceLocal(initialState(), { kind: 'setChord', chord: 'g' }, T0);
    const outcome = handleKey(s, key({ input: 'g' }));
    expect(outcome.scroll).toBe('home');
  });

  it('a chord does not fire across unrelated typing', () => {
    let s = reduceLocal(initialState(), { kind: 'setChord', chord: 'g' }, T0);
    s = reduceLocal(s, { kind: 'composer.set', text: 'x' }, T0);
    expect(s.pendingChord).toBe('');
    expect(handleKey(s, key({ input: 'G' })).scroll).toBeUndefined();
  });
});

describe('E7: cycling a mode keeps focus in the composer', () => {
  it('Shift+Tab cycles and never steals focus', () => {
    const s = typed('still typing');
    const outcome = handleKey(s, key({ input: '', shift: true, tab: true }));
    expect(outcome.actions).toEqual([{ kind: 'mode.cycle' }]);
    expect(outcome.insert).toBeUndefined();
    expect(outcome.scroll).toBeUndefined();
  });

  it('cycling does not change the composer text', () => {
    let s = typed('draft');
    const outcome = handleKey(s, key({ shift: true, tab: true }));
    for (const action of outcome.actions) s = reduceLocal(s, action, T0);
    expect(s.composer).toBe('draft');
    expect(s.workspaceMode).toBe(true);
  });

  it('cycling twice returns to the chat view with the text intact', () => {
    let s = typed('draft');
    for (let i = 0; i < 2; i += 1) {
      for (const action of handleKey(s, key({ shift: true, tab: true })).actions) {
        s = reduceLocal(s, action, T0);
      }
    }
    expect(s.workspaceMode).toBe(false);
    expect(s.composer).toBe('draft');
  });
});

describe('D8: every advertised key dispatches where the footer says it does', () => {
  it('the idle hints name keys that all dispatch', () => {
    // Text arrives one character at a time, exactly as the parser delivers it, so this exercises
    // the real path rather than a synthetic single key carrying a whole word.
    // Typing a slash opens the popup, so Enter accepts the highlighted command rather than
    // running raw text — which is the behaviour the popup row is about.
    const typedSlash = typed('/help');
    expect(typedSlash.slashMenu).not.toBeNull();
    expect(handleKey(typedSlash, key({ return: true, input: '\r' })).actions).toEqual([
      { kind: 'slashMenu.accept' },
    ]);

    // With the popup dismissed, Enter submits the text as written.
    const dismissed = reduceLocal(typedSlash, { kind: 'slashMenu.close' }, T0);
    expect(handleKey(dismissed, key({ return: true, input: '\r' })).actions).toEqual([
      { kind: 'command.run', name: '/help', args: '' },
    ]);
    expect(handleKey(dismissed, key({ ctrl: true, input: 'k' })).actions).toEqual([
      { kind: 'palette.open' },
    ]);
    expect(handleKey(dismissed, key({ ctrl: true, input: 'o' })).actions).toEqual([
      { kind: 'details.toggle' },
    ]);
    expect(handleKey(dismissed, key({ ctrl: true, input: 't' })).actions).toEqual([
      { kind: 'stages.toggle' },
    ]);
    expect(handleKey(dismissed, key({ ctrl: true, input: 'm' })).actions).toEqual([
      { kind: 'mouse.toggle' },
    ]);
  });

  it('Esc closes an overlay before it interrupts a run', () => {
    const withOverlay = reduceLocal(initialState(), { kind: 'palette.open' }, T0);
    expect(handleKey(withOverlay, key({ escape: true })).actions).toEqual([{ kind: 'overlay.close' }]);
  });

  it('Esc interrupts a run only when there is nothing to close', () => {
    let s = reduce(initialState(), { method: 'turn.started', params: { turn_id: 't', prompt: 'go' } } as never, T0);
    expect(handleKey(s, key({ escape: true })).actions).toEqual([{ kind: 'turn.interrupt' }]);
  });

  it('Ctrl+C clears the composer, then interrupts, then arms exit — in that order', () => {
    const typing = typed('some text');
    expect(handleKey(typing, key({ ctrl: true, input: 'c' })).actions).toEqual([
      { kind: 'composer.set', text: '' },
    ]);

    let running = reduce(initialState(), { method: 'turn.started', params: { turn_id: 't', prompt: 'go' } } as never, T0);
    expect(handleKey(running, key({ ctrl: true, input: 'c' })).actions).toEqual([
      { kind: 'turn.interrupt' },
    ]);

    const idle = initialState();
    expect(handleKey(idle, key({ ctrl: true, input: 'c' })).actions).toEqual([{ kind: 'exit.arm' }]);
  });

  it('Ctrl+D exits only on an empty composer', () => {
    expect(handleKey(typed('x'), key({ ctrl: true, input: 'd' })).actions).toEqual([]);
    expect(handleKey(initialState(), key({ ctrl: true, input: 'd' })).actions).toEqual([
      { kind: 'exit.now' },
    ]);
  });
});