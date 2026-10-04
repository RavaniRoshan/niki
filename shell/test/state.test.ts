/**
 * The reducer's contract.
 *
 * The claim under test is the one the whole shell rests on: **a row exists only because an event
 * created it**, and every number shown came from the engine. These tests drive the reducer with
 * declared events and assert on what appears and — just as importantly — on what does not.
 */

import { describe, expect, it } from 'vitest';
import {
  initialState,
  reduce,
  reduceLocal,
  type AppState,
  type ReduceOptions,
} from '../src/state.js';
import type { ServerNotification } from '../src/protocol/generated/index.js';

const T0: ReduceOptions = { nowMs: 1_000 };

function run(events: ServerNotification[], from: AppState = initialState(80, 24)): AppState {
  return events.reduce((acc, e) => reduce(acc, e, T0), from);
}

const sessionReady: ServerNotification = {
  method: 'session.ready',
  params: {
    session_id: 's1',
    project_path: '/home/u/proj',
    model: 'sonnet',
    permission_mode: 'manual',
    branch: 'main',
    ahead: 1,
    behind: null,
    resumed_messages: 0,
  },
} as ServerNotification;

describe('nothing is invented', () => {
  it('starts with no rows at all', () => {
    const s = initialState();
    expect(s.tools).toEqual([]);
    expect(s.stages).toEqual([]);
    expect(s.messages).toEqual([]);
    expect(s.branch).toEqual(null);
    expect(s.context).toEqual(null);
    expect(s.costUsd).toEqual(null);
    expect(s.verdict).toEqual(null);
  });

  it('shows no context meter until the engine reports usage', () => {
    const s = run([sessionReady]);
    expect(s.context).toBeNull();
  });

  it('shows the meter only once the engine sends real numbers', () => {
    const s = run([sessionReady, { method: 'context.usage', params: { used: 31_000, limit: 262_000 } } as ServerNotification]);
    expect(s.context).toEqual({ used: 31_000, limit: 262_000 });
  });

  it('does not invent a branch when the engine sent none', () => {
    const s = run([
      { ...sessionReady, params: { ...(sessionReady.params as object), branch: null, ahead: null } } as ServerNotification,
    ]);
    expect(s.session?.branch).toBeNull();
  });

  it('adds no stage row for a tool call, and no tool row for a stage', () => {
    const s = run([
      sessionReady,
      { method: 'tool.call', params: { tool_id: 'x', name: 'read', args: 'a' } } as ServerNotification,
    ]);
    expect(s.stages).toEqual([]);
    expect(s.tools).toHaveLength(1);

    const s2 = run([
      sessionReady,
      { method: 'stage.start', params: { stage_id: 'g', role: 'planner', attempt: 1 } } as ServerNotification,
    ]);
    expect(s2.tools).toEqual([]);
    expect(s2.stages).toHaveLength(1);
  });
});

describe('tool rows', () => {
  it('tracks independent state per tool, so several can be in flight at once', () => {
    const s = run([
      sessionReady,
      { method: 'tool.call', params: { tool_id: 'a', name: 'read', args: 'one' } } as ServerNotification,
      { method: 'tool.call', params: { tool_id: 'b', name: 'grep', args: 'two' } } as ServerNotification,
      { method: 'tool.result', params: { tool_id: 'a', ok: true, summary: 'read 46 lines', full_ref: null, duration_ms: 5 } } as ServerNotification,
    ]);
    expect(s.tools.map((t) => [t.id, t.state])).toEqual([
      ['a', 'done'],
      ['b', 'running'],
    ]);
    expect(s.tools[0]?.summary).toBe('read 46 lines');
    expect(s.tools[1]?.summary).toBeUndefined();
  });

  it('records a failure without inventing an error message', () => {
    const s = run([
      sessionReady,
      { method: 'tool.call', params: { tool_id: 'a', name: 'bash', args: 'npm test' } } as ServerNotification,
      { method: 'tool.result', params: { tool_id: 'a', ok: false, summary: 'exit 1 · 3 failing', full_ref: null, duration_ms: 9 } } as ServerNotification,
    ]);
    expect(s.tools[0]?.state).toBe('failed');
    expect(s.tools[0]?.summary).toBe('exit 1 · 3 failing');
  });
});

