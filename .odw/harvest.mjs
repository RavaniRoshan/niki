// Harvest completed agent results from a workflow run's journal.
//
//   node .odw/harvest.mjs [runId] [labelFilter]
//
// A workflow only returns its value when the whole script finishes. This reads
// `journal.jsonl` — which is appended the moment each agent completes — so a run
// that is stopped early, times out, or dies on one bad node still yields every
// finding that was actually produced. Run it against a live run to watch
// progress; run it after a kill to recover the work.

import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { join } from 'node:path';

const base = '/home/shiva/projects/niki/.odw';
const name = process.argv[3] || 'niki-parity';
const filter = process.argv[4] || '';
const runsDir = join(base, name, 'runs');

const runId = process.argv[2] ||
  (existsSync(runsDir)
    ? readdirSync(runsDir).filter((d) => d.startsWith('run-')).sort().pop()
    : null);

if (!runId) {
  console.error(`no runs under ${runsDir}`);
  process.exit(1);
}

const path = join(runsDir, runId, 'journal.jsonl');
if (!existsSync(path)) {
  console.error(`no journal at ${path}`);
  process.exit(1);
}

const rows = readFileSync(path, 'utf8').split('\n').filter(Boolean);
const out = [];
for (const line of rows) {
  let r;
  try { r = JSON.parse(line); } catch { continue; }
  if (r.cached) continue;
  if (filter && !String(r.label || '').includes(filter)) continue;
  out.push(r);
}

const byPhase = {};
for (const r of out) {
  const p = r.phase || 'unknown';
  (byPhase[p] ||= []).push(r);
}

console.log(`# run ${runId} — ${out.length} completed agents\n`);
for (const [phase, rs] of Object.entries(byPhase)) {
  console.log(`## ${phase} (${rs.length})`);
  for (const r of rs) {
    // The journal record carries outputTokens but not durationMs.
    const cost = r.outputTokens != null ? `${r.outputTokens}out` : '?';
    console.log(`  ${String(r.label).padEnd(28)} ${String(cost).padStart(8)}`);
  }
  console.log('');
}

if (process.env.HARVEST_JSON) {
  process.stdout.write(JSON.stringify(out.map((r) => ({ label: r.label, phase: r.phase, result: r.result })), null, 2));
} else {
  console.log('(set HARVEST_JSON=1 to dump full results as JSON)');
}
