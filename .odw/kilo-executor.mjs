// kilo-backed Executor for open-dynamic-workflows.
//
// The published `workflow` CLI hardwires `claudeExecutor` and spawns `claude`,
// which is not installed on this machine. This adapter speaks the same ExecResult
// contract on top of `kilo run --format json`, so the published runtime's
// concurrency, journal, resume, and schema-validation machinery all work unchanged.
//
//   kilo --format json  ->  {"type":"text","part":{"text":...}}
//                           {"type":"step_finish","part":{"cost":..,"tokens":{..},"reason":..}}
//
// Schema mode has no native structured-output flag, so the schema is injected into
// the prompt, the JSON is extracted from the reply, validated with ajv, and — on a
// validation failure — repaired with ONE extra call. The runtime throws when a
// schema'd agent returns no valid `structuredOutput`, and `parallel()` swallows
// that into `null`, i.e. silent data loss. The repair pass is what makes schema
// mode safe to use at this fan-out.

import { spawn } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
import { dirname } from 'node:path';
import { createRequire } from 'node:module';

// ajv ships with open-dynamic-workflows; borrow it rather than adding a dep.
const require = createRequire(import.meta.url);
const ODW_ROOT = '/home/shiva/.nvm/versions/node/v24.19.0/lib/node_modules/open-dynamic-workflows';
const Ajv = require(`${ODW_ROOT}/node_modules/ajv`);

const DEFAULT_MODEL = 'kilo/stealth/space-bunny-alpha';
const DEFAULT_TIMEOUT_MS = 45 * 60 * 1000;
const IDLE_TIMEOUT_MS = 5 * 60 * 1000;

/** Extract the first complete top-level JSON object from noisy model output. */
export function extractJson(text) {
  const fenced = text.match(/```(?:json)?\s*([\s\S]*?)```/);
  const candidates = [];
  if (fenced) candidates.push(fenced[1]);
  candidates.push(text);
  for (const c of candidates) {
    const start = c.indexOf('{');
    if (start === -1) continue;
    // Walk forward tracking brace depth outside of string literals.
    let depth = 0, inStr = false, esc = false;
    for (let i = start; i < c.length; i++) {
      const ch = c[i];
      if (esc) { esc = false; continue; }
      if (ch === '\\') { esc = true; continue; }
      if (ch === '"') { inStr = !inStr; continue; }
      if (inStr) continue;
      if (ch === '{') depth++;
      else if (ch === '}') {
        depth--;
        if (depth === 0) {
          try { return JSON.parse(c.slice(start, i + 1)); } catch { break; }
        }
      }
    }
  }
  return null;
}

function runKilo({ prompt, cwd, model, signal, timeoutMs, idleTimeoutMs }) {
  return new Promise((resolve, reject) => {
    const startedAt = Date.now();
    const args = ['run', '--format', 'json', '--auto', '--pure'];
    if (model) args.push('-m', model);
    args.push(prompt);

    const child = spawn('kilo', args, {
      cwd,
      env: { ...process.env, GIT_TERMINAL_PROMPT: '0' },
      stdio: ['ignore', 'pipe', 'pipe'],
      // Own process group, so an abort kills kilo and anything it spawned.
      detached: true,
    });

    const killTree = (sig) => {
      try { if (child.pid !== undefined) process.kill(-child.pid, sig); }
      catch { try { child.kill(sig); } catch { /* gone */ } }
    };

    let settled = false;
    let wallTimer, idleTimer;
    const clearTimers = () => { if (wallTimer) clearTimeout(wallTimer); if (idleTimer) clearTimeout(idleTimer); };
    const fail = (msg) => {
      if (settled) return;
      settled = true; clearTimers();
      signal?.removeEventListener('abort', onAbort);
      killTree('SIGKILL');
      reject(new Error(msg));
    };
    const onAbort = () => fail('kilo aborted');
    if (signal) {
      if (signal.aborted) { killTree('SIGKILL'); reject(new Error('kilo aborted')); return; }
      signal.addEventListener('abort', onAbort, { once: true });
    }
    wallTimer = setTimeout(() => fail('kilo timeout'), timeoutMs);
    const armIdle = () => {
      if (idleTimer) clearTimeout(idleTimer);
      idleTimer = setTimeout(() => fail('kilo idle timeout'), idleTimeoutMs);
    };
    armIdle();

    const events = [];
    let buf = '';
    let stderrBuf = '';
    const consume = (chunk) => {
      buf += chunk;
      let nl;
      while ((nl = buf.indexOf('\n')) !== -1) {
        const line = buf.slice(0, nl); buf = buf.slice(nl + 1);
        if (!line.trim()) continue;
        try { events.push(JSON.parse(line)); } catch { /* non-JSON noise */ }
      }
    };

    child.stdout.setEncoding('utf8');
    child.stdout.on('data', (c) => { armIdle(); consume(c); });
    child.stderr.setEncoding('utf8');
    child.stderr.on('data', (c) => { stderrBuf += c; });
    child.on('error', (e) => fail(`kilo spawn error: ${e.message}`));
    child.on('close', (code) => {
      if (settled) return;
      settled = true; clearTimers();
      signal?.removeEventListener('abort', onAbort);
      if (buf.trim()) { try { events.push(JSON.parse(buf)); } catch { /* ignore */ } }

      const texts = events.filter((e) => e.type === 'text' && e.part?.text).map((e) => e.part.text);
      const fin = [...events].reverse().find((e) => e.type === 'step_finish');
      const text = texts.join('');
      const isError = code !== 0 || fin?.part?.reason === 'error';
      resolve({
        text: text || (isError ? stderrBuf.trim() : ''),
        sessionId: events.find((e) => e.sessionID)?.sessionID,
        costUsd: fin?.part?.cost ?? 0,
        durationMs: Date.now() - startedAt,
        resultSubtype: isError ? (fin?.part?.reason ?? `exit_${code}`) : 'success',
        isError,
        usage: {
          inputTokens: fin?.part?.tokens?.input ?? 0,
          outputTokens: fin?.part?.tokens?.output ?? 0,
        },
        events,
      });
    });
  });
}

