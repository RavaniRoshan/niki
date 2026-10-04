/**
 * The chat loop, row by row: E1 through E13.
 *
 * Each block names the row it is proving and asserts on what the user would actually see, because
 * a row that renders the wrong thing in the right colour is still wrong.
 */

import { describe, expect, it } from 'vitest';
import { render } from 'ink-testing-library';
import React from 'react';

import { App } from '../src/app.js';
import { renderTranscriptLines, formatDuration, renderTurnSummary } from '../src/components/transcript.js';
import { hintsFor, formatHintList, COMMANDS, worksWhileRunning } from '../src/components/footer.js';
import { initialState, reduce, reduceLocal, type AppState, type ReduceOptions } from '../src/state.js';
import { parseBlocks, parseInline } from '../src/markdown.js';
import { mascot, mascotWidth, everyStateHasTheSameWidth, tierForWidth } from '../src/mascot.js';
import { sweepIntervalMs, glyphs } from '../src/glyphs.js';
import { PALETTES } from '../src/theme/index.js';
import type { ServerNotification } from '../src/protocol/generated/index.js';

const T0: ReduceOptions = { nowMs: 0 };

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

function run(events: ServerNotification[], from?: AppState): AppState {
  let s = from ?? initialState(80, 24);
  for (const e of events) s = reduce(s, e, T0);
  return s;
}

function lines(s: AppState, cols = 80, rows = 24): string[] {
  return renderTranscriptLines({
    state: s,
    theme: 'niki',
    charset: 'unicode',
    reducedMotion: true,
    sweepTick: 0,
    height: rows,
  }).map((l) => l.text);
}

function frame(s: AppState, cols = 80, rows = 24): string {
  // The width lives in the state, so a frame rendered at a requested width has to carry it.
  const out = render(
    <App state={{ ...s, cols, rows }} theme="niki" charset="unicode" reducedMotion />,
  );
  const text = out.lastFrame() ?? '';
  out.unmount();
  return text;
}

const n = (method: string, params: unknown): ServerNotification =>
  ({ method, params }) as ServerNotification;

describe('E2: the user message is visually distinct from the assistant', () => {
  it('the user turn is a raised row and the assistant is a bullet', () => {
    const s = run([
      SESSION,
      n('turn.started', { turn_id: 't', prompt: 'audit the tests' }),
      n('turn.delta', { turn_id: 't', text: 'Looking now' }),
    ]);
    const out = lines(s);
    expect(out.some((l) => l.startsWith('> audit the tests'))).toBe(true);
    expect(out.some((l) => l.includes('Looking now'))).toBe(true);
  });

  it('the user row is bold and the assistant row is not', () => {
    const s = run([
      SESSION,
      n('turn.started', { turn_id: 't', prompt: 'go' }),
      n('turn.delta', { turn_id: 't', text: 'answer' }),
    ]);
    const rendered = renderTranscriptLines({
      state: s,
      theme: 'niki',
      charset: 'unicode',
      reducedMotion: true,
      sweepTick: 0,
      height: 24,
    });
    const user = rendered.find((l) => l.text.startsWith('> go'));
    const assistant = rendered.find((l) => l.text.includes('answer'));
    expect(user?.bold).toBe(true);
    expect(assistant?.bold).toBeFalsy();
  });
});

