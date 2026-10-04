/**
 * The round-trip latency probe for checklist row A4.
 *
 * Spawns the **real** `niki serve` binary, sends `initialize` over a real pipe, and measures the
 * time from writing a line to reading its reply. Reported as p50/p95/p99, because a mean over a
 * process pipe hides exactly the tail a user feels.
 *
 * This is a probe, not a threshold: it prints numbers so a later change has something to compare
 * against. It asserts only that the engine answers, because a machine-dependent assertion about
 * latency is a test that fails on a busy laptop and teaches nobody anything.
 */

import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

const REPO = join(import.meta.dirname, '..', '..');
const BIN = join(REPO, 'target', 'debug', 'niki');
const SAMPLES = 200;

type Sample = { id: number; ms: number };

function percentile(values: number[], p: number): number {
  const sorted = [...values].sort((a, b) => a - b);
  const index = Math.min(sorted.length - 1, Math.ceil((p / 100) * sorted.length) - 1);
  return sorted[Math.max(0, index)]!;
}

function roundTrips(samples: number): Promise<Sample[]> {
  return new Promise((resolve, reject) => {
    const child = spawn(BIN, ['serve'], {
      stdio: ['pipe', 'pipe', 'pipe'],
      env: { ...process.env, NO_COLOR: '1' },
    });

    const timer = setTimeout(() => {
      child.kill('SIGKILL');
      reject(new Error(`round-trip probe timed out after ${samples} requests`));
    }, 60_000);

    const results: Sample[] = [];
    const pending = new Map<number, (s: Sample) => void>();
    let buffer = '';

    child.stdout.setEncoding('utf8');
    child.stdout.on('data', (chunk: string) => {
      buffer += chunk;
      let nl = buffer.indexOf('\n');
      while (nl !== -1) {
        const line = buffer.slice(0, nl);
        buffer = buffer.slice(nl + 1);
        if (line.trim()) {
          const frame = JSON.parse(line) as { id?: number };
          if (typeof frame.id === 'number') {
            const resolve1 = pending.get(frame.id);
            if (resolve1) {
              pending.delete(frame.id);
              resolve1({ id: frame.id, ms: 0 });
            }
          }
        }
        nl = buffer.indexOf('\n');
      }
    });
    child.on('error', reject);

    void (async () => {
      for (let id = 1; id <= samples; id += 1) {
        const started = performance.now();
        const reply = new Promise<Sample>((r) => pending.set(id, r));
        child.stdin.write(
          `${JSON.stringify({
            jsonrpc: '2.0',
            id,
            trace_id: `probe-${id}`,
            method: 'initialize',
            params: {
              protocol_version: 1,
              client: { name: 'latency-probe', version: '0.1.0', cols: 80, rows: 24 },
            },
          })}\n`,
        );
        await reply;
        results.push({ id, ms: performance.now() - started });
      }
      clearTimeout(timer);
      child.kill('SIGTERM');
      resolve(results);
    })().catch((e: unknown) => {
      clearTimeout(timer);
      child.kill('SIGKILL');
      reject(e);
    });
  });
}

describe('A4: round-trip latency over the real binary', () => {
  it('answers every initialize, and records p50/p95/p99', async () => {
    expect(
      existsSync(BIN),
      `${BIN} does not exist. Run: cargo build -j 2`,
    ).toBe(true);

    const samples = await roundTrips(SAMPLES);
    expect(samples).toHaveLength(SAMPLES);

    const values = samples.map((s) => s.ms);
    const p50 = percentile(values, 50);
    const p95 = percentile(values, 95);
    const p99 = percentile(values, 99);
    console.log(
      `round trip over the real niki serve (${SAMPLES} initialize requests): ` +
        `p50 ${p50.toFixed(2)}ms  p95 ${p95.toFixed(2)}ms  p99 ${p99.toFixed(2)}ms  ` +
        `max ${Math.max(...values).toFixed(2)}ms`,
    );
    // The recorded budget for this machine. A regression worth investigating shows up here as a
    // number several times these values, not as a hair over an arbitrary line.
    expect(p95).toBeLessThan(1000);
  });
});