describe('stage rows', () => {
  it('shows the retry marker only when the engine incremented the attempt', () => {
    const first = run([
      sessionReady,
      { method: 'stage.start', params: { stage_id: 'g', role: 'coder', attempt: 1 } } as ServerNotification,
    ]);
    expect(first.stages[0]?.attempt).toBe(1);

    const second = run([
      sessionReady,
      { method: 'stage.start', params: { stage_id: 'g', role: 'coder', attempt: 2 } } as ServerNotification,
    ]);
    expect(second.stages[0]?.attempt).toBe(2);
  });

  it('keeps the provenance the engine reported', () => {
    const s = run([
      sessionReady,
      { method: 'stage.start', params: { stage_id: 'g', role: 'reviewer', attempt: 1 } } as ServerNotification,
      {
        method: 'stage.done',
        params: {
          stage_id: 'g',
          role: 'reviewer',
          summary: 'looks fine',
          tokens_in: 10,
          tokens_out: 20,
          cost_usd: 0.02,
          latency_ms: 900,
          retry_count: 0,
          artifact_ref: 'a1',
          provenance: 'independent',
        },
      } as ServerNotification,
    ]);
    expect(s.stages[0]?.provenance).toBe('independent');
    expect(s.costUsd).toBe(0.02);
  });

  it('accumulates cost only from stage.done events', () => {
    const done = (cost: number, id: string): ServerNotification =>
      ({
        method: 'stage.done',
        params: {
          stage_id: id,
          role: 'coder',
          summary: 's',
          tokens_in: 1,
          tokens_out: 1,
          cost_usd: cost,
          latency_ms: 1,
          retry_count: 0,
          artifact_ref: null,
          provenance: 'self_verification',
        },
      }) as ServerNotification;
    const s = run([sessionReady, done(0.1, 'a'), done(0.2, 'b')]);
    expect(s.costUsd).toBeCloseTo(0.3, 10);
  });

  it('collects reasoning as collapsed provider summaries, never as a row of its own', () => {
    const s = run([
      sessionReady,
      { method: 'stage.start', params: { stage_id: 'g', role: 'planner', attempt: 1 } } as ServerNotification,
      { method: 'stage.token', params: { stage_id: 'g', role: 'planner', text: 'looking for the test setup' } } as ServerNotification,
      { method: 'stage.token', params: { stage_id: 'g', role: 'planner', text: 'found vitest' } } as ServerNotification,
    ]);
    expect(s.stages[0]?.reasoning).toEqual(['looking for the test setup', 'found vitest']);
    // Reasoning is an attribute of the stage, never a separate transcript row.
    expect(s.messages).toHaveLength(0);
  });
});

describe('approvals', () => {
  const approval: ServerNotification = {
    method: 'approval.request',
    params: {
      id: 'a1',
      tool: 'bash',
      command: 'npm test',
      options: [
        { id: 'allow', label: 'Allow' },
        { id: 'deny', label: 'Deny' },
      ],
      // The engine names the safest option; the shell must follow it.
      safest_option_id: 'deny',
    },
  } as ServerNotification;

  it('focuses the option the engine named, not the first one', () => {
    const s = run([sessionReady, approval]);
    expect(s.approval?.focusedOptionId).toBe('deny');
    expect(s.phase).toBe('awaitingApproval');
  });

  it('does not invent a recovery action on a stage failure', () => {
    const s = run([
      sessionReady,
      { method: 'stage.start', params: { stage_id: 'g', role: 'coder', attempt: 1 } } as ServerNotification,
      { method: 'stage.failed', params: { stage_id: 'g', role: 'coder', error: 'boom', severity: 'error', recovery: null } } as ServerNotification,
    ]);
    expect(s.stages[0]?.recovery).toBeUndefined();
  });
});

