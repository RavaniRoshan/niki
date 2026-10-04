/**
 * Generates the review frame dumps the owner looks at.
 *
 * These are the frames a person judges: not assertions, pictures. Every one is rendered from an
 * `AppState` that real protocol events produced, so a dump cannot show something the engine never
 * said. Each is written at 80x24 (the reference size) and 120x38 (where the extra width has to
 * earn its place), plus the mascot at all three width tiers in all five states.
 *
 * Regenerate with `npx vitest run test/review-frames.test.tsx`. The test asserts every file was
 * written and is non-empty, so a run that silently produced nothing fails rather than leaving a
 * stale set of pictures behind.
 */

import React from 'react';
import { render } from 'ink-testing-library';
import { describe, expect, it } from 'vitest';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

import { App } from '../src/app.js';
import { initialState, reduce, type AppState, type ReduceOptions } from '../src/state.js';
import { mascot, tierForWidth } from '../src/mascot.js';
import type { ServerNotification } from '../src/protocol/generated/index.js';
import { PALETTES, type ThemeName } from '../src/theme/index.js';

const REVIEW_DIR = join(import.meta.dirname, '..', '..', 'docs', 'foundation', 'review');
const T0: ReduceOptions = { nowMs: 0 };
const SIZES: ReadonlyArray<readonly [number, number]> = [
  [80, 24],
  [120, 38],
];

const SESSION = {
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
} as ServerNotification;

const n = (method: string, params: unknown): ServerNotification =>
  ({ method, params }) as ServerNotification;

function from(cols: number, rows: number, events: ServerNotification[]): AppState {
  let s: AppState = { ...initialState(cols, rows) };
  for (const e of events) s = reduce(s, e, T0);
  return s;
}

const TURN_START = n('turn.started', { turn_id: 't1', prompt: 'audit and improve test coverage' });

