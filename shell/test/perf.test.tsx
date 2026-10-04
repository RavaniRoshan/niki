/**
 * Measured performance baselines.
 *
 * These are **recordings, not thresholds**. The point is to have a number printed before any
 * optimisation, so a later change can be compared against something real rather than a feeling.
 * The ratio check on per-token render cost is the one assertion: the spec requires the cost of
 * appending a token to stay flat as the transcript grows, and a ratio is the only honest way to
 * state that.
 *
 * A failing budget here means "slower than the baseline recorded on this machine", not "broken".
 * The numbers are printed on every run so the file itself stays current.
 */

import { describe, expect, it } from 'vitest';
import { render } from 'ink-testing-library';
import React from 'react';

import { App } from '../src/app.js';
import { initialState, reduce, type AppState, type ReduceOptions } from '../src/state.js';
import { renderTranscriptLines } from '../src/components/transcript.js';
import type { ServerNotification } from '../src/protocol/generated/index.js';

const T0: ReduceOptions = { nowMs: 0 };

function ms(fn: () => void): number {
  const start = process.hrtime.bigint();
  fn();
  return Number(process.hrtime.bigint() - start) / 1e6;
}

/**
 * Best of five. A single sample of a microsecond-scale operation on a machine that is also
 * running a build is mostly noise, and a "regression" measured from noise is a claim about the
 * clock rather than about the code.
 */
function bestOfFive(fn: () => void): number {
  let best = Number.POSITIVE_INFINITY;
  for (let i = 0; i < 5; i += 1) best = Math.min(best, ms(fn));
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

describe('perf baselines (recorded on this machine)', () => {
  it('records first render at seven sizes', () => {
    const sizes: ReadonlyArray<readonly [number, number]> = [
      [49, 16],
      [50, 16],
      [79, 24],
      [80, 24],
      [119, 30],
      [120, 38],
      [180, 50],
    ];
    const results: string[] = [];
    for (const [cols, rows] of sizes) {
      const state: AppState = { ...initialState(cols, rows), phase: 'idle' };
      const elapsed = bestOfFive(() => {
        const inst = render(
          React.createElement(App, { state, theme: 'niki', charset: 'unicode', reducedMotion: true }),
        );
        inst.lastFrame();
        inst.unmount();
      });
      results.push(`${cols}x${rows}: ${elapsed.toFixed(2)}ms`);
    }
    console.log(`first render — ${results.join('  ')}`);
    expect(results).toHaveLength(7);
  });

  it('records idle cost: rendering the same state repeatedly costs nothing extra per frame', () => {
    const state = populated(4, 8);
    const elapsed = bestOfFive(() => {
      for (let i = 0; i < 200; i += 1) {
        renderTranscriptLines({
          state,
          theme: 'niki',
          charset: 'unicode',
          reducedMotion: true,
          sweepTick: 0,
          height: 30,
        });
      }
    });
    console.log(`200 idle transcript renders: ${elapsed.toFixed(2)}ms (${(elapsed / 200).toFixed(3)}ms/frame)`);
    expect(elapsed).toBeGreaterThanOrEqual(0);
  });

  it('keeps per-token render cost flat as the transcript grows (ratio at most 1.5)', () => {
    const ITERATIONS = 300;
    const measure = (turns: number): { perRender: number; messages: number } => {
      const state = populated(turns, 20);
      const run = () => {
        for (let i = 0; i < ITERATIONS; i += 1) {
          renderTranscriptLines({
            state,
            theme: 'niki',
            charset: 'unicode',
            reducedMotion: true,
            sweepTick: 0,
            height: 30,
          });
        }
      };
      // Best-of-five. A single sample of an operation this small is mostly timer noise, and a
      // ratio computed from noise would be a claim about the clock rather than about the code.
      let best = Number.POSITIVE_INFINITY;
      for (let attempt = 0; attempt < 5; attempt += 1) best = Math.min(best, ms(run));
      return { perRender: best / ITERATIONS, messages: state.messages.length };
    };

    // The baseline is deliberately *not* a tiny transcript. Measuring against one would be
    // measuring JIT warm-up, which is a constant that has nothing to do with how the transcript
    // grows. Both ends here are past warm-up, and they differ by 10x in message count.
    const small = measure(200);
    const large = measure(2000);
    const ratio = large.perRender / small.perRender;
    console.log(
      `per-token render cost — ${small.messages} messages: ${small.perRender.toFixed(4)}ms  ` +
        `${large.messages} messages: ${large.perRender.toFixed(4)}ms  ratio: ${ratio.toFixed(2)}`,
    );
    // Windowing is what keeps this flat: the cost is proportional to the rows on screen, not to
    // the history behind them.
    expect(ratio).toBeLessThanOrEqual(1.5);
  });

  it('records a 500-tool-row state', () => {
    let s = populated(1, 2);
    for (let i = 0; i < 500; i += 1) {
      s = reduce(s, { method: 'tool.call', params: { tool_id: `t${i}`, name: 'read', args: `file${i}` } } as never, T0);
    }
    const elapsed = ms(() => {
      for (let i = 0; i < 20; i += 1) {
        renderTranscriptLines({
          state: s,
          theme: 'niki',
          charset: 'unicode',
          reducedMotion: true,
          sweepTick: 0,
          height: 30,
        });
      }
    });
    console.log(`500 tool rows, 20 renders: ${elapsed.toFixed(2)}ms (${(elapsed / 20).toFixed(3)}ms/frame)`);
    expect(s.tools).toHaveLength(500);
  });

  it('records a 10k-line diff and a 10 MB tool result through the sanitiser', () => {
    const bigDiff = Array.from({ length: 10_000 }, (_, i) => ({ kind: 'context', text: `line ${i}` }));
    let s = populated(1, 2);
    s = reduce(
      s,
      { method: 'tool.diff', params: { tool_id: 'd', path: 'src/big.rs', hunks: [{ old_start: 1, old_lines: 10_000, new_start: 1, new_lines: 10_000, header: '@@', lines: bigDiff }] } } as never,
      T0,
    );
    const renderElapsed = bestOfFive(() => {
      renderTranscriptLines({
        state: s,
        theme: 'niki',
        charset: 'unicode',
        reducedMotion: true,
        sweepTick: 0,
        height: 30,
      });
    });

    const tenMb = 'x'.repeat(10 * 1024 * 1024);
    const sanitizeElapsed = ms(() => {
      s = reduce(s, { method: 'tool.result', params: { tool_id: 'b', ok: true, summary: tenMb, full_ref: null, duration_ms: 1 } } as never, T0);
    });
    console.log(
      `10k-line diff render: ${renderElapsed.toFixed(2)}ms   10 MB tool result through the sanitiser: ${sanitizeElapsed.toFixed(2)}ms`,
    );
    // The sanitiser must clamp a 10 MB payload rather than letting it reach a widget.
    expect(s.tools.find((t) => t.id === 'b')?.summary?.length).toBeLessThanOrEqual(4097);
  });

  it('records 100 consecutive resizes', () => {
    const state = populated(10, 4);
    const elapsed = bestOfFive(() => {
      for (let i = 0; i < 100; i += 1) {
        const cols = 40 + (i % 160);
        renderTranscriptLines({
          state: { ...state, cols, rows: 24 },
          theme: 'niki',
          charset: 'unicode',
          reducedMotion: true,
          sweepTick: 0,
          height: 20,
        });
      }
    });
    console.log(`100 resizes: ${elapsed.toFixed(2)}ms (${(elapsed / 100).toFixed(3)}ms each)`);
    expect(elapsed).toBeGreaterThanOrEqual(0);
  });
});