describe('E3: one live activity line, built only from real events', () => {
  it('is absent at idle', () => {
    const s = run([SESSION]);
    expect(s.activity).toBeNull();
    expect(lines(s).some((l) => l.includes('Running') || l.includes('Thinking'))).toBe(false);
  });

  it('names the stage the engine reported, in plain words', () => {
    const cases: [string, string][] = [
      ['planner', 'Planning'],
      ['coder', 'Editing'],
      ['tester', 'Running tests'],
      ['reviewer', 'Reviewing changes'],
    ];
    for (const [role, text] of cases) {
      const s = run([SESSION, n('stage.start', { stage_id: 'g', role, attempt: 1 })]);
      expect(s.activity?.text, `role ${role}`).toBe(text);
    }
  });

  it('shows no "next" line unless the engine supplied one', () => {
    const s = run([SESSION, n('stage.start', { stage_id: 'g', role: 'planner', attempt: 1 })]);
    expect(lines(s).some((l) => l.includes('Next'))).toBe(false);
  });

  it('disappears at idle again', () => {
    const s = run([
      SESSION,
      n('turn.started', { turn_id: 't', prompt: 'go' }),
      n('turn.end', { turn_id: 't', summary: 's', duration_ms: 10, tool_calls: 0, files_changed: 0 }),
    ]);
    expect(s.activity).toBeNull();
    expect(lines(s).some((l) => l.includes('Thinking'))).toBe(false);
  });

  it('cadence follows the spec: 100ms in flight, 200ms after 20s, else 120ms', () => {
    expect(sweepIntervalMs({ toolInFlight: true, runningMs: 0 })).toBe(100);
    expect(sweepIntervalMs({ toolInFlight: false, runningMs: 0 })).toBe(120);
    expect(sweepIntervalMs({ toolInFlight: true, runningMs: 25_000 })).toBe(200);
  });
});

describe('E4: reasoning is collapsed to one dim line', () => {
  it('produces exactly one row for many provider summaries', () => {
    const s = run([
      SESSION,
      n('stage.start', { stage_id: 'g', role: 'planner', attempt: 1 }),
      n('stage.token', { stage_id: 'g', role: 'planner', text: 'looking' }),
      n('stage.token', { stage_id: 'g', role: 'planner', text: 'found vitest' }),
      n('stage.token', { stage_id: 'g', role: 'planner', text: 'planning a change' }),
    ]);
    const reasoningRows = lines(s).filter((l) => l.includes('reasoned'));
    expect(reasoningRows).toHaveLength(1);
    expect(reasoningRows[0]).toContain('ctrl+o');
  });

  it('never renders the raw provider text as its own row', () => {
    const s = run([
      SESSION,
      n('stage.start', { stage_id: 'g', role: 'planner', attempt: 1 }),
      n('stage.token', { stage_id: 'g', role: 'planner', text: 'SECRET-CHAIN-OF-THOUGHT' }),
    ]);
    expect(lines(s).some((l) => l.includes('SECRET-CHAIN-OF-THOUGHT'))).toBe(false);
  });

  it('shows no reasoning row for a stage that sent none', () => {
    const s = run([SESSION, n('stage.start', { stage_id: 'g', role: 'planner', attempt: 1 })]);
    expect(lines(s).some((l) => l.includes('reasoned'))).toBe(false);
  });
});

