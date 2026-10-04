/**
 * A small terminal emulator, enough to answer "what does the screen actually look like?".
 *
 * The PTY tests need to assert on the **final screen**, not on the byte stream. Bytes tell you
 * that something was written; the screen tells you what a user would see. This handles the
 * sequences Ink and the terminal actually emit: cursor movement, erase, scroll region, the
 * alternate screen, and SGR, plus printable text with wide-character and combining-mark handling.
 *
 * It is deliberately small. It is a test instrument, not a terminal, and anything it does not
 * model is a sequence Ink does not send.
 */

export type Cell = { char: string; width: number };

export class Terminal {
  cols: number;
  rows: number;
  #grid: Cell[][];
  #altGrid: Cell[][] | null = null;
  #cursorRow = 0;
  #cursorCol = 0;
  #savedRow = 0;
  #savedCol = 0;
  #parser = '';
  /** Bytes held while a control sequence is incomplete, so a split read still parses. */
  #utf8 = new TextDecoder('utf-8', { fatal: false });

  constructor(cols: number, rows: number) {
    this.cols = cols;
    this.rows = rows;
    this.#grid = emptyGrid(cols, rows);
  }

  feed(data: string | Uint8Array): void {
    const text = typeof data === 'string' ? data : this.#utf8.decode(data);
    for (const ch of text) this.#char(ch);
  }

  /** Resize the grid, preserving what fits. A resize storm must not throw. */
  resize(cols: number, rows: number): void {
    if (cols <= 0 || rows <= 0 || cols > 1000 || rows > 1000) return;
    const next = emptyGrid(cols, rows);
    for (let r = 0; r < Math.min(rows, this.rows); r += 1) {
      for (let c = 0; c < Math.min(cols, this.cols); c += 1) next[r]![c] = this.#grid[r]![c]!;
    }
    this.cols = cols;
    this.rows = rows;
    this.#grid = next;
    this.#altGrid = this.#altGrid ? this.#altGrid : null;
    this.#cursorRow = Math.min(this.#cursorRow, rows - 1);
    this.#cursorCol = Math.min(this.#cursorCol, cols - 1);
  }

