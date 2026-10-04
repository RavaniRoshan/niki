/**
 * Property and fuzz tests.
 *
 * Three invariants that must hold for *any* input, not just the cases someone thought of:
 *  1. The input parser never wedges. Random bytes and split sequences always resolve.
 *  2. The sanitiser never emits a control sequence, whatever it is handed.
 *  3. The reducer renders at any terminal size without throwing, and every row it produces is one
 *     an event created.
 */

import { describe, expect, it } from 'vitest';
import { InputParser } from '../src/input.js';
import { handleKey } from '../src/dispatch.js';
import { sanitize, sanitizeSingleLine } from '../src/sanitize.js';
import { initialState, reduce, type AppState } from '../src/state.js';
import { renderTranscriptLines } from '../src/components/transcript.js';
import { App } from '../src/app.js';
import { render } from 'ink-testing-library';
import React from 'react';

/** A deterministic PRNG, so a failure is reproducible from the seed in the message. */
function rng(seed: number): () => number {
  let s = seed >>> 0 || 1;
  return () => {
    s ^= s << 13;
    s ^= s >>> 17;
    s ^= s << 5;
    s >>>= 0;
    return s / 0xffffffff;
  };
}

function randomBytes(n: number, seed: number): string {
  const next = rng(seed);
  let out = '';
  for (let i = 0; i < n; i += 1) {
    const r = next();
    // Deliberately biased toward the bytes that break naive parsers.
    if (r < 0.25) out += '\x1b';
    else if (r < 0.4) out += String.fromCharCode(0x30 + Math.floor(next() * 10));
    else if (r < 0.5) out += ';';
    else if (r < 0.6) out += '~';
    else if (r < 0.7) out += String.fromCharCode(Math.floor(next() * 32));
    else out += String.fromCharCode(0x41 + Math.floor(next() * 58));
  }
  return out;
}

describe('input parser: never wedges', () => {
  it('resolves every one of 300 random byte strings', () => {
    for (let seed = 1; seed <= 300; seed += 1) {
      const parser = new InputParser();
      const bytes = randomBytes(24, seed);
      const keys = parser.push(bytes);
      // Whatever came out, flushing must return the parser to an idle state.
      keys.push(...parser.flush());
      expect(parser.pending, `seed ${seed} left the parser holding bytes`).toBe(false);
    }
  });

  it('resolves a sequence that arrives one byte at a time', () => {
    const parser = new InputParser();
    const full = '\x1b[5~'; // PgUp
    const keys = [];
    for (const ch of full) keys.push(...parser.push(ch));
    expect(keys).toHaveLength(1);
    expect(keys[0]?.pageUp).toBe(true);
  });

  it('resolves two sequences arriving in one read', () => {
    const parser = new InputParser();
    const keys = parser.push('\x1b[A\x1b[B');
    expect(keys.map((k) => [k.upArrow, k.downArrow])).toEqual([
      [true, false],
      [false, true],
    ]);
  });

  it('reports a lone Esc as Escape once flushed', () => {
    const parser = new InputParser();
    expect(parser.push('\x1b')).toEqual([]);
    expect(parser.flush()[0]?.escape).toBe(true);
  });

  it('reports Alt+key as meta, not as Escape plus a character', () => {
    const parser = new InputParser();
    const keys = parser.push('\x1bx');
    expect(keys).toHaveLength(1);
    expect(keys[0]?.meta).toBe(true);
    expect(keys[0]?.input).toBe('x');
  });

  it('handles the xterm modifier encoding for ctrl+arrow', () => {
    const parser = new InputParser();
    const keys = parser.push('\x1b[1;5D');
    expect(keys[0]?.ctrl).toBe(true);
    expect(keys[0]?.leftArrow).toBe(true);
  });

  it('never emits a key for an empty input', () => {
    const parser = new InputParser();
    expect(parser.push('')).toEqual([]);
  });
});

describe('sanitiser: no control sequence survives, whatever the input', () => {
  it('strips every sequence from 300 random byte strings', () => {
    for (let seed = 1; seed <= 300; seed += 1) {
      const out = sanitize(randomBytes(40, seed));
      // eslint-disable-next-line no-control-regex
      expect(out, `seed ${seed} leaked a control byte`).not.toMatch(/[\x00-\x08\x0b-\x1f\x7f-\x9f]/);
    }
  });

  it('is idempotent on random input', () => {
    for (let seed = 1; seed <= 200; seed += 1) {
      const once = sanitize(randomBytes(32, seed));
      expect(sanitize(once)).toBe(once);
    }
  });

  it('always returns a single line from sanitizeSingleLine', () => {
    for (let seed = 1; seed <= 200; seed += 1) {
      expect(sanitizeSingleLine(randomBytes(48, seed), 80)).not.toMatch(/[\r\n\t]/);
    }
  });
});

