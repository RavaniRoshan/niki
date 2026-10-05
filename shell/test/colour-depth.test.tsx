/**
 * Colour depth must not change what the interface *says*.
 *
 * The spec asks for correct rendering in truecolor, 256, 16 and NO_COLOR. Ink does the actual
 * downgrade — it is the terminal library's job, and re-implementing it here would be a second
 * colour path, which is exactly what the architecture forbids. So the property worth testing is
 * the one that survives any downgrade: the **text** is identical at every depth, and the frame
 * never gains an escape sequence that would show up as garbage on a 16-colour terminal.
 *
 * `FORCE_COLOR` is how a caller asks Ink for a level: 1 is 16-colour, 2 is 256, 3 is truecolor.
 */

import React from 'react';
import { render } from 'ink-testing-library';
import { describe, expect, it } from 'vitest';

import { App } from '../src/app.js';
import { initialState, reduce, type AppState, type ReduceOptions } from '../src/state.js';
import { glyphs } from '../src/glyphs.js';
import type { ServerNotification } from '../src/protocol/generated/index.js';

const T0: ReduceOptions = { nowMs: 0 };
const SIZES: ReadonlyArray<readonly [number, number]> = [
  [49, 16],
  [50, 16],
  [79, 24],
  [80, 24],
  [119, 30],
  [120, 38],
  [180, 50],
];
const DEPTHS = [
  { name: 'truecolor', env: { FORCE_COLOR: '3' } },
  { name: '256', env: { FORCE_COLOR: '2' } },
  { name: '16', env: { FORCE_COLOR: '1' } },
  { name: 'NO_COLOR', env: { NO_COLOR: '1', FORCE_COLOR: '0' } },
] as const;

function busyState(cols: number, rows: number): AppState {
  const events: ServerNotification[] = [
    {
      method: 'session.ready',
      params: {
        session_id: 's',
        project_path: '/home/u/projects/api',
        model: 'claude-sonnet-4',
        permission_mode: 'manual',
        branch: 'main',
        ahead: 1,
        behind: null,
        resumed_messages: 0,
      },
    },
    { method: 'turn.started', params: { turn_id: 't', prompt: 'audit the tests' } },
    { method: 'stage.start', params: { stage_id: 'g', role: 'coder', attempt: 2 } },
    { method: 'tool.call', params: { tool_id: 'x', name: 'Bash', args: 'npm test' } },
    {
      method: 'tool.result',
      params: { tool_id: 'x', ok: false, summary: 'exit 1 · 3 failing', full_ref: null, duration_ms: 900 },
    },
    { method: 'context.usage', params: { used: 31_000, limit: 262_000 } },
  ];
  let s: AppState = { ...initialState(cols, rows) };
  for (const e of events) s = reduce(s, e, T0);
  return s;
}

/** Renders once with the given environment, returning the raw frame Ink produced. */
function rawFrame(cols: number, rows: number, env: Record<string, string>): string {
  const previous = { ...process.env };
  Object.assign(process.env, env);
  try {
    const instance = render(
      <App state={busyState(cols, rows)} theme="niki" charset="unicode" reducedMotion />,
    );
    const frame = instance.lastFrame() ?? '';
    instance.unmount();
    return frame;
  } finally {
    for (const key of Object.keys(process.env)) {
      if (!(key in previous)) delete process.env[key];
    }
    Object.assign(process.env, previous);
  }
}

/** Strips SGR so two depths can be compared on what they actually say. */
function textOnly(frame: string): string {
  // eslint-disable-next-line no-control-regex
  return frame.replace(/\[[0-9;]*m/g, '');
}

describe('colour depth: truecolor, 256, 16 and NO_COLOR', () => {
  it.each(DEPTHS)('renders every size at $name', ({ env }) => {
    for (const [cols, rows] of SIZES) {
      const text = textOnly(rawFrame(cols, rows, env));
      expect(text.length, `nothing rendered at ${cols}x${rows} in ${env.FORCE_COLOR ?? 'no color'}`).toBeGreaterThan(0);
      const longest = text.split('\n').reduce((n, l) => Math.max(n, l.length), 0);
      expect(longest, `a line overflowed at ${cols}x${rows}`).toBeLessThanOrEqual(cols);
    }
  });

  it('says exactly the same thing at every depth', () => {
    // The property that matters: downgrading colour must never cost the user information.
    for (const [cols, rows] of SIZES) {
      const bodies = DEPTHS.map((d) => textOnly(rawFrame(cols, rows, d.env)).trim());
      const [first, ...rest] = bodies;
      for (const [i, body] of rest.entries()) {
        expect(
          body,
          `${cols}x${rows} rendered different text at ${DEPTHS[i + 1]!.name} than at ${DEPTHS[0]!.name}`,
        ).toBe(first);
      }
    }
  });

  it('keeps the state readable with colour entirely off', () => {
    const text = textOnly(rawFrame(80, 24, { NO_COLOR: '1', FORCE_COLOR: '0' }));
    // Nothing may depend on a hue: the failed tool, the retry marker, the posture and the meter
    // all have to survive being rendered with no colour at all.
    expect(text).toContain('exit 1');
    expect(text).toContain('retry 2');
    expect(text).toContain('manual');
    expect(text).toContain('ctx 12%');
    expect(text).toContain('esc interrupt');
  });

  it('gives every state a glyph and a label, so 16-colour and mono still read', () => {
    const a = glyphs('ascii');
    // The pairs that would otherwise be colour-only distinctions.
    const pairs: [string, string][] = [
      [a.done, a.failed],
      [a.queued, a.skipped],
    ];
    for (const [x, y] of pairs) {
      expect(x, 'two states share one ASCII glyph').not.toBe(y);
    }
    expect(new Set(a.sweep).size, 'the sweep has fewer than four distinct frames').toBe(4);
  });
});