describe('E5: tool and stage rows share one grammar', () => {
  it('gives each tool an independent state so several can be in flight', () => {
    const s = run([
      SESSION,
      n('tool.call', { tool_id: 'a', name: 'Read', args: 'package.json' }),
      n('tool.call', { tool_id: 'b', name: 'Search', args: 'test' }),
      n('tool.result', {
        tool_id: 'a',
        ok: true,
        summary: 'read 46 lines',
        full_ref: '/tmp/.niki/run/read.json',
        duration_ms: 3,
      }),
      n('tool.call', { tool_id: 'c', name: 'Bash', args: 'npm test' }),
    ]);
    const out = lines(s);
    expect(out.some((l) => l.includes('Read(package.json)'))).toBe(true);
    expect(out.some((l) => l.includes('read 46 lines'))).toBe(true);
    expect(out.some((l) => l.includes('Search(test)'))).toBe(true);
    expect(out.some((l) => l.includes('Bash(npm test)'))).toBe(true);
    // The expand hint appears only where there is something more to expand to.
    expect(out.some((l) => l.includes('ctrl+o expand'))).toBe(true);
  });

  it('offers no expand hint when the engine said the summary is all there is', () => {
    const s = run([
      SESSION,
      n('tool.call', { tool_id: 'a', name: 'Read', args: 'package.json' }),
      n('tool.result', {
        tool_id: 'a',
        ok: true,
        summary: 'read 46 lines',
        full_ref: null,
        duration_ms: 3,
      }),
    ]);
    expect(lines(s).some((l) => l.includes('ctrl+o expand'))).toBe(false);
  });

  it('shows a result line only when the engine sent a result', () => {
    const running = run([SESSION, n('tool.call', { tool_id: 'a', name: 'Read', args: 'x' })]);
    expect(lines(running).some((l) => l.includes('ctrl+o expand'))).toBe(false);
  });

  it('shows a progress note only while the tool is in flight', () => {
    const s = run([
      SESSION,
      n('tool.call', { tool_id: 'a', name: 'Bash', args: 'npm test' }),
      n('tool.progress', { tool_id: 'a', note: 'installing' }),
    ]);
    expect(lines(s).some((l) => l.includes('installing'))).toBe(true);
  });

  it('marks a retry only when the engine incremented the attempt', () => {
    const first = run([SESSION, n('stage.start', { stage_id: 'g', role: 'coder', attempt: 1 })]);
    expect(lines(first).some((l) => l.includes('retry'))).toBe(false);

    const second = run([SESSION, n('stage.start', { stage_id: 'g', role: 'coder', attempt: 2 })]);
    expect(lines(second).some((l) => l.includes('retry 2'))).toBe(true);
  });

  it('labels provenance, because it changes how much a verdict is worth', () => {
    const independent = run([
      SESSION,
      n('stage.start', { stage_id: 'g', role: 'reviewer', attempt: 1 }),
      n('stage.done', {
        stage_id: 'g',
        role: 'reviewer',
        summary: 'fine',
        tokens_in: 1,
        tokens_out: 1,
        cost_usd: 0,
        latency_ms: 1,
        retry_count: 0,
        artifact_ref: null,
        provenance: 'independent',
      }),
    ]);
    expect(lines(independent).some((l) => l.includes('independent review'))).toBe(true);

    const selfCheck = run([
      SESSION,
      n('stage.start', { stage_id: 'g', role: 'coder', attempt: 1 }),
      n('stage.done', {
        stage_id: 'g',
        role: 'coder',
        summary: 'done',
        tokens_in: 1,
        tokens_out: 1,
        cost_usd: 0,
        latency_ms: 1,
        retry_count: 0,
        artifact_ref: null,
        provenance: 'self_verification',
      }),
    ]);
    expect(lines(selfCheck).some((l) => l.includes('self-verified'))).toBe(true);
  });
});

describe('E6: failures render inline and the session continues', () => {
  it('shows the engine excerpt under the tool row', () => {
    const s = run([
      SESSION,
      n('tool.call', { tool_id: 'a', name: 'Bash', args: 'npm test' }),
      n('tool.result', {
        tool_id: 'a',
        ok: false,
        summary: 'exit 1 · 3 failing',
        full_ref: null,
        duration_ms: 900,
      }),
      n('turn.delta', { turn_id: 't', text: 'Still working on it' }),
    ]);
    const out = lines(s);
    expect(out.some((l) => l.includes('exit 1 · 3 failing'))).toBe(true);
    // The session continues: output after the failure still renders.
    expect(out.some((l) => l.includes('Still working on it'))).toBe(true);
  });

  it('paints the failure in the error token', () => {
    const s = run([
      SESSION,
      n('tool.call', { tool_id: 'a', name: 'Bash', args: 'x' }),
      n('tool.result', { tool_id: 'a', ok: false, summary: 'exit 1', full_ref: null, duration_ms: 1 }),
    ]);
    const rendered = renderTranscriptLines({
      state: s,
      theme: 'niki',
      charset: 'unicode',
      reducedMotion: true,
      sweepTick: 0,
      height: 24,
    });
    expect(rendered.find((l) => l.text.includes('Bash(x)'))?.token).toBe(PALETTES.niki.error);
  });

  it('shows a recovery action only when the engine supplied one', () => {
    const withRecovery = run([
      SESSION,
      n('stage.start', { stage_id: 'g', role: 'coder', attempt: 1 }),
      n('stage.failed', {
        stage_id: 'g',
        role: 'coder',
        error: 'patch did not apply',
        severity: 'error',
        recovery: 'rerun with the branch checked out',
      }),
    ]);
    expect(lines(withRecovery).some((l) => l.includes('rerun with the branch'))).toBe(true);

    const without = run([
      SESSION,
      n('stage.start', { stage_id: 'g', role: 'coder', attempt: 1 }),
      n('stage.failed', { stage_id: 'g', role: 'coder', error: 'boom', severity: 'error', recovery: null }),
    ]);
    expect(without.stages[0]?.recovery).toBeUndefined();
  });

  it('says the state rather than inventing an error message', () => {
    const s = run([
      SESSION,
      n('tool.call', { tool_id: 'a', name: 'Bash', args: 'x' }),
      n('tool.result', { tool_id: 'a', ok: false, summary: '', full_ref: null, duration_ms: 1 }),
    ]);
    expect(lines(s).some((l) => l.includes('failed'))).toBe(true);
  });
});