describe('the turn', () => {
  it('echoes the user turn from the engine prompt, not from local input', () => {
    const s = run([sessionReady, { method: 'turn.started', params: { turn_id: 't', prompt: 'audit the tests' } } as ServerNotification]);
    expect(s.messages).toHaveLength(1);
    expect(s.messages[0]).toMatchObject({ kind: 'user', text: 'audit the tests' });
  });

  it('accumulates streaming text into one block and closes it on turn.end', () => {
    const s = run([
      sessionReady,
      { method: 'turn.started', params: { turn_id: 't', prompt: 'go' } } as ServerNotification,
      { method: 'turn.delta', params: { turn_id: 't', text: 'Hello ' } } as ServerNotification,
      { method: 'turn.delta', params: { turn_id: 't', text: 'world' } } as ServerNotification,
    ]);
    expect(s.messages).toHaveLength(2);
    expect(s.messages[1]).toMatchObject({ kind: 'assistant', text: 'Hello world', streaming: true });

    const done = run([
      sessionReady,
      { method: 'turn.started', params: { turn_id: 't', prompt: 'go' } } as ServerNotification,
      { method: 'turn.delta', params: { turn_id: 't', text: 'Hello' } } as ServerNotification,
      {
        method: 'turn.end',
        params: { turn_id: 't', summary: 's', duration_ms: 42_000, tool_calls: 3, files_changed: 1 },
      } as ServerNotification,
    ]);
    expect(done.messages[1]?.streaming).toBe(false);
    expect(done.turnEnd).toMatchObject({ duration_ms: 42_000, tool_calls: 3, files_changed: 1 });
  });

  it('keeps partial output after an interrupt', () => {
    const s = reduceLocal(
      run([
        sessionReady,
        { method: 'turn.started', params: { turn_id: 't', prompt: 'go' } } as ServerNotification,
        { method: 'turn.delta', params: { turn_id: 't', text: 'partial' } } as ServerNotification,
      ]),
      { kind: 'turn.interrupt' },
      T0,
    );
    expect(s.phase).toBe('interrupted');
    expect(s.messages[1]?.text).toBe('partial');
  });

  it('queues a message typed while a run is active', () => {
    const running = run([
      sessionReady,
      { method: 'turn.started', params: { turn_id: 't', prompt: 'go' } } as ServerNotification,
    ]);
    const queued = reduceLocal(running, { kind: 'turn.submit', prompt: 'and then this' }, T0);
    expect(queued.queue).toEqual(['and then this']);

    const idle = reduceLocal(run([sessionReady]), { kind: 'turn.submit', prompt: 'and then this' }, T0);
    expect(idle.queue, 'with no run active there is nothing to queue behind').toEqual([]);
  });
});

describe('sanitisation on the way in', () => {
  it('strips a control sequence out of engine text before it reaches a widget', () => {
    const s = run([
      sessionReady,
      { method: 'turn.started', params: { turn_id: 't', prompt: 'go' } } as ServerNotification,
      { method: 'turn.delta', params: { turn_id: 't', text: ']0;PWNEDhello' } } as ServerNotification,
    ]);
    expect(s.messages[1]?.text).not.toContain('');
    expect(s.messages[1]?.text).not.toContain(']0;PWNED');
    expect(s.messages[1]?.text).toContain('hello');
  });

  it('strips a control sequence out of a tool name and a notice', () => {
    const s = run([
      sessionReady,
      { method: 'tool.call', params: { tool_id: 'x', name: ']0;tread', args: 'a' } } as ServerNotification,
      { method: 'notice', params: { text: ']0;tcareful', level: 'warning' } } as ServerNotification,
    ]);
    expect(s.tools[0]?.name).toBe('read');
    expect(s.notices[0]?.text).toBe('careful');
  });
});

describe('the reducer is pure', () => {
  it('produces the same state for the same input', () => {
    const events = [
      sessionReady,
      { method: 'turn.started', params: { turn_id: 't', prompt: 'go' } } as ServerNotification,
      { method: 'tool.call', params: { tool_id: 'x', name: 'read', args: 'a' } } as ServerNotification,
    ];
    expect(run(events)).toEqual(run(events));
  });

  it('does not mutate the state it was given', () => {
    const before = initialState();
    const snapshot = JSON.stringify(before);
    run([sessionReady], before);
    expect(JSON.stringify(before)).toBe(snapshot);
  });
});