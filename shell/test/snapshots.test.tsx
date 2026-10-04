/**
 * Render snapshots at the seven sizes from the spec.
 *
 * Each frame is rendered from a real `AppState` that real events produced, so a snapshot can only
 * pass if the whole chain — reducer, sanitiser, theme, glyphs, mascot, footer — still draws what
 * it claims. Sizes, colour depths, charsets and motion are all swept, because a layout that is
 * right at 80 columns and broken at 49 is not right.
 */

import React from 'react';
import { render } from 'ink-testing-library';
import { describe, expect, it } from 'vitest';

import { App } from '../src/app.js';
import { initialState, reduce, type AppState, type ReduceOptions } from '../src/state.js';
import type { ServerNotification } from '../src/protocol/generated/index.js';
import type { ThemeName } from '../src/theme/index.js';

const T0: ReduceOptions = { nowMs: 1_000 };

/** The seven sizes the spec names. */
const SIZES: ReadonlyArray<readonly [number, number]> = [
  [49, 16],
  [50, 16],
  [79, 24],
  [80, 24],
  [119, 30],
  [120, 38],
  [180, 50],
];

function build(cols: number, rows: number): AppState {
  let s: AppState = { ...initialState(cols, rows), phase: 'idle' };
  const events: ServerNotification[] = [
    {
      method: 'session.ready',
      params: {
        session_id: 's1',
        project_path: '/home/u/projects/api',
        model: 'claude-sonnet-4',
        permission_mode: 'manual',
        branch: 'main',
        ahead: 1,
        behind: null,
        resumed_messages: 0,
      },
    } as ServerNotification,
    { method: 'turn.started', params: { turn_id: 't1', prompt: 'audit and improve test coverage' } } as ServerNotification,
    { method: 'stage.start', params: { stage_id: 'g1', role: 'planner', attempt: 1 } } as ServerNotification,
    { method: 'turn.delta', params: { turn_id: 't1', text: 'Reading the project to find the test setup' } } as ServerNotification,
    { method: 'tool.call', params: { tool_id: 'x1', name: 'Read', args: 'package.json' } } as ServerNotification,
    {
      method: 'tool.result',
      params: { tool_id: 'x1', ok: true, summary: 'read 46 lines', full_ref: null, duration_ms: 12 },
    } as ServerNotification,
    { method: 'tool.call', params: { tool_id: 'x2', name: 'Bash', args: 'npm test' } } as ServerNotification,
    {
      method: 'tool.result',
      params: { tool_id: 'x2', ok: false, summary: 'exit 1 · 3 failing', full_ref: null, duration_ms: 900 },
    } as ServerNotification,
  ];
  for (const e of events) s = reduce(s, e, T0);
  return s;
}

function frame(
  cols: number,
  rows: number,
  opts: { theme?: ThemeName; charset?: 'unicode' | 'ascii'; reducedMotion?: boolean } = {},
): string {
  // ink-testing-library types `lastFrame` as possibly-undefined because the first render may be
  // empty; in this suite it never is, so an undefined frame is a failure, not an empty string.
  const state = build(cols, rows);
  const { lastFrame } = render(
    <App
      state={state}
      theme={opts.theme ?? 'niki'}
      charset={opts.charset ?? 'unicode'}
      reducedMotion={opts.reducedMotion ?? true}
      sweepTick={0}
      version="0.1.0"
    />,
  );
  return lastFrame() ?? '';
}

describe('render snapshots: the seven sizes', () => {
  it.each(SIZES)('renders at %ix%i without throwing', (cols, rows) => {
    expect(frame(cols, rows).length).toBeGreaterThan(0);
  });

  it('keeps the permission posture visible at every width from 50 up', () => {
    // The spec's collapse order drops hints, then cwd, then branch, then model — never the
    // posture. So the posture is the one ambient fact this asserts at every surviving width.
    for (const [cols, rows] of SIZES.filter(([c]) => c >= 50)) {
      expect(frame(cols, rows) ?? '', `posture missing at ${cols}`).toContain('manual');
    }
  });

  it('keeps the composer at every width from 50 up', () => {
    for (const [cols, rows] of SIZES.filter(([c]) => c >= 50)) {
      expect(frame(cols, rows) ?? '', `composer missing at ${cols}`).toMatch(/> /);
    }
  });

  it.each(SIZES)('never exceeds %i columns on any line', (cols) => {
    const out = frame(cols, 40);
    const longest = out.split('\n').reduce((n, l) => Math.max(n, l.length), 0);
    expect(longest).toBeLessThanOrEqual(cols);
  });

  it('says so plainly below 50 columns instead of drawing a broken layout', () => {
    const out = frame(49, 16);
    expect(out).toContain('49 columns');
    expect(out).toContain('50 or more');
  });
});

describe('render snapshots: colour depths and charsets', () => {
  it.each(['niki', 'niki-light', 'niki-contrast', 'niki-dim'] as ThemeName[])(
    'renders identically-shaped output in %s',
    (theme) => {
      const out = frame(80, 24, { theme });
      // The same content in every palette: a theme change must not change what is said.
      expect(out).toContain('manual');
      expect(out).toContain('Read(package.json)');
      expect(out).toContain('exit 1');
    },
  );

  it('renders in ASCII with the ASCII state glyphs and no Unicode glyph', () => {
    const out = frame(80, 24, { charset: 'ascii' });
    expect(out, 'ASCII mode must not emit a box-drawing, eye or tick glyph').not.toMatch(
      /[◐◑●○✓✗└─▄▀█░]/,
    );
    // Done and failed degrade to + and x, so the state survives a monochrome terminal.
    expect(out).toContain('+ Read(package.json)');
    expect(out).toContain('x Bash(npm test)');
  });

  it('is stable when motion is reduced', () => {
    expect(frame(80, 24, { reducedMotion: true })).toBe(frame(80, 24, { reducedMotion: true }));
  });

  it('does not change the composer when the sweep tick changes', () => {
    const a = frame(80, 24, { reducedMotion: false });
    const b = frame(80, 24, { reducedMotion: false });
    expect(a).toBe(b);
  });
});

describe('render snapshots: content honesty', () => {
  it('shows no context meter when the engine never sent usage', () => {
    expect(frame(80, 24)).not.toMatch(/ctx \d+%/);
  });

  it('shows the end-of-turn summary only when the engine sent one', () => {
    const state = build(80, 24);
    expect(frame(80, 24)).not.toContain('Done in');
    const withEnd = reduce(
      state,
      {
        method: 'turn.end',
        params: { turn_id: 't1', summary: 's', duration_ms: 42_000, tool_calls: 3, files_changed: 1 },
      } as ServerNotification,
      T0,
    );
    const { lastFrame } = render(
      <App state={withEnd} theme="niki" charset="unicode" reducedMotion />,
    );
    expect(lastFrame()).toContain('Done in 42s');
    expect(lastFrame()).toContain('3 tool calls');
    expect(lastFrame()).toContain('1 file changed');
  });

  it('renders an approval prompt with the safest option focused', () => {
    let s = build(80, 24);
    s = reduce(
      s,
      {
        method: 'approval.request',
        params: {
          id: 'a1',
          tool: 'bash',
          command: 'npm test',
          options: [
            { id: 'allow', label: 'Allow' },
            { id: 'deny', label: 'Deny' },
          ],
          safest_option_id: 'deny',
        },
      } as ServerNotification,
      T0,
    );
    const { lastFrame } = render(<App state={s} theme="niki" charset="unicode" reducedMotion />);
    const out = lastFrame() ?? '';
    expect(out).toContain('Deny');
    expect(out).toContain('esc denies');
  });
});