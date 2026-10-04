/**
 * The one input parser.
 *
 * Bytes from the terminal in, {@link KeyEvent}s out. Nothing else in the shell is allowed to look
 * at a raw byte or an escape sequence, which is what makes "split sequences, lone Esc versus
 * Alt+key, non-ASCII and AltGr" a property of one testable file rather than a habit.
 *
 * The parser is incremental and holds a partial sequence until the rest arrives, so a paste that
 * arrives in two reads does not become two bogus keys. `flush` forces a stuck prefix to resolve as
 * whatever it is rather than swallowing input forever.
 */

import type { KeyEvent } from './dispatch.js';

const ESC = '\x1b';

const BASE: KeyEvent = {
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
};

/** CSI sequences we understand, keyed by their final byte. */
const CSI_FINAL: Record<string, Partial<KeyEvent>> = {
  A: { upArrow: true },
  B: { downArrow: true },
  C: { rightArrow: true },
  D: { leftArrow: true },
  H: { home: true },
  F: { end: true },
  '~': {},
};

const CSI_TILDE: Record<string, Partial<KeyEvent>> = {
  '1': { home: true },
  '3': { delete: true },
  '4': { end: true },
  '5': { pageUp: true },
  '6': { pageDown: true },
  '7': { home: true },
  '8': { end: true },
};

/**
 * Tracks a partial escape sequence across reads. `push` is fed whatever the tty hands us;
 * `pending` is true when bytes were withheld because a sequence is still incomplete.
 */
export class InputParser {
  #buffer = '';

  /** Feeds bytes and returns every complete key event they produced. */
  push(chunk: string): KeyEvent[] {
    this.#buffer += chunk;
    const events: KeyEvent[] = [];

    while (this.#buffer.length > 0) {
      const consumed = this.#takeOne(events);
      if (!consumed) break; // an incomplete sequence: wait for the rest
    }
    return events;
  }

  /** True when bytes are held back waiting for the rest of a sequence. */
  get pending(): boolean {
    return this.#buffer.length > 0;
  }

  /**
   * Resolves a stuck prefix. A lone Esc that never grew into a sequence is reported as Escape
   * rather than being swallowed, which is what makes Esc usable to dismiss an overlay.
   */
  flush(): KeyEvent[] {
    const events: KeyEvent[] = [];
    if (this.#buffer === ESC) {
      events.push({ ...BASE, escape: true });
      this.#buffer = '';
      return events;
    }
    if (this.#buffer.startsWith(ESC)) {
      // An unrecognised CSI: consume it rather than emitting its bytes as text.
      events.push({ ...BASE, escape: true });
      this.#buffer = '';
      return events;
    }
    this.#buffer = '';
    return events;
  }

  /** Returns true when at least one key was emitted. */
  #takeOne(out: KeyEvent[]): boolean {
    const first = this.#buffer[0]!;

    if (first !== ESC) {
      const ch = this.#buffer[0]!;
      this.#buffer = this.#buffer.slice(1);
      out.push(keyForChar(ch));
      return true;
    }

    // ESC followed by `[` is CSI; ESC O is the application-cursor form.
    if (this.#buffer.length === 1) return false; // could be Esc, could be the start of a sequence
    const second = this.#buffer[1]!;

    if (second === '[') return this.#takeCsi(out);
    if (second === 'O') {
      if (this.#buffer.length < 3) return false;
      const final = this.#buffer[2]!;
      this.#buffer = this.#buffer.slice(3);
      out.push({ ...BASE, ...(CSI_FINAL[final] ?? {}), meta: false });
      return true;
    }

    // ESC <char> is Alt+<char>. ESC ESC is a literal Esc.
    if (second === ESC) {
      this.#buffer = this.#buffer.slice(1);
      out.push({ ...BASE, escape: true });
      return true;
    }
    // Consume both the ESC and the character: leaving the character behind would emit it a
    // second time as its own key.
    this.#buffer = this.#buffer.slice(2);
    out.push({ ...BASE, ...keyForChar(second), meta: true, escape: false });
    return true;
  }

  /** `ESC [ params final` — arrows, Home/End, and the `~` family (PgUp, PgDn, Delete). */
  #takeCsi(out: KeyEvent[]): boolean {
    let i = 2;
    while (i < this.#buffer.length) {
      const ch = this.#buffer[i]!;
      const code = ch.charCodeAt(0);
      const isParam = code >= 0x30 && code <= 0x3f;
      const isIntermediate = code >= 0x20 && code <= 0x2f;
      if (isParam || isIntermediate) {
        i += 1;
        continue;
      }
      // Final byte.
      const params = this.#buffer.slice(2, i);
      this.#buffer = this.#buffer.slice(i + 1);
      const modifier = parseModifier(params);
      // With no parameters the final byte *is* the key (`ESC [ A` is Up). With parameters, xterm's
      // convention puts the key number in the first parameter and the modifier in the second
      // (`ESC [ 1;5D` is ctrl+Left, `ESC [ 5~` is PgUp).
      const key =
        params === '' || params.includes(';')
          ? // no parameters, or a modifier list: the final byte names the key
            (CSI_FINAL[ch] ?? {})
          : // a bare parameter number: the `~` family, including Home and End
            (CSI_TILDE[params] ?? {});
      out.push({ ...BASE, ...key, ...modifier });
      return true;
    }
    return false; // incomplete: hold the bytes
  }
}

/** xterm encodes ctrl/shift/meta in the CSI modifier parameter, e.g. `1;5D` is ctrl+Left. */
function parseModifier(params: string): Partial<KeyEvent> {
  const parts = params.split(';');
  if (parts.length < 2) return {};
  const m = Number.parseInt(parts[1] ?? '', 10);
  if (Number.isNaN(m) || m <= 1) return {};
  return {
    shift: (m - 1) % 2 === 1,
    meta: Math.floor((m - 1) / 2) % 2 === 1,
    ctrl: Math.floor((m - 1) / 4) % 2 === 1,
  };
}

function keyForChar(ch: string): KeyEvent {
  switch (ch) {
    case '\r':
    case '\n':
      return { ...BASE, input: '\r', return: true };
    case '\t':
      return { ...BASE, input: '\t', tab: true };
    case '\b':
    case '\x7f':
      return { ...BASE, input: '', backspace: true };
    default:
      break;
  }
  if (ch < ' ') return { ...BASE, input: '' };
  return { ...BASE, input: ch };
}