describe('render: any size, no panic', () => {
  const sizes: ReadonlyArray<readonly [number, number]> = [
    [1, 1],
    [2, 3],
    [5, 2],
    [13, 40],
    [49, 16],
    [80, 24],
    [119, 30],
    [180, 50],
    [300, 100],
  ];

  it.each(sizes)('renders the app at %ix%i', (cols, rows) => {
    const state = initialState(cols, rows);
    const instance = render(
      React.createElement(App, { state, theme: 'niki', charset: 'unicode', reducedMotion: true }),
    );
    expect(typeof instance.lastFrame()).toBe('string');
    instance.unmount();
  });

  it('renders a populated state at every size without a line wider than the terminal', () => {
    const state: AppState = {
      ...initialState(80, 24),
      phase: 'toolRunning',
      messages: [
        { kind: 'user', text: 'audit the tests', streaming: false },
        { kind: 'assistant', text: 'x'.repeat(500), streaming: true },
      ],
      tools: [
        { id: 'a', name: 'read', args: 'x'.repeat(200), state: 'running' },
        { id: 'b', name: 'bash', args: 'npm test', state: 'failed', summary: 'exit 1' },
      ],
      stages: [{ id: 'g', role: 'coder', attempt: 2, state: 'running', reasoning: ['thinking'] }],
      activity: { text: 'Running read', toolInFlight: true, startedAtMs: 0 },
    };
    for (const [cols, rows] of sizes) {
      const lines = renderTranscriptLines({
        state: { ...state, cols, rows },
        theme: 'niki',
        charset: 'unicode',
        reducedMotion: true,
        sweepTick: 0,
        height: Math.max(1, rows - 8),
      });
      for (const line of lines) {
        expect(line.text.length, `line overflows at ${cols}: ${line.text.slice(0, 40)}`).toBeLessThanOrEqual(cols);
      }
    }
  });
});

describe('reducer: every row is created by an event', () => {
  it('adds at most one row per tool.call and one per stage.start', () => {
    const opts = { nowMs: 0 };
    let s = initialState();
    for (let i = 0; i < 50; i += 1) {
      s = reduce(
        s,
        { method: 'tool.call', params: { tool_id: `t${i}`, name: 'read', args: 'a' } } as never,
        opts,
      );
      s = reduce(
        s,
        { method: 'stage.start', params: { stage_id: `s${i}`, role: 'coder', attempt: 1 } } as never,
        opts,
      );
    }
    expect(s.tools).toHaveLength(50);
    expect(s.stages).toHaveLength(50);
    expect(s.tools.every((t) => t.id.startsWith('t'))).toBe(true);
  });

  it('never grows the transcript without a turn event', () => {
    const s = reduce(
      initialState(),
      { method: 'tool.call', params: { tool_id: 't', name: 'read', args: 'a' } } as never,
      { nowMs: 0 },
    );
    expect(s.messages).toEqual([]);
  });
});
describe('D10: a bracketed paste is text, never a command', () => {
  it('delivers the whole paste as one insert', () => {
    const parser = new InputParser();
    const events = parser.push('\x1b[200~hello world\x1b[201~');
    expect(events).toHaveLength(1);
    expect(events[0]?.input).toBe('hello world');
    expect(events[0]?.paste).toBe(true);
  });

  it('never turns an Enter inside a paste into a submit', () => {
    const parser = new InputParser();
    const events = parser.push('\x1b[200~one\r\ntwo\x1b[201~');
    expect(events).toHaveLength(1);
    expect(events.some((e) => e.return)).toBe(false);
    expect(events[0]?.input).toContain('\n');
  });

  it('never turns a Ctrl+C inside a paste into an interrupt', () => {
    const parser = new InputParser();
    const events = parser.push('\x1b[200~danger\x03command\x1b[201~');
    expect(events.filter((e) => e.ctrl)).toHaveLength(0);
  });

  it('reassembles a paste split across several reads', () => {
    const parser = new InputParser();
    let events = parser.push('\x1b[200~first line\n');
    events = events.concat(parser.push('second line\n'));
    events = events.concat(parser.push('third'));
    events = events.concat(parser.push('\x1b[201~'));
    expect(events).toHaveLength(1);
    expect(events[0]?.input).toBe('first line\nsecond line\nthird');
  });

  it('collapses a very large paste to a placeholder with a preview', () => {
    const parser = new InputParser();
    const huge = Array.from({ length: 5000 }, (_, i) => `line ${i}`).join('\n');
    const events = parser.push(`\x1b[200~${huge}\x1b[201~`);
    expect(events[0]?.input).toContain('pasted 5000 lines');
    expect(events[0]?.input.length).toBeLessThan(400);
  });

  it('leaves the parser usable after a paste ends', () => {
    const parser = new InputParser();
    parser.push('\x1b[200~abc\x1b[201~');
    const after = parser.push('x');
    expect(after).toHaveLength(1);
    expect(after[0]?.input).toBe('x');
    expect(parser.pending).toBe(false);
  });

  it('the dispatcher inserts a paste and runs nothing', () => {
    const s = initialState(80, 24);
    const outcome = handleKey(s, {
      input: '/quit',
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
      paste: true,
    });
    expect(outcome.actions).toEqual([]);
    expect(outcome.insert).toBe('/quit');
  });
});