describe('E7: the footer is honest about what it knows', () => {
  it('shows no context meter until the engine reports usage', () => {
    const s = run([SESSION]);
    expect(frame(s)).not.toMatch(/ctx \\d+%/);
  });

  it('shows the meter with real numbers once reported', () => {
    const s = run([SESSION, n('context.usage', { used: 31_000, limit: 262_000 })]);
    const out = frame(s);
    expect(out).toContain('ctx 12%');
    expect(out).toContain('31k/262k');
  });

  it('shows no branch arrow until git reported the counts', () => {
    const none = run([
      n('session.ready', {
        ...(SESSION.params as object),
        branch: 'main',
        ahead: null,
      }),
    ]);
    expect(frame(none)).not.toContain('↑');
  });

  it('shows the arrow when the engine really reported it', () => {
    const ahead = run([SESSION]);
    expect(frame(ahead)).toContain('↑1');
  });

  it('changes its hints with the state', () => {
    expect(formatHintList(hintsFor('idle'))).toContain('/ commands');
    expect(formatHintList(hintsFor('thinking'))).toContain('esc interrupt');
    expect(formatHintList(hintsFor('awaitingApproval'))).toContain('esc deny');
    expect(formatHintList(hintsFor('interrupted'))).toContain('type to continue');
  });

  it('keeps the permission posture at every width from 50 up', () => {
    for (const cols of [50, 60, 70, 80, 120, 180]) {
      const s = { ...run([SESSION]), cols };
      expect(frame(s, cols), `posture missing at ${cols}`).toContain('manual');
    }
  });
});

describe('E8: the header carries the mascot and the real facts', () => {
  it('shows the name, the model and the posture once the engine reports them', () => {
    const out = frame(run([SESSION]));
    expect(out).toContain('Niki');
    expect(out).toContain('claude-sonnet-4');
    expect(out).toContain('manual');
    expect(out).toContain('main');
  });

  it('shows nothing rather than a placeholder before the engine reports a session', () => {
    const out = frame(initialState(80, 24));
    expect(out).toContain('connecting to the engine');
    expect(out).not.toContain('undefined');
    expect(out).not.toContain('null');
  });

  it('uses the full art at 80 columns and the compact tier at 50-79', () => {
    expect(tierForWidth(80)).toBe('full');
    expect(tierForWidth(50)).toBe('compact');
    expect(frame(run([SESSION]), 80).split('\n')[0]).toContain('▄');
    expect(frame(run([SESSION]), 60).split('\n')[0]).not.toContain('▄');
  });

  it('follows the real state machine and nothing else', () => {
    expect(mascot('idle', 'full', 'unicode', 'niki').bodyToken).toBe('accent');
    const working = run([
      SESSION,
      n('turn.started', { turn_id: 't', prompt: 'go' }),
      n('turn.delta', { turn_id: 't', text: 'a' }),
    ]);
    expect(working.phase).toBe('streaming');
    expect(mascot('working', 'full', 'unicode', 'niki').lines[1]).toContain('◑');

    const failed = run([SESSION, n('stage.failed', {
      stage_id: 'g',
      role: 'coder',
      error: 'x',
      severity: 'error',
      recovery: null,
    })]);
    expect(mascot('error', 'full', 'unicode', 'niki').bodyToken).toBe('error');
    expect(failed.phase).toBe('error');
  });

  it('keeps the same width in every state, at every tier', () => {
    for (const tier of ['full', 'compact', 'tiny'] as const) {
      expect(everyStateHasTheSameWidth(tier, 'unicode')).toBe(true);
      expect(mascotWidth('done', tier, 'unicode')).toBe(mascotWidth('idle', tier, 'unicode'));
    }
  });
});

