/**
 * Sanitisation: hostile engine text must never reach the terminal.
 *
 * Every case here is a real escape sequence a hostile or buggy engine could emit. The rule under
 * test is one sentence: all untrusted text passes one sanitizer before reaching any widget.
 */

import { describe, expect, it } from 'vitest';
import { sanitize, sanitizeClipped, sanitizeSingleLine } from '../src/sanitize.js';

describe('sanitize', () => {
  it('keeps newline and tab', () => {
    expect(sanitize('a\nb\tc')).toBe('a\nb\tc');
  });

  it('removes a CSI colour sequence', () => {
    expect(sanitize('[31mred[0m')).toBe('red');
  });

  it('removes an OSC window-title spoof', () => {
    expect(sanitize('before]0;PWNEDafter')).toBe('beforeafter');
  });

  it('removes an OSC 52 clipboard write', () => {
    expect(sanitize('x]52;c;BASE64\\y')).toBe('xy');
  });

  it('removes a DCS payload', () => {
    expect(sanitize('aP+q544e\\b')).toBe('ab');
  });

  it('removes a cursor-position CSI', () => {
    expect(sanitize('x[2J[H y')).toBe('x y');
  });

  it('removes a bare ESC and a two-byte charset select', () => {
    expect(sanitize('a(Bb')).toBe('ab');
  });

  it('removes C1 controls, including the 8-bit CSI introducer', () => {
    expect(sanitize('a31mbc')).toBe('abc');
  });

  it('removes other C0 controls but keeps tab and newline', () => {
    expect(sanitize('abc\td\nef')).toBe('abc\td\nef');
  });

  it('drops a truncated sequence rather than rendering half of it', () => {
    // No terminator: everything after the introducer is consumed, not leaked.
    expect(sanitize('safe[31m and then the string just stops')).toBe('safe and then the string just stops');
  });

  it('replaces a lone surrogate rather than emitting invalid text', () => {
    const lone = `a${String.fromCharCode(0xd800)}b`;
    const out = sanitize(lone);
    expect(out).toBe('a�b');
  });

  it('keeps a valid surrogate pair intact', () => {
    const emoji = '😀';
    expect(sanitize(`a${emoji}b`)).toBe(`a${emoji}b`);
  });

  it('is idempotent', () => {
    const once = sanitize('[31mx]0;ty');
    expect(sanitize(once)).toBe(once);
  });

  it('handles a payload of nothing but escapes', () => {
    expect(sanitize('[31m[1m[0m')).toBe('');
  });
});

describe('sanitizeSingleLine', () => {
  it('flattens newlines and tabs so a row cannot break the layout', () => {
    expect(sanitizeSingleLine('a\nb\tc')).toBe('a b c');
  });

  it('strips escapes before flattening', () => {
    expect(sanitizeSingleLine('[31mred[0m\nnext')).toBe('red next');
  });

  it('truncates an absurdly long single token', () => {
    expect(sanitizeSingleLine('x'.repeat(9000))).toHaveLength(4097);
  });
});

describe('sanitizeClipped', () => {
  it('clips to the requested width with an ellipsis', () => {
    const out = sanitizeClipped('abcdefghij', 5);
    expect(out).toHaveLength(5);
    expect(out.endsWith('…')).toBe(true);
  });
});