/**
 * The shell's client for the NIKI engine.
 *
 * The engine is a child process speaking newline-delimited JSON-RPC 2.0 over stdio. This module
 * owns that pipe and nothing else: it spawns, it frames, it validates, it correlates. It never
 * reads the filesystem, never calls a model, never invents an event. Everything the UI shows
 * arrives here first.
 *
 * The spawn function is injectable so a test can drive the client from a scripted engine with no
 * process at all, and so a PTY test can drive it from the real binary.
 */

import { spawn as nodeSpawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { EventEmitter } from 'node:events';
import { notificationSchema, responseSchema } from './schemas.js';
import type { ClientRequest, ClientResult, ServerNotification } from './generated/index.js';

export const PROTOCOL_VERSION = 1;

/**
 * The child handle the client needs. `on` is optional so a test can supply a plain stub without
 * emulating the whole EventEmitter.
 */
export type EngineChild = {
  readonly stdin: { write(chunk: string): unknown };
  readonly stdout: {
    setEncoding(enc: string): void;
    on(event: 'data', cb: (chunk: string) => void): unknown;
  };
  readonly stderr: {
    setEncoding(enc: string): void;
    on(event: 'data', cb: (chunk: string) => void): unknown;
  };
  kill(signal?: NodeJS.Signals): unknown;
  on?(event: string, cb: (e: Error) => void): unknown;
};

export type SpawnFn = (
  command: string,
  args: readonly string[],
  options: { stdio: ['pipe', 'pipe', 'pipe'] },
) => EngineChild;

export type ClientOptions = {
  readonly command: string;
  readonly args?: readonly string[];
  readonly spawn?: SpawnFn;
  readonly onEngineStderr?: (line: string) => void;
};

export type PendingRequest = {
  resolve: (value: ClientResult) => void;
  reject: (reason: Error) => void;
};

const defaultSpawn: SpawnFn = (command, args, options) =>
  nodeSpawn(command, [...args], { stdio: options.stdio }) as unknown as EngineChild;

/**
 * Emits:
 *  - `notification` (ServerNotification)
 *  - `response`    ({ id, outcome })
 *  - `protocolError` (Error) — a frame that did not validate, or a line that was not JSON
 */
export class EngineClient extends EventEmitter {
  readonly #child: EngineChild;
  readonly #pending = new Map<number, PendingRequest>();
  #nextId = 1;
  #buffer = '';
  #closed = false;

  constructor(options: ClientOptions) {
    super();
    const spawn = options.spawn ?? defaultSpawn;
    this.#child = spawn(options.command, options.args ?? [], {
      stdio: ['pipe', 'pipe', 'pipe'],
    });

    this.#child.stdout.setEncoding('utf8');
    this.#child.stdout.on('data', (chunk: string) => this.#onChunk(chunk));
    this.#child.stderr.setEncoding('utf8');
    this.#child.stderr.on('data', (chunk: string) => {
      // Engine stderr goes to a log sink or nowhere. It must never reach the TUI's own stderr
      // while the interface is live (checklist D6).
      if (options.onEngineStderr) {
        for (const line of chunk.split('\n')) {
          if (line.trim()) options.onEngineStderr(line);
        }
      }
    });
    this.#child.on?.('error', (e: Error) => this.emit('protocolError', e));
  }

  get closed(): boolean {
    return this.#closed;
  }

  /** Writes one request and resolves with its typed result. */
  request(call: ClientRequest, traceId: string): Promise<ClientResult> {
    if (this.#closed) return Promise.reject(new Error('engine client is closed'));
    const id = this.#nextId++;
    const frame = { jsonrpc: '2.0', id, trace_id: traceId, ...call };
    return new Promise<ClientResult>((resolve, reject) => {
      this.#pending.set(id, { resolve, reject });
      this.#write(JSON.stringify(frame));
    });
  }

  /** Resolves the engine's process exit, killing it first if it is still running. */
  async stop(signal: NodeJS.Signals = 'SIGTERM'): Promise<void> {
    this.#closed = true;
    for (const [, p] of this.#pending) p.reject(new Error('engine client closed before the reply'));
    this.#pending.clear();
    this.#child.kill(signal);
  }

  /** Test seam: pushes a line through the exact parser the real pipe uses. */
  ingestForTest(line: string): void {
    this.#onChunk(line);
  }

  #write(line: string): void {
    this.#child.stdin.write(`${line}\n`);
  }

  #onChunk(chunk: string): void {
    this.#buffer += chunk;
    let index = this.#buffer.indexOf('\n');
    while (index !== -1) {
      const line = this.#buffer.slice(0, index);
      this.#buffer = this.#buffer.slice(index + 1);
      this.#onLine(line);
      index = this.#buffer.indexOf('\n');
    }
  }

  #onLine(rawLine: string): void {
    const line = rawLine.trim();
    if (line === '') return;

    let parsed: unknown;
    try {
      parsed = JSON.parse(line);
    } catch {
      this.emit('protocolError', new Error(`engine sent a line that is not JSON: ${clip(line)}`));
      return;
    }

    const decoded = notificationSchema.safeParse(parsed);
    if (decoded.success) {
      const frame = decoded.data;
      if (frame.kind === 'notification') this.emit('notification', frame.notification);
      return;
    }

    const response = responseSchema.safeParse(parsed);
    if (response.success) {
      const frame = response.data;
      if (frame.kind !== 'response') return;
      const { id, outcome } = frame;
      const pending = this.#pending.get(id);
      if (!pending) {
        this.emit('protocolError', new Error(`engine replied to unknown request id ${id}`));
        return;
      }
      this.#pending.delete(id);
      if ('error' in outcome) {
        const { error } = outcome;
        pending.reject(Object.assign(new Error(error.message), { code: error.code }));
      } else {
        // zod validates shape, not meaning, so the wire value is `unknown` here. The caller asked
        // for a typed result and knows which method it sent, so this is its contract rather than
        // an unchecked cast.
        pending.resolve(outcome.result as ClientResult);
      }
      return;
    }

    // A frame that is neither a declared notification nor a declared response is a protocol
    // break. It is reported, never rendered.
    this.emit(
      'protocolError',
      new Error(
        `engine sent a frame that matches no declared message: ${clip(line)}\n` +
          `  notification: ${decoded.error.issues
            .map((issue: { path: (string | number)[]; message: string }) =>
              `${issue.path.join('.')} ${issue.message}`,
            )
            .join('; ')}`,
      ),
    );
  }
}

function clip(line: string, max = 160): string {
  return line.length > max ? `${line.slice(0, max)}…` : line;
}

export type { ServerNotification };