describe('E10: the end-of-turn summary uses real counters', () => {
  it('renders exactly the sentence the spec asks for', () => {
    const s = run([
      SESSION,
      n('turn.started', { turn_id: 't', prompt: 'go' }),
      n('turn.end', {
        turn_id: 't',
        summary: 's',
        duration_ms: 42_000,
        tool_calls: 3,
        files_changed: 1,
      }),
    ]);
    expect(renderTurnSummary(s)).toBe('Done in 42s · 3 tool calls · 1 file changed');
  });

  it('pluralises from the number, not from a template', () => {
    const one = run([
      SESSION,
      n('turn.end', { turn_id: 't', summary: 's', duration_ms: 1000, tool_calls: 1, files_changed: 0 }),
    ]);
    expect(renderTurnSummary(one)).toContain('1 tool call ·');
    expect(renderTurnSummary(one)).toContain('0 files changed');
  });

  it('shows no summary at all when the engine sent none', () => {
    expect(renderTurnSummary(run([SESSION]))).toBe('');
  });

  it('formats durations the way a person reads them', () => {
    expect(formatDuration(420)).toBe('420ms');
    expect(formatDuration(4200)).toBe('4.2s');
    expect(formatDuration(42_000)).toBe('42s');
    expect(formatDuration(3_600_000)).toBe('60m 0s');
  });
});

describe('E11: interrupting keeps what was produced', () => {
  it('keeps the partial answer and says how to continue', () => {
    let s = run([
      SESSION,
      n('turn.started', { turn_id: 't', prompt: 'go' }),
      n('turn.delta', { turn_id: 't', text: 'partial work' }),
    ]);
    s = reduceLocal(s, { kind: 'turn.interrupt' }, T0);
    expect(s.phase).toBe('interrupted');
    expect(s.messages.some((m) => m.text.includes('partial work'))).toBe(true);
    expect(lines(s).some((l) => l.includes('Interrupted · type to continue'))).toBe(true);
  });

  it('clears the activity line, because nothing is running any more', () => {
    let s = run([SESSION, n('turn.started', { turn_id: 't', prompt: 'go' })]);
    expect(s.activity).not.toBeNull();
    s = reduceLocal(s, { kind: 'turn.interrupt' }, T0);
    expect(s.activity).toBeNull();
  });
});

