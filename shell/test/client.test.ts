/**
 * The client test: the shell must handle every message the engine can send, and refuse anything
 * else. A scripted engine stands in for the child process, so this drives the real parser, the
 * real validator and the real correlation map — only the pipe is a stub.
 */

import { EventEmitter } from 'node:events';
import { describe, expect, it } from 'vitest';

import { EngineClient, type EngineChild, type SpawnFn } from '../src/protocol/client.js';
import { HANDLED_METHODS, NOTIFICATION_SCHEMAS } from '../src/protocol/schemas.js';

type Recorded = { line: string };

function scriptedEngine() {
  const written: Recorded[] = [];
  const child = Object.assign(new EventEmitter(), {
    stdin: { write: (line: string) => written.push({ line }) },
    stdout: new EventEmitter() as unknown as EngineChild['stdout'],
    stderr: new EventEmitter() as unknown as EngineChild['stderr'],
    kill: () => true,
  }) as unknown as EngineChild;
  const stream = () =>
    Object.assign(new EventEmitter(), { setEncoding: () => undefined }) as unknown as EventEmitter;
  (child as unknown as { stdout: EventEmitter }).stdout = stream();
  (child as unknown as { stderr: EventEmitter }).stderr = stream();

  const spawn: SpawnFn = () => child;
  return { spawn, written, emitLine: (line: string) => (child.stdout as unknown as EventEmitter).emit('data', line) };
}

const sampleParams: Record<string, unknown> = {
  'session.ready': {
    session_id: 's1',
    project_path: '/tmp/p',
    model: 'm',
    permission_mode: 'manual',
    branch: null,
    ahead: null,
    behind: null,
    resumed_messages: 0,
  },
  'turn.started': { turn_id: 't1', prompt: 'go' },
  'turn.delta': { turn_id: 't1', text: 'hi' },
  'turn.end': { turn_id: 't1', summary: 's', duration_ms: 1, tool_calls: 0, files_changed: 0 },
  'stage.start': { stage_id: 'g', role: 'coder', attempt: 1 },
  'stage.token': { stage_id: 'g', role: 'coder', text: 't' },
  'stage.done': {
    stage_id: 'g',
    role: 'coder',
    summary: 's',
    tokens_in: 1,
    tokens_out: 1,
    cost_usd: 0,
    latency_ms: 1,
    retry_count: 0,
    artifact_ref: null,
    provenance: 'independent',
  },
  'stage.failed': { stage_id: 'g', role: 'coder', error: 'e', severity: 'error', recovery: null },
  'tool.call': { tool_id: 'x', name: 'read', args: 'a' },
  'tool.progress': { tool_id: 'x', note: 'n' },
  'tool.result': { tool_id: 'x', ok: true, summary: 's', full_ref: null, duration_ms: 1 },
  'tool.diff': { tool_id: 'x', path: 'p', hunks: [] },
  'approval.request': {
    id: 'a',
    tool: 'bash',
    command: 'ls',
    options: [{ id: 'allow', label: 'Allow' }],
    safest_option_id: 'allow',
  },
  'plan.update': { items: [] },
  notice: { text: 'n', level: 'info' },
  'diff.ready': { ref: 'd' },
  'verdict.ready': { ref: 'v', verdict: 'approved', provenance: 'independent' },
  'branch.created': { name: 'niki/1' },
  'context.usage': { used: 1, limit: 2 },
  'cost.update': { usd: 1 },
  final: { verdict: null, error: null },
};