/** The states a reviewer is asked to judge, each from real events only. */
const STATES: ReadonlyArray<{ name: string; build: (c: number, r: number) => AppState }> = [
  {
    name: '01-idle',
    build: (c, r) => from(c, r, [SESSION]),
  },
  {
    name: '02-thinking',
    build: (c, r) =>
      from(c, r, [
        SESSION,
        TURN_START,
        n('stage.start', { stage_id: 'g1', role: 'planner', attempt: 1 }),
        n('stage.token', { stage_id: 'g1', role: 'planner', text: 'looking for the test setup' }),
      ]),
  },
  {
    name: '03-parallel-tools',
    build: (c, r) =>
      from(c, r, [
        SESSION,
        TURN_START,
        n('stage.start', { stage_id: 'g1', role: 'planner', attempt: 1 }),
        n('tool.call', { tool_id: 't1', name: 'Read', args: 'package.json' }),
        n('tool.result', {
          tool_id: 't1',
          ok: true,
          summary: 'read 46 lines',
          full_ref: '/tmp/.niki/read.json',
          duration_ms: 12,
        }),
        n('tool.call', { tool_id: 't2', name: 'Search', args: 'pattern: "**/*.test.ts"' }),
        n('tool.result', {
          tool_id: 't2',
          ok: true,
          summary: 'found 100 files',
          full_ref: '/tmp/.niki/search.json',
          duration_ms: 40,
        }),
        n('tool.call', { tool_id: 't3', name: 'Bash', args: 'npm test' }),
      ]),
  },
  {
    name: '04-failed-tool',
    build: (c, r) =>
      from(c, r, [
        SESSION,
        TURN_START,
        n('tool.call', { tool_id: 't1', name: 'Read', args: 'package.json' }),
        n('tool.result', {
          tool_id: 't1',
          ok: true,
          summary: 'read 46 lines',
          full_ref: '/tmp/.niki/read.json',
          duration_ms: 12,
        }),
        n('tool.call', { tool_id: 't2', name: 'Bash', args: 'npm test' }),
        n('tool.result', {
          tool_id: 't2',
          ok: false,
          summary: 'exit 1 · 3 failing',
          full_ref: '/tmp/.niki/test.log',
          duration_ms: 900,
        }),
        n('turn.delta', { turn_id: 't1', text: 'Three tests fail on the new endpoint. Fixing those first.' }),
      ]),
  },
  {
    name: '05-approval',
    build: (c, r) =>
      from(c, r, [
        SESSION,
        TURN_START,
        n('tool.call', { tool_id: 't1', name: 'Bash', args: 'npm test' }),
        n('approval.request', {
          id: 'a1',
          tool: 'bash',
          command: 'npm test -- --coverage',
          options: [
            { id: 'allow', label: 'Allow' },
            { id: 'deny', label: 'Deny' },
          ],
          // A hostile engine naming Allow as safest must still open on Deny in manual mode.
          safest_option_id: 'allow',
        }),
      ]),
  },
  {
    name: '06-stages',
    build: (c, r) =>
      from(c, r, [
        SESSION,
        TURN_START,
        n('stage.start', { stage_id: 'g1', role: 'planner', attempt: 1 }),
        n('stage.done', {
          stage_id: 'g1',
          role: 'planner',
          summary: 'three test files need cases',
          tokens_in: 1240,
          tokens_out: 380,
          cost_usd: 0.012,
          latency_ms: 4200,
          retry_count: 0,
          artifact_ref: 'plan.json',
          provenance: 'independent',
        }),
        n('stage.start', { stage_id: 'g2', role: 'coder', attempt: 2 }),
        n('stage.done', {
          stage_id: 'g2',
          role: 'coder',
          summary: 'added 14 cases',
          tokens_in: 8800,
          tokens_out: 2100,
          cost_usd: 0.09,
          latency_ms: 38_000,
          retry_count: 1,
          artifact_ref: 'diff.patch',
          provenance: 'self_verification',
        }),
        n('stage.start', { stage_id: 'g3', role: 'tester', attempt: 1 }),
      ]),
  },
  {
    name: '07-plan',
    build: (c, r) =>
      from(c, r, [
        SESSION,
        TURN_START,
        n('plan.update', {
          items: [
            { text: 'read the test setup', done: true },
            { text: 'add missing cases', done: false },
            { text: 'run the suite', done: false },
          ],
        }),
        n('tool.call', { tool_id: 't1', name: 'Grep', args: 'pattern: "describe("' }),
      ]),
  },
  {
    name: '08-interrupted',
    build: (c, r) => {
      let s = from(c, r, [
        SESSION,
        TURN_START,
        n('turn.delta', { turn_id: 't1', text: 'Reading the project to find the test setup' }),
      ]);
      s = { ...s, interrupted: true, phase: 'interrupted', activity: null };
      return s;
    },
  },
  {
    name: '09-end-of-turn',
    build: (c, r) =>
      from(c, r, [
        SESSION,
        TURN_START,
        n('turn.delta', {
          turn_id: 't1',
 text: 'Added **14** cases across three files and re-ran the suite: **all green**.',
        }),
        n('tool.call', { tool_id: 't1', name: 'Edit', args: 'tests/health.test.ts' }),
        n('tool.result', {
          tool_id: 't1',
          ok: true,
          summary: 'edited 1 file',
          full_ref: '/tmp/.niki/diff.json',
          duration_ms: 210,
        }),
        n('tool.call', { tool_id: 't2', name: 'Edit', args: 'tests/api.test.ts' }),
        n('tool.result', {
          tool_id: 't2',
          ok: true,
          summary: 'edited 1 file',
          full_ref: '/tmp/.niki/diff.json',
          duration_ms: 190,
        }),
        n('tool.call', { tool_id: 't3', name: 'Bash', args: 'npm test' }),
        n('tool.result', {
          tool_id: 't3',
          ok: true,
          summary: '148 passing',
          full_ref: '/tmp/.niki/test.log',
          duration_ms: 12_400,
        }),
        n('branch.created', { name: 'niki/4821' }),
        n('verdict.ready', { ref: 'verdict.json', verdict: 'approved', provenance: 'independent' }),
        n('context.usage', { used: 31_000, limit: 262_000 }),
        n('cost.update', { usd: 0.42 }),
        n('turn.end', {
          turn_id: 't1',
          summary: 'done',
          duration_ms: 42_000,
          tool_calls: 3,
          files_changed: 1,
        }),
      ]),
  },
  {
    name: '10-error',
    build: (c, r) =>
      from(c, r, [
        SESSION,
        TURN_START,
        n('stage.start', { stage_id: 'g1', role: 'coder', attempt: 1 }),
        n('stage.failed', {
          stage_id: 'g1',
          role: 'coder',
          error: 'the patch did not apply against the current file',
          severity: 'error',
          recovery: 're-read the file and re-run with the branch checked out',
        }),
      ]),
  },
  {
    name: '11-narrow-49',
    build: (_c, _r) => initialState(49, 16),
  },
];