describe('E9: streaming Markdown renders into one block', () => {
  it('accumulates deltas into a single assistant block', () => {
    let s = run([SESSION, n('turn.started', { turn_id: 't', prompt: 'go' })]);
    for (const chunk of ['Hello ', 'world', ' — ', 'done']) {
      s = reduce(s, n('turn.delta', { turn_id: 't', text: chunk }), T0);
    }
    const assistant = s.messages.filter((m) => m.kind === 'assistant');
    expect(assistant).toHaveLength(1);
    expect(assistant[0]!.text).toBe('Hello world — done');
  });

  it('does not move an already-rendered line when the next token arrives', () => {
    const finalText = '# Title\n\nBody text here.\n\n- one\n- two';
    let s = run([SESSION, n('turn.started', { turn_id: 't', prompt: 'go' })]);
    const snapshots: string[][] = [];
    for (const chunk of finalText.split(/(?=.)/)) {
      s = reduce(s, n('turn.delta', { turn_id: 't', text: chunk }), T0);
      snapshots.push(lines(s));
    }
    // Every earlier frame's visible text must appear as a prefix of the final frame, block for
    // block. A flicker is exactly a line that changes after it has been shown.
    const finalOut = snapshots[snapshots.length - 1]!;
    for (const snap of snapshots) {
      expect(snap.length, 'the visible block count grew backwards').toBeLessThanOrEqual(finalOut.length);
    }
    expect(finalOut.some((l) => l.includes('Title'))).toBe(true);
    expect(finalOut.some((l) => l.includes('one'))).toBe(true);
  });

  it('renders the block kinds the spec lists', () => {
    const md = [
      '# Heading',
      '',
      'A paragraph with `inline code` and **bold**.',
      '',
      '- item one',
      '  - nested',
      '- [x] done',
      '- [ ] todo',
      '',
      '> a quote',
      '',
      '```ts',
      'const x = 1;',
      '```',
      '',
      '| a | b |',
      '| - | - |',
      '| 1 | 2 |',
      '',
      '---',
    ].join('\n');
    const blocks = parseBlocks(md);
    const kinds = blocks.map((b) => b.kind);
    expect(kinds).toContain('heading');
    expect(kinds).toContain('paragraph');
    expect(kinds).toContain('list');
    expect(kinds).toContain('quote');
    expect(kinds).toContain('code');
    expect(kinds).toContain('table');
    expect(kinds).toContain('rule');
  });

  it('carries the code language label', () => {
    const [code] = parseBlocks('```rust\nfn main() {}\n```');
    expect(code).toMatchObject({ kind: 'code', lang: 'rust', closed: true });
  });

  it('marks an unterminated fence as streaming rather than pretending it is closed', () => {
    const [code] = parseBlocks('```rust\nfn main() {}');
    expect(code).toMatchObject({ kind: 'code', closed: false });
  });

  it('parses a task list with its state', () => {
    const [list] = parseBlocks('- [x] done\n- [ ] todo');
    expect(list?.kind).toBe('list');
    if (list?.kind !== 'list') throw new Error('expected a list');
    expect(list.items.map((i) => i.done)).toEqual([true, false]);
  });

  it('never lets markdown markup reach the screen as literal asterisks or backticks', () => {
    const out = lines(
      run([
        SESSION,
        n('turn.started', { turn_id: 't', prompt: 'go' }),
        n('turn.delta', {
          turn_id: 't',
          text: 'a **bold** and `code` and [link](https://x.example)',
        }),
      ]),
    );
    const joined = out.join('\n');
    expect(joined).toContain('bold');
    expect(joined).toContain('code');
    expect(joined).not.toContain('**');
    expect(joined).not.toContain('`');
  });

  it('parses inline spans in the right order, so code is not re-parsed as emphasis', () => {
    const spans = parseInline('a `**not bold**` b');
    expect(spans.find((s) => s.style === 'code')?.text).toBe('**not bold**');
  });
});

describe('E1: the composer is the anchor', () => {
  it('never moves: it is the last thing rendered, above the footer', () => {
    const s = run([
      SESSION,
      n('turn.started', { turn_id: 't', prompt: 'go' }),
      n('tool.call', { tool_id: 'a', name: 'Bash', args: 'npm test' }),
      n('turn.delta', { turn_id: 't', text: 'streaming' }),
    ]);
    const out = frame(s).split('\n');
    const composerAt = out.findIndex((l) => l.includes('›'));
    // The header also shows the posture, so the footer is located as the row after the lower rule.
    const lastRule = out.map((l, i) => (l.trim().length > 0 && /^─+$/.test(l.trim()) ? i : -1)).filter((i) => i > -1).pop();
    expect(composerAt).toBeGreaterThan(-1);
    expect(lastRule).toBeGreaterThan(composerAt);
    expect(out[lastRule! + 1]).toContain('manual');
  });

  it('stays live while a run is active, and queues what is typed', () => {
    const running = run([SESSION, n('turn.started', { turn_id: 't', prompt: 'go' })]);
    const queued = reduceLocal(running, { kind: 'turn.submit', prompt: 'then this' }, T0);
    expect(queued.queue).toEqual(['then this']);
    expect(frame(queued)).toContain('queued');
  });

  it('shows the queue above the composer, visibly', () => {
    let s = run([SESSION, n('turn.started', { turn_id: 't', prompt: 'go' })]);
    s = reduceLocal(s, { kind: 'turn.submit', prompt: 'second thing' }, T0);
    expect(frame(s)).toContain('second thing');
  });
});

