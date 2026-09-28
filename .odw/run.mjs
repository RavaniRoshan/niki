// Driver for open-dynamic-workflows with the kilo executor.
//
//   node .odw/run.mjs <name> '<json args>'
//
// Concurrency is deliberately NOT the runtime default (min(16, cpus-2) = 14 on
// this 16-core box). Each kilo subagent is a Node process holding a full agent
// runtime; 14 of them on a 7.5 GiB host is the OOM that killed an earlier run of
// this work. CONCURRENCY below is chosen so that peak RSS stays bounded, and
// WORKFLOW_CONCURRENCY can lower it further without editing the script.

import { runWorkflow } from '/home/shiva/.nvm/versions/node/v24.19.0/lib/node_modules/open-dynamic-workflows/dist/index.js';
import { kiloExecutor } from './kilo-executor.mjs';

const [, , name, argsJson] = process.argv;
if (!name) {
  console.error("usage: node .odw/run.mjs <workflow-name> '<json-args>'");
  process.exit(2);
}

const CONCURRENCY = Number(process.env.WORKFLOW_CONCURRENCY || 5);
const args = argsJson ? JSON.parse(argsJson) : {};
const controller = new AbortController();
for (const sig of ['SIGINT', 'SIGTERM']) {
  process.on(sig, () => {
    console.error('\n[odw] aborting — killing the agent process tree');
    controller.abort();
  });
}

const started = Date.now();
let lastLine = 0;

const result = await runWorkflow({
  name,
  args,
  cwd: process.cwd(),
  scriptPath: `/home/shiva/projects/niki/.odw/${name}/script.js`,
  runDir: `/home/shiva/projects/niki/.odw/${name}/runs`,
  executor: kiloExecutor,
  concurrency: CONCURRENCY,
  signal: controller.signal,
  agentTimeoutMs: 45 * 60 * 1000,
  onEvent: (e) => {
    if (process.env.ODW_QUIET) return;
    if (e.type === 'agent_end') {
      console.log(`  [${Math.round((Date.now() - started) / 1000)}s] ${e.label} ${e.ok ? 'ok' : 'FAILED'}`);
    } else if (e.type === 'agent_start' && Date.now() - lastLine > 5000) {
      lastLine = Date.now();
      console.log(`  … ${e.label} (${e.agentId})`);
    } else if (e.type === 'log') {
      console.log(`  · ${e.message}`);
    }
  },
});

console.log(`\n=== run ${result.runId} — ${result.agentCount} agents, ${Math.round(result.durationMs / 1000)}s ===\n`);
console.log(JSON.stringify(result.value, null, 2));
