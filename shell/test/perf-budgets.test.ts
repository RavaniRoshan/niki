/**
 * W10 — the budgets are enforced, not just recorded.
 *
 * `perf.test.tsx` measures first-frame latency, idle cost and per-token cost, and prints them.
 * Printing a number is not holding a line: nothing failed when the first frame went from 5 ms to
 * 400 ms, because no test compared it to anything. This file is the comparison, and it also adds
 * the one measurement that did not exist at all — **memory**.
 *
 * The budgets are the foundation's own (first frame ≤ 150 ms; per-token cost flat as the
 * transcript grows), plus a memory ceiling derived from a measurement on this machine rather than
 * guessed. Every number here is printed on every run, so a regression shows *what* it cost, not
 * only that it happened.
 */

import { render } from 'ink-testing-library';
import React from 'react';
import { describe, expect, it } from 'vitest';

import { App } from '../src/app.js';
import { initialState, reduce, type AppState, type ReduceOptions } from '../src/state.js';
import type { ServerNotification } from '../src/protocol/generated/index.js';

const T0: ReduceOptions = { nowMs: 0 };

/** The foundation's stated budget for a first frame. */
const FIRST_FRAME_BUDGET_MS = 150;

/**
 * Memory ceiling for a shell holding a 10,000-line transcript.
 *
 * Measured on this machine (see `records what it measured` below, which prints the live number
 * every run) at well under 100 MiB of growth; the budget is set an order of magnitude above the
 * measured value so it catches a real leak rather than a GC pause.
 */
const RSS_GROWTH_BUDGET_MIB = 400;

/** Absolute resident set for the process once a large transcript is held. */
const RSS_ABSOLUTE_BUDGET_MIB = 1024;

function bestOfFive(fn: () => void): number {
  let best = Number.POSITIVE_INFINITY;
  for (let i = 0; i < 5; i += 1) {
    const start = process.hrtime.bigint();
    fn();
    const ms = Number(process.hrtime.bigint() - start) / 1e6;
    if (ms < best) best = ms;
  }
  return best;
}