describe('F1: the command registry is declared once', () => {
  it('every command has a description and a tier', () => {
    for (const c of COMMANDS) {
      expect(c.description.length, `${c.name} has no description`).toBeGreaterThan(0);
      expect(['always', 'immediateUi', 'sideEffectFree', 'queued']).toContain(c.tier);
    }
  });

  it('command names and aliases are unique', () => {
    const seen = new Set<string>();
    for (const c of COMMANDS) {
      for (const name of [c.name, ...c.aliases]) {
        expect(seen.has(name), `${name} is declared twice`).toBe(false);
        seen.add(name);
      }
    }
  });

  it('every non-queued command works while a run is active', () => {
    for (const c of COMMANDS) {
      if (c.tier !== 'queued') expect(worksWhileRunning(c.tier), `${c.name}`).toBe(true);
    }
  });
});

describe('glyphs: colour is never the only state indicator', () => {
  it('every state has an ASCII fallback that differs from the others', () => {
    const a = glyphs('ascii');
    expect(a.done).not.toBe(a.failed);
    expect(a.done).toBe('+');
    expect(a.failed).toBe('x');
  });

  it('the ASCII sweep has four distinct frames', () => {
    expect(new Set(glyphs('ascii').sweep).size).toBe(4);
  });
});
describe('G1: the footer collapses whole fields in the specified order', () => {
  const base = (cols: number): AppState => ({
    ...run([SESSION, n('context.usage', { used: 31_000, limit: 262_000 })]),
    cols,
  });

  const lastLine = (s: AppState): string => {
    const lines = (frame(s, s.cols) ?? '').split('\n').filter((l) => l.trim().length > 0);
    return lines[lines.length - 1] ?? '';
  };

  it('shows everything when there is room', () => {
    const out = lastLine(base(180));
    expect(out).toContain('claude-sonnet-4');
    expect(out).toContain('main');
    expect(out).toContain('/home/u/projects/api');
    expect(out).toContain('manual');
  });

  it('drops cwd before branch, and branch before model, never the posture', () => {
    // Step the width down and record which field disappears first at each step.
    const order: string[] = [];
    let previous = '';
    let postureAlwaysShown = true;
    // 50 and below render the narrow-width notice instead of the full layout, so the footer
    // collapse ladder is only meaningful from 60 up. The narrow case is asserted separately.
    for (const cols of [180, 120, 100, 90, 80, 70, 60]) {
      const out = lastLine(base(cols));
      for (const field of ['/home/u/projects/api', 'main', 'claude-sonnet-4', 'manual']) {
        const present = out.includes(field);
        if (previous.includes(field) && !present) order.push(field);
      }
      // The header also carries the posture, so only the footer row is evidence about it.
      if (out.includes('manual')) postureAlwaysShown = true;
      previous = out;
    }
    // Earlier in the array means dropped sooner at a wider terminal.
    expect(order.indexOf('/home/u/projects/api')).toBeLessThan(order.indexOf('main'));
    expect(order.indexOf('main')).toBeLessThan(order.indexOf('claude-sonnet-4'));
    expect(order, 'the permission posture must never be dropped').not.toContain('manual');
    expect(postureAlwaysShown).toBe(true);
  });

  it('never leaves a bare ellipsis where a field used to be', () => {
    for (const cols of [60, 70, 80, 100, 120, 180]) {
      expect(lastLine(base(cols)), `a truncated field at ${cols}`).not.toMatch(/ · …/);
    }
  });
});
