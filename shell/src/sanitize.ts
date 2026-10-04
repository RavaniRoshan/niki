/**
 * The one place untrusted text is cleaned.
 *
 * Every string that came from the engine — a tool name, a diff line, a notice, a branch name, a
 * model-supplied summary — passes through {@link sanitize} before any widget sees it. A hostile
 * fixture that carries an OSC title-spoof, a CSI cursor move or a raw C1 byte must never reach
 * the terminal.
 *
 * Kept: newline and tab, because the transcript and code blocks need them.
 * Removed: everything else in C0 (except `\n` and `\t`), all of C1, and every escape sequence —
 * CSI, OSC, DCS, SOS, PM, APC and a bare ESC. A sequence that is cut off mid-way is dropped to
 * the end of the string, never half-rendered.
 */

const ESC = '';

/** Characters we deliberately keep. */
const KEEP = new Set(['\n', '\t']);

/**
 * Escape sequences we must consume *completely*, not just strip the introducer. Each is an
 * introducer byte followed by parameter/intermediate bytes and a terminator.
 */
/**
 * CSI ends at a final byte in 0x40..0x7E. That rule is specific to CSI: OSC, DCS, SOS, PM and APC
 * bodies run until BEL or ST, and applying the CSI rule to them truncates a title like
 * `]0;PWNED` at the first letter that happens to fall in the final-byte range.
 */
function isCsiFinal(code: number): boolean {
  return code >= 0x40 && code <= 0x7e;
}

/**
 * Strips every control sequence from `input` and returns text that is safe to render.
 *
 * The result is not necessarily printable: a lone high-surrogate or an invalid sequence is
 * replaced with U+FFFD so the renderer cannot be handed a broken string.
 */
export function sanitize(input: string): string {
  let out = '';
  let i = 0;
  const n = input.length;

  while (i < n) {
    const ch = input[i]!;

    if (ch === ESC) {
      i = skipEscape(input, i);
      continue;
    }

    // C1 controls are single characters in U+0080..U+009F. Two of them introduce a sequence
    // whose body carries no ESC, so the body has to be consumed too — with each introducer's own
    // terminator rule, not the CSI one.
    if (ch >= '' && ch <= '') {
      if (ch === '') {
        i = skipCsi(input, i + 1);
      } else if (''.includes(ch)) {
        i = skipUntilBelOrSt(input, i + 1);
      } else {
        i += 1;
      }
      continue;
    }

    // Other C0 controls, plus DEL.
    const code = ch.charCodeAt(0);
    if ((code < 0x20 && !KEEP.has(ch)) || code === 0x7f) {
      i += 1;
      continue;
    }

    // Lone surrogates would produce invalid UTF-8 downstream.
    const isHighSurrogate = code >= 0xd800 && code <= 0xdbff;
    const isLowSurrogate = code >= 0xdc00 && code <= 0xdfff;
    if (isHighSurrogate) {
      const next = input[i + 1];
      if (next !== undefined) {
        const nextCode = next.charCodeAt(0);
        if (nextCode >= 0xdc00 && nextCode <= 0xdfff) {
          out += ch + next;
          i += 2;
          continue;
        }
      }
      out += '�';
      i += 1;
      continue;
    }
    if (isLowSurrogate) {
      out += '�';
      i += 1;
      continue;
    }

    out += ch;
    i += 1;
  }

  return out;
}

/** Returns the index just past an escape sequence starting at `start`. */
function skipEscape(input: string, start: number): number {
  const next = input[start + 1];
  if (next === undefined) return start + 1; // a trailing lone ESC is simply dropped

  // ESC [ ... final — CSI.
  if (next === '[') return skipCsi(input, start + 2);
  // ESC ] ... BEL | ESC \  — OSC: window title, clipboard.
  if (next === ']') return skipUntilBelOrSt(input, start + 2);
  // ESC P, ESC X, ESC ^, ESC _ — DCS, SOS, PM, APC. All end at ST.
  if (next === 'P' || next === 'X' || next === '^' || next === '_') {
    return skipUntilBelOrSt(input, start + 2);
  }
  // ESC ( B, ESC # 8 — two-byte character set selects.
  if (next === '(' || next === ')' || next === '*' || next === '+' || next === '#') {
    return Math.min(start + 3, input.length);
  }
  // ESC followed by a single ordinary byte (RIS, DECSC, and friends).
  return start + 2;
}

/** CSI body: parameter and intermediate bytes, then exactly one final byte. */
function skipCsi(input: string, start: number): number {
  let i = start;
  while (i < input.length) {
    if (isCsiFinal(input.charCodeAt(i))) return i + 1;
    i += 1;
  }
  return input.length;
}

/**
 * String-sequence body: runs until BEL or ST (ESC backslash). If the input ends first, everything
 * to the end is dropped, so a truncated sequence is never partially rendered.
 */
function skipUntilBelOrSt(input: string, start: number): number {
  let i = start;
  while (i < input.length) {
    const ch = input[i]!;
    if (ch === '\u0007') return i + 1;
    if (ch === ESC && input[i + 1] === '\\') return i + 2;
    i += 1;
  }
  return input.length;
}

/** Sanitises and collapses to a single line, for a row that must not wrap into the next one. */
const HARD_LINE_LIMIT = 4096;

export function sanitizeSingleLine(input: string, maxColumns?: number): string {
  const flat = sanitize(input).replace(/[\r\n\t]+/g, ' ').replace(/\s{2,}/g, ' ').trim();
  // A single token longer than this cannot be laid out at any terminal width, and letting it
  // through is what turns one hostile payload into a frame that costs seconds to measure.
  if (flat.length > HARD_LINE_LIMIT) {
    return `${flat.slice(0, HARD_LINE_LIMIT)}…`;
  }
  // Width-aware truncation is the renderer's job: it knows the display width of CJK and emoji.
  // The optional argument only shortens further.
  if (maxColumns !== undefined && flat.length > maxColumns) {
    return `${flat.slice(0, Math.max(0, maxColumns - 1))}…`;
  }
  return flat;
}

/** Sanitises, then hard-limits to `max` characters. Used for tool summaries and error excerpts. */
export function sanitizeClipped(input: string, max: number): string {
  const flat = sanitizeSingleLine(input);
  return flat.length > max ? `${flat.slice(0, Math.max(0, max - 1))}…` : flat;
}