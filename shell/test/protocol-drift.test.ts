/**
 * The two-way drift test.
 *
 * Rust is the single source of truth for the message set. This file fails if:
 *  - the TypeScript in `src/protocol/generated/` differs from what the Rust crate exports, or
 *  - the Rust crate declares a message the shell has no validator and no handler for.
 *
 * The second direction is the one that actually bites: adding a variant to `ServerNotification`
 * without teaching the shell about it must fail the build, not sit there until a user hits it.
 */

import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import { HANDLED_METHODS, RESULT_SCHEMAS } from '../src/protocol/schemas.js';
import { reduce, initialState, type ReduceOptions } from '../src/state.js';

const REPO = join(import.meta.dirname, '..', '..');
const RUST_MESSAGES = join(REPO, 'crates', 'niki-protocol', 'src', 'messages.rs');
const RUST_BINDINGS = join(REPO, 'crates', 'niki-protocol', 'bindings');
const TS_GENERATED = join(import.meta.dirname, '..', 'src', 'protocol', 'generated');

function rustEnumVariants(enumName: string): string[] {
  const src = readFileSync(RUST_MESSAGES, 'utf8');
  const start = src.indexOf(`pub enum ${enumName}`);
  if (start === -1) throw new Error(`${enumName} is not declared in messages.rs`);
  const open = src.indexOf('{', start);
  let depth = 0;
  let end = open;
  for (let i = open; i < src.length; i += 1) {
    if (src[i] === '{') depth += 1;
    if (src[i] === '}') {
      depth -= 1;
      if (depth === 0) {
        end = i;
        break;
      }
    }
  }
  const body = src.slice(open, end);
  const names = [...body.matchAll(/#\[serde\(rename = "([^"]+)"\)\]\s*([A-Za-z0-9_]+)/g)].map(
    (m) => m[1]!,
  );
  return names;
}

describe('protocol drift: Rust is the source of truth', () => {
  it('declares the same notifications in Rust and in the TypeScript validators', () => {
    const rust = rustEnumVariants('ServerNotification').sort();
    const ts = [...HANDLED_METHODS].sort();
    expect(ts, 'the shell must handle every message Rust declares, and no others').toEqual(rust);
  });

  it('declares the same requests in Rust and in the TypeScript result validators', () => {
    const rust = rustEnumVariants('ClientRequest').sort();
    const ts = Object.keys(RESULT_SCHEMAS).sort();
    expect(ts).toEqual(rust);
  });

  it('has a committed TypeScript binding for every file the Rust crate exports', () => {
    const rustFiles = readdirSync(RUST_BINDINGS)
      .filter((f) => f.endsWith('.ts'))
      .sort();
    const tsFiles = readdirSync(TS_GENERATED)
      .filter((f) => f.endsWith('.ts') && f !== 'index.ts')
      .sort();
    expect(tsFiles).toEqual(rustFiles);
  });

  it('has byte-identical TypeScript bindings on both sides', () => {
    const files = readdirSync(RUST_BINDINGS).filter((f) => f.endsWith('.ts'));
    const drifted = files.filter((f) => {
      const a = readFileSync(join(RUST_BINDINGS, f), 'utf8');
      const b = readFileSync(join(TS_GENERATED, f), 'utf8');
      return a !== b;
    });
    expect(
      drifted,
      `stale bindings: ${drifted.join(', ')}. Run cargo run -p niki-protocol --bin gen-protocol-bindings and re-copy.`,
    ).toEqual([]);
  });
});

describe('every declared message reduces into real state', () => {
  const sampleParams: Record<string, unknown> = {
    'session.ready': {
      session_id: 's1',
      project_path: '/tmp/p',
      model: 'm',
      permission_mode: 'manual',
      branch: 'main',
      ahead: null,
      behind: null,
      resumed_messages: 0,
    },
    'turn.started': { turn_id: 't1', prompt: 'do the thing' },
    'turn.delta': { turn_id: 't1', text: 'working on it' },
    'turn.end': {
      turn_id: 't1',
      summary: 'done',
      duration_ms: 42000,
      tool_calls: 3,
      files_changed: 1,
    },
    'stage.start': { stage_id: 'g1', role: 'planner', attempt: 1 },
    'stage.token': { stage_id: 'g1', role: 'planner', text: 'considering the tests' },
    'stage.done': {
      stage_id: 'g1',
      role: 'planner',
      summary: 'planned',
      tokens_in: 10,
      tokens_out: 20,
      cost_usd: 0.01,
      latency_ms: 1200,
      retry_count: 0,
      artifact_ref: 'a1',
      provenance: 'independent',
    },
    'stage.failed': { stage_id: 'g2', role: 'coder', error: 'boom', severity: 'error', recovery: null },
    'tool.call': { tool_id: 'x1', name: 'read', args: 'package.json' },
    'tool.progress': { tool_id: 'x1', note: 'scanning' },
    'tool.result': { tool_id: 'x1', ok: true, summary: 'read 46 lines', full_ref: null, duration_ms: 12 },
    'tool.diff': { tool_id: 'x1', path: 'src/lib.rs', hunks: [] },
    'approval.request': {
      id: 'a1',
      tool: 'bash',
      command: 'npm test',
      options: [
        { id: 'allow', label: 'Allow' },
        { id: 'deny', label: 'Deny' },
      ],
      safest_option_id: 'deny',
    },
    'plan.update': { items: [{ text: 'read the code', done: false }] },
    notice: { text: 'a note', level: 'info' },
    'diff.ready': { ref: 'd1' },
    'verdict.ready': { ref: 'v1', verdict: 'approved', provenance: 'independent' },
    'branch.created': { name: 'niki/42' },
    'context.usage': { used: 31000, limit: 262000 },
    'cost.update': { usd: 0.42 },
    final: { verdict: 'approved', error: null },
  };

  it('accepts every declared message and produces no protocol error', () => {
    const opts: ReduceOptions = { nowMs: 1000 };
    for (const method of HANDLED_METHODS) {
      const params = sampleParams[method];
      expect(params, `no sample for ${method}`).toBeDefined();
      const before = initialState(80, 24);
      const after = reduce(before, { method, params } as never, opts);
      expect(after.protocolError, `${method} produced a protocol error`).toBeNull();
    }
  });
});