async function writeTrace(path, prompt, events) {
  try {
    await mkdir(dirname(path), { recursive: true });
    await writeFile(path, JSON.stringify([{ type: 'user_input', text: prompt }, ...events], null, 2));
  } catch { /* debug artifact only */ }
}

export const kiloExecutor = (opts) => {
  const timeoutMs = opts.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const idleTimeoutMs = opts.idleTimeoutMs ?? IDLE_TIMEOUT_MS;
  const model = opts.model ?? DEFAULT_MODEL;

  return (async () => {
    const schemaNote = opts.schema
      ? `\n\n## OUTPUT FORMAT — MANDATORY\n\nYour ENTIRE reply must be one JSON object and nothing else. No prose before it, no prose after it, no markdown fence.\nIt must validate against this JSON Schema:\n\n\`\`\`json\n${JSON.stringify(opts.schema)}\n\`\`\`\n`
      : '';
    const systemNote = opts.appendSystemPrompt
      ? `\n\n<system-prompt>${opts.appendSystemPrompt}</system-prompt>\n`
      : '';

    const first = await runKilo({
      prompt: systemNote + opts.prompt + schemaNote,
      cwd: opts.cwd, model, signal: opts.signal, timeoutMs, idleTimeoutMs,
    });

    if (!opts.schema) {
      if (opts.tracePath) await writeTrace(opts.tracePath, opts.prompt, first.events);
      return first;
    }

    const ajv = new Ajv({ allErrors: true, strict: false });
    const validate = ajv.compile(opts.schema);
    let parsed = extractJson(first.text);
    if (parsed && validate(parsed)) {
      if (opts.tracePath) await writeTrace(opts.tracePath, opts.prompt, first.events);
      return { ...first, structuredOutput: parsed };
    }

    // One repair call. The runtime throws (and parallel() nulls the node) when
    // structuredOutput is missing, so this pass is what keeps the run lossless.
    const errors = parsed ? JSON.stringify(validate.errors).slice(0, 1200) : 'no parsable JSON object was found in the reply';
    const repair = await runKilo({
      prompt:
        `Your previous reply did not satisfy the required schema.\n\nVALIDATION ERRORS:\n${errors}\n\n` +
        `Return the corrected JSON object and NOTHING else — no prose, no fence.\n\n` +
        `SCHEMA:\n\`\`\`json\n${JSON.stringify(opts.schema)}\n\`\`\``,
      cwd: opts.cwd, model, signal: opts.signal, timeoutMs, idleTimeoutMs,
    });
    const repaired = extractJson(repair.text);
    if (opts.tracePath) {
      await writeTrace(opts.tracePath, opts.prompt, [...first.events, ...repair.events]);
    }
    if (repaired && validate(repaired)) {
      return {
        ...first,
        text: repair.text,
        structuredOutput: repaired,
        durationMs: first.durationMs + repair.durationMs,
      };
    }
    // Leave structuredOutput undefined: hooks.js will throw, which parallel()
    // turns into a null this script can see and report rather than lose silently.
    return { ...first, text: `${first.text}\n\n[schema repair failed] ${errors}` };
  })();
};