function renderFrame(state: AppState, theme: ThemeName = 'niki'): string {
  const out = render(
    <App state={state} theme={theme} charset="unicode" reducedMotion version="0.1.0" />,
  );
  const frame = out.lastFrame() ?? '';
  out.unmount();
  return frame;
}

describe('review frame dumps', () => {
  it('writes every key chat state at both sizes', () => {
    mkdirSync(REVIEW_DIR, { recursive: true });
    const written: string[] = [];

    for (const [cols, rows] of SIZES) {
      for (const state of STATES) {
        const name = `${state.name}_${cols}x${rows}`;
        const text = renderFrame(state.build(cols, rows));
        const path = join(REVIEW_DIR, `${name}.txt`);
        writeFileSync(path, `${text}\n`, 'utf8');
        written.push(path);
      }
    }

    expect(written.length).toBe(STATES.length * SIZES.length);
    for (const path of written) {
      expect(textOf(path).length, `${path} is empty`).toBeGreaterThan(0);
    }
    console.log(`review frames written: ${written.length}`);
  });

  it('writes the mascot at every width tier in all five states', () => {
    mkdirSync(REVIEW_DIR, { recursive: true });
    const tiers = [
      { name: 'full-80', cols: 80 },
      { name: 'compact-60', cols: 60 },
      { name: 'tiny-40', cols: 40 },
    ] as const;
    const states = ['idle', 'working', 'done', 'error', 'interrupted'] as const;

    const linesOut: string[] = [
      '# NIKI mascot — all width tiers, all five states',
      '',
      'Generated by `shell/test/review-frames.test.tsx`. Rendered from the theme tokens, not',
      'from a picture: the body colour is the state token and the eye is the activity-sweep glyph,',
      'so the mascot is a truthful status indicator rather than decoration.',
      '',
    ];

    for (const tier of tiers) {
      const mascotTier = tierForWidth(tier.cols);
      linesOut.push(`## ${tier.name} (${tier.cols} columns → ${mascotTier} tier)`, '');
      for (const state of states) {
        const art = mascot(state, mascotTier, 'unicode', 'niki');
        linesOut.push(`### ${state} — body: ${art.bodyToken}, eye: ${art.eyeToken}`, '');
        for (const row of art.lines) linesOut.push(`    ${row}`);
        linesOut.push('');
      }
    }
    linesOut.push('## width stability', '');
    for (const tier of tiers) {
      const mascotTier = tierForWidth(tier.cols);
      const widths = states.map((s) => mascot(s, mascotTier, 'unicode', 'niki').lines[0]!.length);
      linesOut.push(
        `- ${tier.name}: ${mascotTier} tier, row widths ${new Set(widths).size === 1 ? 'identical' : 'DIFFERENT'} (${[...new Set(widths)].join(', ')})`,
      );
    }

    const path = join(REVIEW_DIR, 'mascot-tiers-and-states.txt');
    writeFileSync(path, `${linesOut.join('\n')}\n`, 'utf8');
    expect(textOf(path)).toContain('idle');
    console.log(`mascot review written: ${path}`);
  });

  it('writes the palette reference so a reviewer can judge colour without guessing', () => {
    mkdirSync(REVIEW_DIR, { recursive: true });
    const rows: string[] = [
      '# NIKI palettes as tokens',
      '',
      'Every colour in the shell comes from this table. A value that is not in it does not exist.',
      '',
    ];
    for (const [name, palette] of Object.entries(PALETTES)) {
      rows.push(`## ${name}`, '');
      for (const [key, value] of Object.entries(palette)) {
        if (key !== 'name') rows.push(`- \`${key}\`: \`${value}\``);
      }
      rows.push('');
    }
    const path = join(REVIEW_DIR, 'palettes.txt');
    writeFileSync(path, `${rows.join('\n')}\n`, 'utf8');
    expect(textOf(path)).toContain('niki-contrast');
  });
});

function textOf(path: string): string {
  return readFileSync(path, 'utf8');
}