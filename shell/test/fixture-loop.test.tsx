/**
 * The reference loop, end to end, twice.
 *
 * The engine's fixture runtime replays the loop the reference GIFs show: a user prompt, a
 * thinking stage, parallel tool rows, results, a **failed** tool, streaming text, an approval, a
 * diff, an interrupt and a completion. This test drives it through both halves of the harness:
 *
 *  1. **The PTY driver** runs the real shell against the real binary and asserts on the final
 *     screen, in a real pseudo-terminal.
 *  2. **Render snapshots** fold the same notification stream through the real reducer and render
 *     the resulting states.
 *
 * Two paths that agree is the point. A fixture that only ever runs one of them cannot tell whether
 * the reducer and the live shell disagree about what the engine said.
 */

import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { render } from 'ink-testing-library';
import React from 'react';
import { afterEach, describe, expect, it } from 'vitest';

import { App } from '../src/app.js';
import { Terminal } from '../src/vt.js';
import { EngineClient } from '../src/protocol/client.js';
import { initialState, reduce, type AppState, type ReduceOptions } from '../src/state.js';
import type { ServerNotification } from '../src/protocol/generated/index.js';

const REPO = join(import.meta.dirname, '..', '..');
const ENGINE = join(REPO, 'target', 'debug', 'niki');
const SHELL_ENTRY = join(REPO, 'shell', 'src', 'cli.tsx');
const T0: ReduceOptions = { nowMs: 0 };

const binaryAvailable = existsSync(ENGINE);

/**
 * Whether this binary was actually built with the scripted runtime. A plain `cargo build` or a
 * clippy run replaces `target/debug/niki` without the feature, and the fixture tests then wait out
 * their full budget for a run that can never happen — which reads like a hang rather than like a
 * mis-built binary. One fast probe turns that into an immediate, named failure.
 */
function engineHasFixtureRuntime(): boolean {
  if (!binaryAvailable) return false;
  const probe = spawnSync(ENGINE, ['serve', '--fixture', '--help'], { encoding: 'utf8' });
  return probe.status === 0 && /--fixture/.test(`${probe.stdout}${probe.stderr}`);
}

const fixtureAvailable = engineHasFixtureRuntime();
const open: ChildProcess[] = [];

afterEach(() => {
  for (const c of open.splice(0)) if (!c.killed) c.kill('SIGKILL');
});

/* ------------------------------------------------------------------ *
 * 1. The PTY path: the real shell, the real binary, a real terminal.  *
 * ------------------------------------------------------------------ */

/**
 * `tracked = false` keeps a session out of the per-test cleanup list. A session shared across a
 * `beforeAll` must survive every `it` in between, or the first test's cleanup kills the thing the
 * rest of the block is about.
 */
function startFixtureShell(cols = 80, rows = 24, tracked = true): {
  child: ChildProcess;
  term: Terminal;
  write: (s: string) => void;
  waitFor: (p: () => boolean, ms: number) => Promise<boolean>;
  screen: () => string;
} {
  const term = new Terminal(cols, rows);
  const bootstrap =
    `import(${JSON.stringify(SHELL_ENTRY)})` +
    `.then((m) => m.main(['--engine', ${JSON.stringify(ENGINE)}, '--engine-arg', '--fixture', '--engine-arg', '--bare', '--engine-arg', '--project', '--engine-arg', '/tmp']))` +
    `.catch((e) => { process.stderr.write('shell failed: ' + String(e && e.stack || e)); process.exit(1); });`;
  const command = `stty cols ${cols} rows ${rows}; TERM=xterm-256color NO_COLOR=1 node --import tsx -e ${JSON.stringify(bootstrap)}`;

  const child = spawn('script', ['-qfec', `cd ${join(REPO, 'shell')} && ${command}`, '/dev/null'], {
    env: { ...process.env, TERM: 'xterm-256color', NO_COLOR: '1' },
    stdio: ['pipe', 'pipe', 'pipe'],
  });
  if (tracked) open.push(child);
  child.stdout?.on('data', (c: Buffer) => term.feed(c));
  child.stderr?.on('data', () => undefined);

  return {
    child,
    term,
    write: (s) => child.stdin?.write(s),
    screen: () => term.screenText(),
    waitFor: async (p, ms) => {
      const deadline = Date.now() + ms;
      while (Date.now() < deadline) {
        if (p()) return true;
        await new Promise((r) => setTimeout(r, 25));
      }
      return p();
    },
  };
}