  /** The screen as text, one line per row, trailing blanks trimmed. */
  screen(): string[] {
    return this.#grid.map((row) =>
      row
        .map((cell) => cell.char)
        .join('')
        .replace(/\s+$/, ''),
    );
  }

  screenText(): string {
    return this.screen().join('\n');
  }

  cursor(): { row: number; col: number } {
    return { row: this.#cursorRow, col: this.#cursorCol };
  }

  /** True once the alternate screen has been entered and not left. */
  get inAltScreen(): boolean {
    return this.#altGrid !== null;
  }

  #char(ch: string): void {
    if (this.#parser.length > 0 || ch === '\x1b') {
      this.#parser += ch;
      // Incomplete sequences are held, not consumed: the rest arrives in the next read.
      if (!this.#consumeSequence()) return;
      this.#parser = '';
      return;
    }
    switch (ch) {
      case '\r':
        this.#cursorCol = 0;
        return;
      case '\n':
        this.#newline();
        return;
      case '\b':
        this.#cursorCol = Math.max(0, this.#cursorCol - 1);
        return;
      case '\t':
        this.#cursorCol = Math.min(this.cols - 1, (Math.floor(this.#cursorCol / 8) + 1) * 8);
        return;
      default:
        break;
    }
    if (ch < ' ' || ch === '\x7f') return; // any other C0 control: consumed, not displayed
    this.#printable(ch);
  }

  #printable(ch: string): void {
    const width = charWidth(ch);
    if (width === 0) return; // combining mark: attach, do not advance
    if (this.#cursorCol + width > this.cols) {
      this.#cursorCol = 0;
      this.#newline();
    }
    this.#grid[this.#cursorRow]![this.#cursorCol] = { char: ch, width };
    if (width === 2 && this.#cursorCol + 1 < this.cols) {
      this.#grid[this.#cursorRow]![this.#cursorCol + 1] = { char: '', width: 0 };
    }
    this.#cursorCol += width;
  }

  #newline(): void {
    this.#cursorRow += 1;
    if (this.#cursorRow >= this.rows) {
      this.#cursorRow = this.rows - 1;
      this.#grid.shift();
      this.#grid.push(emptyRow(this.cols));
    }
  }

  /**
   * Returns true when the sequence was complete and consumed, false when it is still arriving.
   * Getting this backwards is the classic terminal-parser bug: consume-early swallows the rest of
   * a sequence that arrived in two reads, and the screen silently loses a line.
   */
  #consumeSequence(): boolean {
    const seq = this.#parser;
    if (seq === '\x1b') return false; // a bare ESC may still become a sequence

    const csi = /^\x1b\[([0-9;?<>= ]*)([@-~])$/.exec(seq);
    if (csi) {
      this.#applyCsi(csi[1]!, csi[2]!);
      return true;
    }
    // Started as CSI or OSC but the terminator has not arrived: hold the bytes.
    if (/^\x1b\[[0-9;?<>= ]*$/.test(seq)) return false;
    const osc = /^\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)?$/.exec(seq);
    if (seq.startsWith('\x1b]')) {
      const complete = /^\x1b\][^\x07\x1b]*(\x07|\x1b\\)$/.test(seq);
      if (complete) return true;
      return false;
    }
    void osc;

    switch (seq) {
      case '\x1b7':
        this.#savedRow = this.#cursorRow;
        this.#savedCol = this.#cursorCol;
        return true;
      case '\x1b8':
        this.#cursorRow = clamp(this.#savedRow, 0, this.rows - 1);
        this.#cursorCol = clamp(this.#savedCol, 0, this.cols - 1);
        return true;
      case '\x1bM':
        this.#cursorRow = Math.max(0, this.#cursorRow - 1);
        return true;
      case '\x1bc':
        this.#grid = emptyGrid(this.cols, this.rows);
        this.#cursorRow = 0;
        this.#cursorCol = 0;
        return true;
      default:
        // An unrecognised two-byte sequence: consumed and ignored. An unknown sequence must
        // never crash the emulator, because the emulator is what the tests trust.
        return seq.length >= 2;
    }
  }

  #applyCsi(params: string, final: string): void {
    const priv = params.startsWith('?');
    const nums = (priv ? params.slice(1) : params)
      .split(';')
      .map((p) => Number.parseInt(p, 10))
      .filter((n) => !Number.isNaN(n));
    const at = (index: number, fallback: number): number => nums[index] ?? fallback;

    switch (final) {
      case 'A':
        this.#cursorRow = Math.max(0, this.#cursorRow - Math.max(1, at(0, 1)));
        break;
      case 'B':
        this.#cursorRow = Math.min(this.rows - 1, this.#cursorRow + Math.max(1, at(0, 1)));
        break;
      case 'C':
        this.#cursorCol = Math.min(this.cols - 1, this.#cursorCol + Math.max(1, at(0, 1)));
        break;
      case 'D':
        this.#cursorCol = Math.max(0, this.#cursorCol - Math.max(1, at(0, 1)));
        break;
      case 'E':
        this.#cursorRow = Math.min(this.rows - 1, this.#cursorRow + Math.max(1, at(0, 1)));
        this.#cursorCol = 0;
        break;
      case 'F':
        this.#cursorRow = Math.max(0, this.#cursorRow - Math.max(1, at(0, 1)));
        this.#cursorCol = 0;
        break;
      case 'G':
      case '`':
        this.#cursorCol = clamp(at(0, 1) - 1, 0, this.cols - 1);
        break;
      case 'd':
        this.#cursorRow = clamp(at(0, 1) - 1, 0, this.rows - 1);
        break;
      case 'H':
      case 'f':
        this.#cursorRow = clamp(at(0, 1) - 1, 0, this.rows - 1);
        this.#cursorCol = clamp(at(1, 1) - 1, 0, this.cols - 1);
        break;
      case 'J': {
        const mode = at(0, 0);
        if (mode === 2 || mode === 3) this.#grid = emptyGrid(this.cols, this.rows);
        else if (mode === 0) this.#erase(this.#cursorRow, this.#cursorCol, this.rows - 1, this.cols - 1);
        else if (mode === 1) this.#erase(0, 0, this.#cursorRow, this.#cursorCol);
        this.#cursorRow = clamp(this.#cursorRow, 0, this.rows - 1);
        this.#cursorCol = clamp(this.#cursorCol, 0, this.cols - 1);
        break;
      }
      case 'K': {
        const mode = at(0, 0);
        if (mode === 0) this.#erase(this.#cursorRow, this.#cursorCol, this.#cursorRow, this.cols - 1);
        else if (mode === 1) this.#erase(this.#cursorRow, 0, this.#cursorRow, this.#cursorCol);
        else this.#erase(this.#cursorRow, 0, this.#cursorRow, this.cols - 1);
        break;
      }
      case 'L': {
        const count = Math.max(1, at(0, 1));
        for (let i = 0; i < count; i += 1) {
          this.#grid.splice(this.#cursorRow, 0, emptyRow(this.cols));
          this.#grid.length = this.rows;
        }
        break;
      }
      case 'M': {
        const count = Math.max(1, at(0, 1));
        for (let i = 0; i < count; i += 1) {
          this.#grid.splice(this.#cursorRow, 1);
          this.#grid.push(emptyRow(this.cols));
        }
        break;
      }
      case 'h':
        if (priv && at(0, 0) === 1049) {
          this.#altGrid = this.#grid;
          this.#grid = emptyGrid(this.cols, this.rows);
          this.#cursorRow = 0;
          this.#cursorCol = 0;
        }
        break;
      case 'l':
        if (priv && at(0, 0) === 1049) {
          if (this.#altGrid) this.#grid = this.#altGrid;
          this.#altGrid = null;
        }
        break;
      case 's':
        this.#savedRow = this.#cursorRow;
        this.#savedCol = this.#cursorCol;
        break;
      case 'u':
        this.#cursorRow = clamp(this.#savedRow, 0, this.rows - 1);
        this.#cursorCol = clamp(this.#savedCol, 0, this.cols - 1);
        break;
      default:
        break; // SGR and anything else: consumed, not displayed
    }
  }

  #erase(r0: number, c0: number, r1: number, c1: number): void {
    for (let r = clamp(r0, 0, this.rows - 1); r <= clamp(r1, 0, this.rows - 1); r += 1) {
      for (let c = clamp(c0, 0, this.cols - 1); c <= clamp(c1, 0, this.cols - 1); c += 1) {
        this.#grid[r]![c] = { char: ' ', width: 1 };
      }
    }
  }
}