describe('EngineClient', () => {
  it('handles every declared notification', () => {
    const { spawn, emitLine } = scriptedEngine();
    const client = new EngineClient({ command: 'x', spawn });
    const seen: string[] = [];
    client.on('notification', (n: { method: string }) => seen.push(n.method));
    const errors: Error[] = [];
    client.on('protocolError', (e: Error) => errors.push(e));

    for (const method of HANDLED_METHODS) {
      const schema = NOTIFICATION_SCHEMAS[method];
      expect(schema.parse(sampleParams[method]), `${method} sample must satisfy its schema`).toBeTruthy();
      emitLine(`${JSON.stringify({ jsonrpc: '2.0', trace_id: 't', method, params: sampleParams[method] })}\n`);
    }

    expect(seen.sort()).toEqual([...HANDLED_METHODS].sort());
    expect(errors, 'no declared message may be reported as a protocol error').toEqual([]);
  });

  it('reports an undeclared method as a protocol error instead of ignoring it', () => {
    const { spawn, emitLine } = scriptedEngine();
    const client = new EngineClient({ command: 'x', spawn });
    const errors: Error[] = [];
    const seen: string[] = [];
    client.on('protocolError', (e: Error) => errors.push(e));
    client.on('notification', () => seen.push('nope'));

    emitLine(`${JSON.stringify({ jsonrpc: '2.0', trace_id: 't', method: 'stage.telepathy', params: {} })}\n`);
    expect(seen).toEqual([]);
    expect(errors).toHaveLength(1);
    expect(errors[0]!.message).toContain('stage.telepathy');
  });

  it('reassembles a frame that arrives split across chunks', () => {
    const { spawn, emitLine } = scriptedEngine();
    const client = new EngineClient({ command: 'x', spawn });
    const seen: string[] = [];
    client.on('notification', (n: { method: string }) => seen.push(n.method));

    const line = `${JSON.stringify({ jsonrpc: '2.0', trace_id: 't', method: 'notice', params: { text: 'x', level: 'info' } })}\n`;
    emitLine(line.slice(0, 20));
    expect(seen, 'a partial line must not be parsed').toEqual([]);
    emitLine(line.slice(20));
    expect(seen).toEqual(['notice']);
  });

  it('handles two frames arriving in one chunk', () => {
    const { spawn, emitLine } = scriptedEngine();
    const client = new EngineClient({ command: 'x', spawn });
    const seen: string[] = [];
    client.on('notification', (n: { method: string }) => seen.push(n.method));

    const frame = (method: string) =>
      `${JSON.stringify({ jsonrpc: '2.0', trace_id: 't', method, params: sampleParams[method] })}\n`;
    emitLine(frame('notice') + frame('final'));
    expect(seen).toEqual(['notice', 'final']);
  });

  it('correlates a response with the request that asked for it', async () => {
    const { spawn, written, emitLine } = scriptedEngine();
    const client = new EngineClient({ command: 'x', spawn });

    const pending = client.request({ method: 'initialize', params: { protocol_version: 1, client: { name: 's', version: '0', cols: 80, rows: 24 } } }, 'trace-1');
    expect(written).toHaveLength(1);
    const sent = JSON.parse(written[0]!.line) as { id: number; trace_id: string };
    expect(sent.trace_id).toBe('trace-1');

    emitLine(`${JSON.stringify({ jsonrpc: '2.0', id: sent.id, trace_id: 'trace-1', result: { protocol_version: 1, engine_version: '0.1.0', capabilities: {} } })}\n`);
    // The `result` member is the payload itself; the method that produced it is known from the
    // request the caller sent, which is why the Rust enum is untagged on this side of the wire.
    await expect(pending).resolves.toMatchObject({ protocol_version: 1, engine_version: '0.1.0' });
  });

  it('rejects the promise when the engine answers with an error', async () => {
    const { spawn, written, emitLine } = scriptedEngine();
    const client = new EngineClient({ command: 'x', spawn });
    const pending = client.request({ method: 'shutdown', params: { user_initiated: true } }, 't');
    const sent = JSON.parse(written[0]!.line) as { id: number };
    emitLine(`${JSON.stringify({ jsonrpc: '2.0', id: sent.id, trace_id: 't', error: { code: -32601, message: 'nope', data: null } })}\n`);
    await expect(pending).rejects.toThrow('nope');
  });

  it('reports a line that is not JSON', () => {
    const { spawn, emitLine } = scriptedEngine();
    const client = new EngineClient({ command: 'x', spawn });
    const errors: Error[] = [];
    client.on('protocolError', (e: Error) => errors.push(e));
    emitLine('this is not json\n');
    expect(errors).toHaveLength(1);
  });

  it('rejects a payload that does not satisfy the declared schema', () => {
    const { spawn, emitLine } = scriptedEngine();
    const client = new EngineClient({ command: 'x', spawn });
    const errors: Error[] = [];
    const seen: string[] = [];
    client.on('protocolError', (e: Error) => errors.push(e));
    client.on('notification', () => seen.push('nope'));
    emitLine(`${JSON.stringify({ jsonrpc: '2.0', trace_id: 't', method: 'context.usage', params: { used: 'lots', limit: null } })}\n`);
    expect(seen).toEqual([]);
    expect(errors).toHaveLength(1);
  });
});