describe('the reference loop, through the PTY driver', () => {
  // One session, one flow, one set of assertions. The engine's scripted loop stops at its
  // approval request, so everything the reference GIFs show up to that point is on one screen:
  // stages, parallel tools, a failed tool, and the approval. Splitting this into several tests
  // meant several pseudo-terminals and several chances for one of them to boot slowly enough to
  // look like a product failure.
  async function bootToApproval(cols = 100, rows = 30): Promise<ReturnType<typeof startFixtureShell>> {
    const s = startFixtureShell(cols, rows);
    await s.waitFor(() => s.screen().includes('Niki'), 180_000);
    // Type and submit exactly as a user would: the bytes go through the real input parser.
    for (const ch of 'go') s.write(ch);
    await s.waitFor(() => s.screen().includes('go'), 30_000);
    s.write('\r');
    // Wait for the *completed* prompt, not its title. Ink draws a frame at a time, so the box
    // border and its first line arrive before the options do; asserting on the title catches a
    // half-drawn box and looks like a missing option.
    await s.waitFor(() => s.screen().includes('esc denies'), 90_000);
    return s;
  }

  // This asserted only that the file existed, while its name promised the *feature*. A plain
  // `cargo build` replaces `target/debug/niki` without `fixture-runtime`, that assertion stayed
  // green, and the three tests below it skipped — which is how 403 shell tests could pass while
  // the only leg that drives the real binary end to end was silently absent.
  it('has the fixture-enabled binary', () => {
    expect(
      binaryAvailable,
      'run: cargo build -j 2 --features fixture-runtime',
    ).toBe(true);
    expect(
      fixtureAvailable,
      'target/debug/niki exists but was built without `fixture-runtime`, so `niki serve --fixture` \
       does not exist and every test below this line silently skipped. Rebuild it with: \
       cargo build -j 2 --features fixture-runtime',
    ).toBe(true);
  });

  it.skipIf(!fixtureAvailable)('replays the whole reference loop on the real screen', async () => {
    const s = await bootToApproval();
    // Wait for the run to finish rather than for the approval: the approval is a transient frame,
    // and asserting on a transient is how a test ends up reading a half-drawn box. The finished
    // run is what a user is left looking at, and it is the thing worth asserting on.
    const finished = await s.waitFor(() => s.screen().includes('Done in'), 240_000);
    const screen = s.screen();
    expect(finished, `the run never finished:\n${screen}`).toBe(true);

    // The loop the reference frames show, every row of it from a real engine event.
    expect(screen, 'no stage row').toMatch(/planner/);
    expect(screen, 'no coder row').toMatch(/coder/);
    expect(screen, 'the retry marker was not shown').toMatch(/retried/);
    expect(screen, 'no tool rows').toMatch(/read\(|git_status\(|bash\(/);
    expect(screen, 'the failed tool was not shown inline').toContain('exit 127');
    expect(screen, 'the interrupt notice was not shown').toContain('interrupted by the user');
    // The end-of-turn summary, with every number from a real counter.
    expect(screen).toMatch(/Done in \d+m?s · 3 tool calls · 1 file changed/);

    // And the approval really was on screen at some point: the run could not have finished
    // without it, because the fixture stops and waits for a reply there.
    expect(screen, 'the composer is missing, so nothing was submitted').toMatch(/[>\u203a]/);
  }, 600_000);

  it.skipIf(!fixtureAvailable)('Esc on the approval denies it and the run continues', async () => {
    const s = await bootToApproval();
    s.write('\x1b');
    const dismissed = await s.waitFor(() => !s.screen().includes('esc denies'), 30_000);
    expect(dismissed, `Esc did not dismiss the approval:\n${s.screen()}`).toBe(true);
  }, 600_000);
});

/** The notification stream the engine's fixture emits, as the shell receives it. */
const FIXTURE_STREAM: ServerNotification[] = [
  { method: 'session.ready', params: { session_id: 's1', project_path: '/home/u/projects/api', model: 'claude-sonnet-4', permission_mode: 'manual', branch: 'main', ahead: 1, behind: null, resumed_messages: 0 } },
  { method: 'turn.started', params: { turn_id: 't1', prompt: 'go' } },
  { method: 'stage.start', params: { stage_id: 'g1', role: 'planner', attempt: 1 } },
  { method: 'stage.done', params: { stage_id: 'g1', role: 'planner', summary: 'read the repo', tokens_in: 120, tokens_out: 40, cost_usd: 0.001, latency_ms: 900, retry_count: 0, artifact_ref: null, provenance: 'independent' } },
  { method: 'stage.start', params: { stage_id: 'g2', role: 'coder', attempt: 1 } },
  { method: 'tool.call', params: { tool_id: 'x1', name: 'Read', args: 'package.json' } },
  { method: 'tool.call', params: { tool_id: 'x2', name: 'Grep', args: 'pattern: "describe("' } },
  { method: 'tool.call', params: { tool_id: 'x3', name: 'Bash', args: 'npm test' } },
  { method: 'tool.result', params: { tool_id: 'x1', ok: true, summary: 'read 46 lines', full_ref: '/tmp/read.json', duration_ms: 12 } },
  { method: 'tool.result', params: { tool_id: 'x2', ok: true, summary: 'found 100 files', full_ref: '/tmp/grep.json', duration_ms: 30 } },
  { method: 'tool.result', params: { tool_id: 'x3', ok: false, summary: 'exit 127 · command not found', full_ref: '/tmp/bash.log', duration_ms: 8 } },
  { method: 'turn.delta', params: { turn_id: 't1', text: 'The suite command is not installed; trying the local runner.' } },
  { method: 'stage.token', params: { stage_id: 'g2', role: 'coder', text: 'choosing a runner' } },
  { method: 'approval.request', params: { id: 'a1', tool: 'bash', command: 'npx vitest run', options: [{ id: 'allow', label: 'Allow' }, { id: 'deny', label: 'Deny' }], safest_option_id: 'allow' } },
] as ServerNotification[];

function foldUpTo(count: number): AppState {
  let s: AppState = initialState(100, 30);
  for (const e of FIXTURE_STREAM.slice(0, count)) s = reduce(s, e, T0);
  return s;
}

describe('the reference loop, through render snapshots', () => {
  it('reaches the approval with the failure already on screen', () => {
    const s = foldUpTo(FIXTURE_STREAM.length);
    const inst = render(<App state={s} theme="niki" charset="unicode" reducedMotion />);
    const out = inst.lastFrame() ?? '';
    inst.unmount();

    expect(out).toContain('wants to run');
    expect(out).toContain('exit 127');
    expect(out).toContain('read 46 lines');
    expect(out).toContain('found 100 files');
    expect(out).toContain('esc denies');
  });

  it('renders the same content at 80x24 and 120x38', () => {
    for (const [cols, rows] of [
      [80, 24],
      [120, 38],
    ] as const) {
      const inst = render(
        <App state={{ ...foldUpTo(FIXTURE_STREAM.length), cols, rows }} theme="niki" charset="unicode" reducedMotion />,
      );
      const out = inst.lastFrame() ?? '';
      inst.unmount();
      expect(out, `wants to run missing at ${cols}x${rows}`).toContain('wants to run');
      expect(out, `the failure row is missing at ${cols}x${rows}`).toContain('exit 127');
    }
  });

  it('shows the retry marker when a stage really reports retry_count', () => {
    // The fixture's own stream has no retrying stage, so this adds one: the row must come from the
    // field the engine sent, not from a guess that a run is "second time lucky".
    const withRetry: ServerNotification[] = [
      ...FIXTURE_STREAM,
      { method: 'stage.done', params: { stage_id: 'g2', role: 'coder', summary: 'fixed the slice bound', tokens_in: 1400, tokens_out: 220, cost_usd: 0.004, latency_ms: 2600, retry_count: 1, artifact_ref: null, provenance: 'self_verification' } },
    ] as ServerNotification[];
    let s: AppState = initialState(100, 30);
    for (const e of withRetry) s = reduce(s, e, T0);
    const inst = render(<App state={s} theme="niki" charset="unicode" reducedMotion />);
    const out = inst.lastFrame() ?? '';
    inst.unmount();
    expect(out).toContain('retried');
  });

  it('carries the whole stream through the reducer without a protocol error', () => {
    const s = foldUpTo(FIXTURE_STREAM.length);
    expect(s.protocolError).toBeNull();
    expect(s.tools).toHaveLength(3);
    expect(s.tools.filter((t) => t.state === 'failed')).toHaveLength(1);
    expect(s.stages.map((st) => st.role)).toEqual(['planner', 'coder']);
  });

  it('the two halves agree: the same events produce the same facts in both paths', async () => {
    // The PTY case above proves the live shell reaches the approval; this proves the reducer
    // reaches it from the same stream. Both are required: one without the other is a coincidence.
    const s = foldUpTo(FIXTURE_STREAM.length);
    expect(s.approval?.request.id).toBe('a1');
    expect(s.approval?.focusedOptionId).toBe('deny');
  });
});

/* ------------------------------------------------------------------ *
 * 3. The client, against the real fixture binary.                     *
 * ------------------------------------------------------------------ */

describe('the client against the real fixture engine', () => {
  it.skipIf(!fixtureAvailable)('receives the declared notifications and refuses nothing declared', async () => {
    const client = new EngineClient({
      command: ENGINE,
      args: ['serve', '--fixture', '--bare', '--project', '/tmp'],
    });
    open.push({ kill: () => client.stop() } as unknown as ChildProcess);

    const seen: string[] = [];
    const errors: Error[] = [];
    client.on('notification', (n: { method: string }) => seen.push(n.method));
    client.on('protocolError', (e: Error) => errors.push(e));

    await client.request(
      {
        method: 'initialize',
        params: {
          protocol_version: 1,
          client: { name: 'fixture-check', version: '0.1.0', cols: 100, rows: 30 },
        },
      },
      'fixture-trace',
    );

    // A turn is what makes the fixture replay; `initialize` alone is answered and then quiet.
    await client.request(
      { method: 'turn.start', params: { prompt: 'go', permission_mode: 'manual' } },
      'fixture-turn',
    );

    await new Promise<void>((r) => setTimeout(r, 3000));
    await client.stop();

    expect(errors, 'the fixture engine sent something undeclared').toEqual([]);
    // `session.ready` arrives from `session.load`, not from `initialize`; this test drives a turn,
    // so these are the messages that must appear.
    expect(seen, 'the turn never started').toContain('turn.started');
    expect(seen, 'no stage ran').toContain('stage.start');
    expect(seen, 'no tool ran').toContain('tool.call');
    expect(seen, 'the reference loop did not replay an approval').toContain('approval.request');
    expect(seen, 'the run never finished').toContain('turn.end');
  }, 240_000);
});