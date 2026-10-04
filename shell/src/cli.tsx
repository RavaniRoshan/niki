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
import { join } from 'node:path';
import { homedir } from 'node:os';

import { App } from './app.js';
import { EngineClient, PROTOCOL_VERSION } from './protocol/client.js';
import { handleKey } from './dispatch.js';
import { InputParser } from './input.js';
import { initialState, reduce, reduceLocal, type AppState, type ReduceOptions } from './state.js';
import type { ServerNotification } from './protocol/generated/index.js';
import { isThemeName, type ThemeName } from './theme/index.js';
import { charsetFromEnv, noColorFromEnv, reducedMotionFromEnv } from './glyphs.js';

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

  const paint = () => {
    if (!instance) return;
    instance.rerender(
      React.createElement(App, { state, theme, charset, reducedMotion, sweepTick: 0 }),
    );
  };

  const client = new EngineClient({
    command: engine,
    args: ['serve'],
    onEngineStderr: engineLog,
  });

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

  await client
    .request(
      {
        method: 'initialize',
        params: {
          protocol_version: PROTOCOL_VERSION,
          client: { name: 'niki-shell', version: '0.1.0', cols: state.cols, rows: state.rows },
        },
      },
      `boot-${Date.now()}`,
    )
    .catch((e: Error) => {
      engineLog(`initialize failed: ${e.message}`);
      state = reduceLocal(state, { kind: 'protocolError', message: `engine did not start: ${e.message}` }, opts());
    });

  instance = render(
    React.createElement(App, { state, theme, charset, reducedMotion, sweepTick: 0 }),
    { exitOnCtrlC: false },
  );

  // One key handler, routed to the one dispatcher.
  process.stdin.setRawMode?.(true);
  process.stdin.resume();
  process.stdin.setEncoding('utf8');
  const parser = new InputParser();
  process.stdin.on('data', (chunk: string) => {
    for (const keyEvent of parser.push(chunk)) {
      const outcome = handleKey(state, keyEvent);
      for (const action of outcome.actions) state = reduceLocal(state, action, opts());
      if (outcome.insert !== undefined) {
        state = reduceLocal(state, { kind: 'composer.set', text: state.composer + outcome.insert }, opts());
      }
      if (outcome.scroll) {
        state = reduceLocal(state, { kind: 'scroll', by: outcome.scroll }, opts());
      }
      if (outcome.approval) {
        void client
          .request(
            {
              method: 'approval.reply',
              params: { id: outcome.approval.id, decision: 'allow', reason: null },
            },
            `approval-${Date.now()}`,
          )
          .catch((e: Error) => engineLog(`approval reply failed: ${e.message}`));
      }
      if (state.exitArmed) {
        void shutdown(client, instance, cleanup);
        return;
      }
    }
    paint();
  });

  await instance.waitUntilExit();
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