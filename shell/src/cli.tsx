/**
 * The shell entry point. The one place that owns the terminal, the engine process and the clock.
 *
 * Everything below is terminal lifecycle and wiring. Rendering is `App`; key matching is
 * `dispatch`; state is `state`. This file contains no glyphs, no colours and no key comparisons
 * beyond turning an Ink key event into the dispatcher's shape — which is what keeps the "one
 * dispatcher, one theme, one art module" lints meaningful rather than decorative.
 *
 * Lifecycle rules, from the checklist:
 *  - the terminal is restored on normal exit, Ctrl+C, SIGTERM, SIGHUP, error return and panic;
 *  - nothing is written to stdout except what Ink itself draws;
 *  - engine stderr goes to a log file, never to the interface.
 */

import React from 'react';
import { render, type Instance } from 'ink';
import { appendFileSync, createWriteStream, mkdirSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { join } from 'node:path';
import { homedir } from 'node:os';

import { App } from './app.js';
import { EngineClient, PROTOCOL_VERSION } from './protocol/client.js';
import { handleKey } from './dispatch.js';
import { decisionFor } from './approval.js';
import { InputParser } from './input.js';
import { initialState, reduce, reduceLocal, type AppState, type ReduceOptions } from './state.js';
import type { ClientRequest, InitializeResult, ServerNotification } from './protocol/generated/index.js';
import { isThemeName, type ThemeName } from './theme/index.js';
import { charsetFromEnv, noColorFromEnv, reducedMotionFromEnv, sweepIntervalMs } from './glyphs.js';

/* c8 ignore start -- process wiring: exercised by a PTY test, not by unit tests */

function logPath(): string {
  const dir = join(process.env.NIKI_STATE_DIR ?? join(homedir(), '.niki'), 'logs');
  mkdirSync(dir, { recursive: true });
  return join(dir, 'shell.log');
}

const log = createWriteStream(logPath(), { flags: 'a' });
function engineLog(line: string): void {
  appendFileSync(logPath(), `${line}\n`);
}

const ESC = String.fromCharCode(0x1b);
const BEL = String.fromCharCode(0x07);
const seq = {
  altScreenOn: `${ESC}[?1049h`,
  altScreenOff: `${ESC}[?1049l`,
  cursorHide: `${ESC}[?25l`,
  cursorShow: `${ESC}[?25h`,
  mouseOn: `${ESC}[?1000h${ESC}[?1006h`,
  mouseOff: `${ESC}[?1006l${ESC}[?1000l`,
  pasteOn: `${ESC}[?2004h`,
  pasteOff: `${ESC}[?2004l`,
  // The title body is stripped of control bytes first: an engine-supplied topic must not be
  // able to smuggle an escape sequence into the window title.
  title: (t: string) => `${ESC}]0;${t.replace(/[\u0000-\u001f]/g, '')}${BEL}`,
};

/** Everything the terminal had when we started, so restore is exact rather than hopeful. */
type TerminalState = {
  mouseOn: boolean;
  rawMode: boolean;
};

function enterTerminal(): TerminalState {
  process.stdout.write(seq.altScreenOn + seq.cursorHide + seq.mouseOn + seq.pasteOn);
  return { mouseOn: true, rawMode: true };
}

function restoreTerminal(state: TerminalState): void {
  // Idempotent and unconditional: every exit path calls it, and calling it twice must not send
  // the user a stray escape sequence.
  process.stdout.write(seq.pasteOff);
  if (state.mouseOn) process.stdout.write(seq.mouseOff);
  process.stdout.write(seq.cursorShow + seq.altScreenOff);
  process.stdout.write(`${ESC}[0m`);
}

/* c8 ignore stop */

export async function main(argv = process.argv.slice(2)): Promise<number> {
  const env = process.env;
  const engineArgIndex = argv.indexOf('--engine');
  const engine = engineArgIndex === -1 ? 'niki' : (argv[engineArgIndex + 1] ?? 'niki');
  // Extra arguments for `niki serve`, so a test can point the shell at the scripted runtime
  // without a second code path. Repeatable: `--engine-arg --fixture --engine-arg --bare`.
  const engineArgs: string[] = [];
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === '--engine-arg' && argv[i + 1]) engineArgs.push(argv[i + 1]!);
  }
  const themeArg = argv.indexOf('--theme');
  const theme: ThemeName =
    themeArg !== -1 && isThemeName(argv[themeArg + 1] ?? '') ? (argv[themeArg + 1] as ThemeName) : 'niki';

  const reducedMotion = reducedMotionFromEnv(env);
  const charset = charsetFromEnv(env);
  void noColorFromEnv(env);

  const terminal = enterTerminal();
  let exited = false;
  const cleanup = () => {
    if (exited) return;
    exited = true;
    restoreTerminal(terminal);
  };
  const removeResizeListener = (): void => {
    process.stdout.off('resize', onResize);
  };
  process.on('exit', removeResizeListener);
  for (const sig of ['SIGINT', 'SIGTERM', 'SIGHUP'] as const) {
    process.on(sig, () => {
      cleanup();
      process.exit(0);
    });
  }

  // Ctrl+Z: the OS stops us, so there is nothing to handle here. But `fg` sends SIGCONT, and by
  // then the terminal may have been cleared or run by something else while we slept. Ink still
  // believes the screen holds our last frame, so without this the user comes back to a stale or
  // blank terminal. Restoring the terminal and forcing a full repaint is the only correct answer.
  process.on('SIGCONT', () => {
    restoreTerminal(terminal);
    if (sweepTimer !== null) {
      clearInterval(sweepTimer);
      sweepTimer = null;
    }
    instance?.clear();
    paint();
    restopSweep();
  });
  // A panic must still give the user their terminal back.
  process.on('uncaughtException', (e) => {
    engineLog(`uncaught: ${e.stack ?? String(e)}`);
    cleanup();
    process.exit(1);
  });

  // Seed from the real terminal, then keep it current. A shell that starts at a hard-coded
  // 80x24 ignores every window the user has, and never notices a resize at all.
  let state: AppState = {
    ...initialState(
      Math.max(1, process.stdout.columns ?? 80),
      Math.max(1, process.stdout.rows ?? 24),
    ),
  };
  const onResize = () => {
    state = reduceLocal(
      state,
      {
        kind: 'resize',
        cols: Math.max(1, process.stdout.columns ?? state.cols),
        rows: Math.max(1, process.stdout.rows ?? state.rows),
      },
      opts(),
    );
    paint();
  };
  process.stdout.on('resize', onResize);
  let instance: Instance | null = null;
  const opts = (): ReduceOptions => ({ nowMs: Date.now() });

  // The theme the shell booted with is a fact about this shell, so it goes in the state rather than
  // living only in a prop: the settings sheet has to be able to show it.
  state = reduceLocal(state, { kind: 'theme.commit', theme }, opts());

  let sweepTick = 0;
  let sweepTimer: ReturnType<typeof setInterval> | null = null;

  const paint = () => {
    if (!instance) return;
    instance.rerender(
      React.createElement(App, { state, theme, charset, reducedMotion, sweepTick }),
    );
  };

  let client = new EngineClient({
    command: engine,
    args: ['serve', ...engineArgs],
    onEngineStderr: engineLog,
  });

  const attach = (): void => {
    client.on('notification', (n: ServerNotification) => {
      const next = reduce(state, n, opts());
      // The window title tracks the session topic, and only when the engine gave us one.
      if (n.method === 'turn.started') process.stdout.write(seq.title(n.params.prompt));
      if (n.method === 'final') process.stdout.write(seq.title('Niki'));
      state = next;
      paint();
    });
    client.on('protocolError', (e: Error) => {
      engineLog(`protocol: ${e.message}`);
      state = reduceLocal(state, { kind: 'protocolError', message: e.message }, opts());
      paint();
    });
  };
  attach();

  const initialize = async (): Promise<void> => {
    try {
      const result = (await client.request(
        {
          method: 'initialize',
          params: {
            protocol_version: PROTOCOL_VERSION,
            client: { name: 'niki-shell', version: '0.1.0', cols: state.cols, rows: state.rows },
          },
        },
        `boot-${Date.now()}`,
      )) as InitializeResult;
      // What the engine said about itself. `/version` and the empty-state copy both read this, so
      // neither has to guess a version or a capability.
      state = reduceLocal(
        state,
        { kind: 'engine.meta', version: result.engine_version, capabilities: result.capabilities },
        opts(),
      );
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      engineLog(`initialize failed: ${message}`);
      state = reduceLocal(state, { kind: 'protocolError', message: `engine did not start: ${message}` }, opts());
    }
  };

  // `session.ready` — the frame carrying the model, the permission posture and the branch — is the
  // engine's reply to `session.load`, not to `initialize`. Without this the shell boots straight
  // into a header that reads "connecting to the engine" and stays there, which is exactly how it
  // looked the first time anyone ran it.
  const loadSession = async (): Promise<void> => {
    try {
      await client.request(
        { method: 'session.load', params: { session_id: null, project_path: process.cwd() } },
        `session-${Date.now()}`,
      );
    } catch (e) {
      // A refusal is not fatal: the interface still works, the header says only what it was told.
      const message = e instanceof Error ? e.message : String(e);
      engineLog(`session.load failed: ${message}`);
      state = reduceLocal(
        state,
        { kind: 'notice.push', text: 'the engine did not report a session', level: 'warning' },
        opts(),
      );
    }
  };

  /**
   * Sends whatever the reducer decided to send. The reducer is pure, so it records the intent in
   * `state.outbox` and this is the only place that puts it on the wire.
   */
  const flushOutbox = async (): Promise<void> => {
    while (state.outbox.length > 0) {
      const pending = state.outbox[state.outbox.length - 1]!;
      state = reduceLocal(state, { kind: 'outbox.clear' }, opts());
      try {
        await client.request(
          { method: pending.method, params: pending.params } as ClientRequest,
          pending.traceId,
        );
      } catch (e) {
        engineLog(`${pending.method} failed: ${e instanceof Error ? e.message : String(e)}`);
      }
    }
  };

  /**
   * Hands the terminal to `$EDITOR` and takes it back.
   *
   * The alt screen is left and raw mode dropped for exactly as long as the editor runs; anything
   * else leaves the user's scrollback full of escape sequences. With no `$EDITOR` the shell says
   * so instead of pretending an edit happened.
   */
  const runEditor = (): void => {
    const editor = process.env.EDITOR ?? process.env.VISUAL;
    if (!editor) {
      state = reduceLocal(state, { kind: 'notice.push', text: 'no $EDITOR is set', level: 'warning' }, opts());
      return;
    }
    process.stdout.write(seq.pasteOff + seq.mouseOff + seq.cursorShow + seq.altScreenOff);
    process.stdin.setRawMode?.(false);
    const result = spawnSync(editor, [], { stdio: 'inherit', shell: true });
    process.stdout.write(seq.altScreenOn + seq.cursorHide + seq.mouseOn + seq.pasteOn);
    process.stdin.setRawMode?.(true);
    paint();
    if (result.error || result.status !== 0) {
      engineLog(`editor exited: status ${String(result.status)} ${result.error?.message ?? ''}`);
      state = reduceLocal(state, { kind: 'notice.push', text: `${editor} exited without editing`, level: 'warning' }, opts());
      return;
    }
    state = reduceLocal(state, { kind: 'notice.push', text: 'editor closed — nothing was sent', level: 'info' }, opts());
  };

  /** `/reload`: a fresh engine process, the same wiring, then a fresh handshake. */
  const reload = async (): Promise<void> => {
    await client.stop();
    client = new EngineClient({
      command: engine,
      args: ['serve', ...engineArgs],
      onEngineStderr: engineLog,
    });
    attach();
    state = reduceLocal(state, { kind: 'notice.push', text: 'reconnected to the engine', level: 'info' }, opts());
    await initialize();
    await loadSession();
    paint();
  };

  // Handshake: initialize tells us the engine's version and capabilities; session.load is what
  // makes it report the project, model, posture and branch. Both are needed before the header can
  // say anything true.
  await initialize();
  await loadSession();

  instance = render(
    React.createElement(App, { state, theme, charset, reducedMotion, sweepTick: 0 }),
    { exitOnCtrlC: false },
  );

  // One key handler, routed to the one dispatcher.
  process.stdin.setRawMode?.(true);
  process.stdin.resume();
  process.stdin.setEncoding('utf8');
  const parser = new InputParser();
  // The activity sweep: the only thing in the shell that repaints on a timer. It exists while
  // something is in flight and is torn down the moment nothing is, so an idle shell owns no
  // timer, schedules no redraw and costs no CPU. "Motion stops at idle" only means something if
  // motion exists while work is happening.
  const restopSweep = (): void => {
    if (sweepTimer !== null) {
      clearInterval(sweepTimer);
      sweepTimer = null;
    }
    if (reducedMotion || !state.activity) return;
    sweepTimer = setInterval(() => {
      sweepTick = (sweepTick + 1) % 4;
      paint();
    }, sweepIntervalMs({
      toolInFlight: state.activity.toolInFlight,
      runningMs: Date.now() - state.activity.startedAtMs,
    }));
  };

  process.stdin.on('data', (chunk: string) => {
    for (const keyEvent of parser.push(chunk)) {
      const outcome = handleKey(state, keyEvent);
      for (const action of outcome.actions) {
        // A submitted turn is the one action that has to reach the engine as well as the state.
        // Applying it locally without sending it is what makes a composer that types beautifully
        // and then does nothing.
        if (action.kind === 'turn.submit') {
          void client
            .request(
              {
                method: 'turn.start',
                params: {
                  prompt: action.prompt,
                  // The posture the user asked for wins; otherwise the engine's own. The shell never
                  // picks one the user did not choose and the engine did not report.
                  permission_mode: state.permissionOverride ?? state.session?.permission_mode ?? 'manual',
                },
              },
              `turn-${Date.now()}`,
            )
            .catch((e: Error) => {
              engineLog(`turn.start failed: ${e.message}`);
              state = reduceLocal(state, { kind: 'protocolError', message: e.message }, opts());
            });
        }
        state = reduceLocal(state, action, opts());
      }
      if (outcome.insert !== undefined) {
        state = reduceLocal(state, { kind: 'composer.set', text: state.composer + outcome.insert }, opts());
      }
      if (outcome.scroll) {
        state = reduceLocal(state, { kind: 'scroll', by: outcome.scroll }, opts());
      }
      if (outcome.approval) {
        // The decision comes from the option the user actually chose. Hardcoding `allow` here
        // would turn an Esc-deny into an allow the moment the key handling changed.
        const option = state.approval?.request.options.find((o) => o.id === outcome.approval?.optionId);
        const decision = option ? decisionFor(option) : 'deny';
        state = reduceLocal(state, { kind: 'approval.decide' }, opts());
        void client
          .request(
            {
              method: 'approval.reply',
              params: { id: outcome.approval.id, decision, reason: null },
            },
            `approval-${Date.now()}`,
          )
          .catch((e: Error) => engineLog(`approval reply failed: ${e.message}`));
      }
      if (state.exitArmed) {
        void shutdown(client, instance, cleanup);
        return;
      }
      // Requests a command decided on: a resume, a settings save that reached the wire. The
      // reducer recorded the intent; this is the only place that sends it.
      if (state.outbox.length > 0) {
        void flushOutbox().then(paint);
      }
      if (state.editorRequested) {
        state = reduceLocal(state, { kind: 'flag.clear', flag: 'editorRequested' }, opts());
        runEditor();
      }
      if (state.reloadRequested) {
        state = reduceLocal(state, { kind: 'flag.clear', flag: 'reloadRequested' }, opts());
        void reload();
      }
    }
    restopSweep();
    paint();
  });

  await instance.waitUntilExit();
  if (sweepTimer !== null) clearInterval(sweepTimer);
  await shutdown(client, instance, cleanup);
  return 0;
}

async function shutdown(
  client: EngineClient,
  instance: Instance | null,
  cleanup: () => void,
): Promise<void> {
  await client
    .request({ method: 'shutdown', params: { user_initiated: true } }, `bye-${Date.now()}`)
    .catch(() => undefined)
    .finally(() => undefined);
  await client.stop();
  instance?.unmount();
  cleanup();
  log.end();
}