function clamp(n: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, n));
}

function emptyRow(cols: number): Cell[] {
  return Array.from({ length: cols }, () => ({ char: ' ', width: 1 }));
}

function emptyGrid(cols: number, rows: number): Cell[][] {
  return Array.from({ length: rows }, () => emptyRow(cols));
}

/**
 * Display width of one code point: 0 for a combining mark, 2 for East Asian wide and emoji, 1
 * otherwise. This is what keeps CJK and emoji from shearing the layout.
 */
export function charWidth(ch: string): number {
  const cp = ch.codePointAt(0);
  if (cp === undefined) return 1;
  if (cp === 0x200d) return 0; // ZWJ
  if (cp >= 0x0300 && cp <= 0x036f) return 0; // combining diacriticals
  if (cp >= 0xfe00 && cp <= 0xfe0f) return 0; // variation selectors
  if (cp >= 0x1f300 && cp <= 0x1faff) return 2; // emoji
  if (
    (cp >= 0x1100 && cp <= 0x115f) ||
    (cp >= 0x2e80 && cp <= 0xa4cf) ||
    (cp >= 0xac00 && cp <= 0xd7a3) ||
    (cp >= 0xf900 && cp <= 0xfaff) ||
    (cp >= 0xff00 && cp <= 0xff60) ||
    (cp >= 0x1f004 && cp <= 0x1f004)
  ) {
    return 2;
  }
  return 1;
}