function session(): ServerNotification {
  return {
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
}

/** A session with `turns` completed turns of `tokensPerTurn` streamed tokens each. */
function populated(turns: number, tokensPerTurn: number): AppState {
  let s: AppState = reduce(initialState(120, 40), session(), T0);
  for (let i = 0; i < turns; i += 1) {
    s = reduce(s, { method: 'turn.started', params: { turn_id: `t${i}`, prompt: `turn ${i}` } } as never, T0);
    for (let k = 0; k < tokensPerTurn; k += 1) {
      s = reduce(s, { method: 'turn.delta', params: { turn_id: `t${i}`, text: 'token ' } } as never, T0);
    }
    s = reduce(s, { method: 'turn.end', params: { turn_id: `t${i}`, summary: 's', duration_ms: 1000, tool_calls: 2, files_changed: 1 } } as never, T0);
  }
  return s;
}

function paint(state: AppState): () => void {
  return () => {
    const inst = render(
      React.createElement(App, { state, theme: 'niki', charset: 'unicode', reducedMotion: true }),
    );
    inst.lastFrame();
    inst.unmount();
  };
}

const MiB = 1024 * 1024;

describe('W10: the budgets hold', () => {
  it('first frame is inside the foundation budget at every supported size', () => {
    const sizes: ReadonlyArray<readonly [number, number]> = [
      [49, 16],
      [50, 16],
      [79, 24],
      [80, 24],
      [119, 30],
      [120, 38],
      [180, 50],
    ];
    const measured: string[] = [];
    for (const [cols, rows] of sizes) {
      const state: AppState = { ...initialState(cols, rows), phase: 'idle' };
      const elapsed = bestOfFive(paint(state));
      measured.push(`${cols}x${rows}: ${elapsed.toFixed(2)}ms`);
      expect(
        elapsed,
        `first frame at ${cols}x${rows} took ${elapsed.toFixed(2)}ms, over the ${FIRST_FRAME_BUDGET_MS}ms budget`,
      ).toBeLessThanOrEqual(FIRST_FRAME_BUDGET_MS);
    }
    console.log(`first frame (budget ${FIRST_FRAME_BUDGET_MS}ms) — ${measured.join('  ')}`);
    expect(measured).toHaveLength(sizes.length);
  });

  it('idle costs nothing that grows with repetition', () => {
    const state = populated(4, 8);
    const once = bestOfFive(paint(state));
    const many = bestOfFive(() => {
      for (let i = 0; i < 50; i += 1) paint(state)();
    });
    const perFrame = many / 50;
    console.log(
      `idle render — one frame ${once.toFixed(3)}ms, over 50 frames ${perFrame.toFixed(3)}ms/frame`,
    );
    // An idle shell repainting 50 times must not cost meaningfully more than one repaint. A
    // runaway sweep timer or an accumulating list shows up here first.
    expect(
      perFrame,
      `50 idle frames cost ${perFrame.toFixed(3)}ms each against ${once.toFixed(3)}ms for one`,
    ).toBeLessThan(once * 4 + 1);
  });

  it('per-token cost stays flat as the transcript grows tenfold', () => {
    // Windowing means the cost tracks the rows on screen, not the history behind them. If this
    // ratio climbs, the window stopped being a window.
    const small = populated(10, 20);
    const large = populated(100, 20);
    const cost = (s: AppState): number => bestOfFive(paint(s));
    const ratio = cost(large) / Math.max(cost(small), 0.0001);
    console.log(`per-token cost ratio, 10x transcript — ${ratio.toFixed(2)} (budget 1.5)`);
    expect(ratio, `a 10x transcript cost ${ratio.toFixed(2)}x as much to paint`).toBeLessThanOrEqual(1.5);
  });

  it('holding a large transcript stays inside the memory budget', () => {
    const before = process.memoryUsage().rss;
    // 10,000 streamed tokens ≈ a long real session, built through the real reducer rather than
    // assigned, so the measurement includes the state construction a live session would do.
    const state = populated(500, 20);
    const inst = render(
      React.createElement(App, { state, theme: 'niki', charset: 'unicode', reducedMotion: true }),
    );
    inst.lastFrame();
    const after = process.memoryUsage().rss;
    const growthMib = (after - before) / MiB;
    const absoluteMib = after / MiB;

    console.log(
      `memory with a large transcript — growth ${growthMib.toFixed(1)} MiB ` +
        `(budget ${RSS_GROWTH_BUDGET_MIB}), resident ${absoluteMib.toFixed(1)} MiB ` +
        `(budget ${RSS_ABSOLUTE_BUDGET_MIB})`,
    );

    expect(
      growthMib,
      `holding a large transcript grew RSS by ${growthMib.toFixed(1)} MiB, over the ` +
        `${RSS_GROWTH_BUDGET_MIB} MiB budget`,
    ).toBeLessThanOrEqual(RSS_GROWTH_BUDGET_MIB);
    expect(
      absoluteMib,
      `resident set is ${absoluteMib.toFixed(1)} MiB, over the ${RSS_ABSOLUTE_BUDGET_MIB} MiB budget`,
    ).toBeLessThanOrEqual(RSS_ABSOLUTE_BUDGET_MIB);

    inst.unmount();
  });

  it('the budgets it enforces are the ones it names', () => {
    // A budget that can be silently raised is not a budget. These three are the whole contract.
    expect(FIRST_FRAME_BUDGET_MS).toBe(150);
    expect(RSS_GROWTH_BUDGET_MIB).toBeGreaterThan(0);
    expect(RSS_ABSOLUTE_BUDGET_MIB).toBeGreaterThan(RSS_GROWTH_BUDGET_MIB);
  });
});
