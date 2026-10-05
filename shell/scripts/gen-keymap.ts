/**
 * KEYMAP.md is generated. Nothing in it is written by hand.
 *
 * The owner's rule for checklist row H1 is that the keymap is generated from the registry and
 * nothing is advertised that the dispatcher does not handle. Two sources, both read at run time:
 *
 *   shell/src/components/footer.tsx   COMMANDS (the registry) and HINTS (what the footer advertises)
 *   shell/src/dispatch.ts             handleKey (the only place a key is matched)
 *
 * The generator does not import those modules. It reads them as text, because a doc that is built
 * from the same objects the UI renders cannot fail on a key the UI never binds, and a test that
 * re-derives the binding set independently of the generator is the thing that catches a stale file.
 *
 * Determinism is a property, not a hope: no timestamp, no clock, no directory listing order, and
 * every list is ordered either by declaration order in the source or by first appearance in
 * `handleKey`. Running this twice must produce the same bytes, and `shell/test/keymap.test.ts`
 * runs it twice and compares.
 *
 * Usage:
 *   npx tsx shell/scripts/gen-keymap.ts            # write docs/foundation/KEYMAP.md
 *   npx tsx shell/scripts/gen-keymap.ts --stdout   # print it, write nothing
 *   npx tsx shell/scripts/gen-keymap.ts --check    # exit 1 if the committed file is stale
 */

import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const REPO = join(import.meta.dirname, '..', '..');
const FOOTER = join(REPO, 'shell', 'src', 'components', 'footer.tsx');
const DISPATCH = join(REPO, 'shell', 'src', 'dispatch.ts');
const REDUCER = join(REPO, 'shell', 'src', 'state.ts');
const OUT = join(REPO, 'docs', 'foundation', 'KEYMAP.md');

const REGENERATE = '`cd shell && npx tsx scripts/gen-keymap.ts` (or `npx tsx shell/scripts/gen-keymap.ts` from the repo root)';

// ---------------------------------------------------------------------------
// Source parsing
// ---------------------------------------------------------------------------

/**
 * Blanks comments, and blanks braces that live inside a string literal.
 *
 * The contents of strings are kept, because `'c'` and `'ctrl+c'` are exactly what this file is
 * about. Only braces are masked, so brace counting stays exact: `key.input === 'c'` is one
 * condition, and a `${...}` inside a template literal is not a nesting level.
 */
function stripNoise(src: string): string {
  let out = '';
  let i = 0;
  let block = false;
  while (i < src.length) {
    if (block) {
      if (src.startsWith('*/', i)) {
        block = false;
        i += 2;
        continue;
      }
      if (src[i] === '\n') out += '\n';
      i += 1;
      continue;
    }
    if (src.startsWith('/*', i)) {
      block = true;
      i += 2;
      continue;
    }
    if (src.startsWith('//', i)) {
      const nl = src.indexOf('\n', i);
      i = nl === -1 ? src.length : nl;
      continue;
    }
    const ch = src[i]!;
    if (ch === "'" || ch === '"' || ch === '`') {
      out += ch;
      i += 1;
      while (i < src.length && src[i] !== ch) {
        const c = src[i]!;
        if (c === '\\') {
          out += c + (src[i + 1] ?? '');
          i += 2;
          continue;
        }
        out += c === '{' || c === '}' ? ' ' : c;
        i += 1;
      }
      if (i < src.length) out += ch;
      i += 1;
      continue;
    }
    out += ch;
    i += 1;
  }
  return out;
}

/** Brace depth at the start of each line, on the stripped source. */
function depths(lines: readonly string[]): number[] {
  const out: number[] = [];
  let d = 0;
  for (const line of lines) {
    out.push(d);
    for (const ch of line) {
      if (ch === '{') d += 1;
      else if (ch === '}') d -= 1;
    }
  }
  return out;
}

export type CommandRow = {
  readonly name: string;
  readonly description: string;
  readonly aliases: readonly string[];
  readonly keywords: readonly string[];
  readonly tier: string;
};

/** The `COMMANDS` array, in declaration order. Tolerant of field order and of added fields. */
function parseCommands(footerSrc: string): CommandRow[] {
  const stripped = stripNoise(footerSrc);
  const open = stripped.indexOf('export const COMMANDS');
  if (open === -1) throw new Error('footer.tsx has no COMMANDS registry');
  const body = stripped.slice(open);
  const end = body.indexOf('];');
  if (end === -1) throw new Error('COMMANDS is not a closed array literal');
  const rows: CommandRow[] = [];
  for (const chunk of body.slice(0, end).split('{ name:').slice(1)) {
    const field = (key: string): string | readonly string[] | null => {
      const m = new RegExp(`${key}:\\s*\\[([^\\]]*)\\]`).exec(chunk);
      if (m) {
        return [...m[1]!.matchAll(/'([^']*)'/g)].map((x) => x[1]!);
      }
      const s = new RegExp(`${key}:\\s*'([^']*)'`).exec(chunk);
      return s ? s[1]! : null;
    };
    const list = (key: string): readonly string[] => {
      const v = field(key);
      return Array.isArray(v) ? v : [];
    };
    const text = (key: string): string => {
      const v = field(key);
      return typeof v === 'string' ? v : '';
    };
    const name = /^ ?'([^']+)'/.exec(chunk)?.[1];
    if (!name) throw new Error(`a COMMANDS entry has no name: ${chunk.slice(0, 60)}`);
    rows.push({
      name,
      description: text('description'),
      aliases: list('aliases'),
      keywords: list('keywords'),
      tier: text('tier') || 'unknown',
    });
  }
  if (rows.length === 0) throw new Error('COMMANDS parsed to zero rows');
  return rows;
}

export type HintRow = { readonly phase: string; readonly key: string; readonly label: string };

/** Every hint the footer can show, in declaration order. */
function parseHints(footerSrc: string): HintRow[] {
  const stripped = stripNoise(footerSrc);
  const rows: HintRow[] = [];
  const open = /^const HINTS[^\n]*= \{/m.exec(stripped);
  if (!open) throw new Error('footer.tsx has no HINTS record');
  // Take each `phase: [...]` by bracket balance rather than by a brace-excluding regex: a hint is
  // an object literal, so the array body is full of the braces a naive character class rejects.
  const body = stripped.slice(open.index);
  const re = /(\w+):\s*\[/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(body)) !== null) {
    const phase = m[1]!;
    let depth = 1;
    let i = m.index + m[0].length;
    const start = i;
    for (; i < body.length && depth > 0; i += 1) {
      if (body[i] === '[') depth += 1;
      else if (body[i] === ']') depth -= 1;
    }
    const list = body.slice(start, i - 1);
    for (const h of list.matchAll(/\{\s*key:\s*'([^']*)',\s*label:\s*'([^']*)'\s*\}/g)) {
      rows.push({ phase, key: h[1]!, label: h[2]! });
    }
    re.lastIndex = i;
  }
  // The overlay hints are a second registry in the same file, not a phase of `HINTS`, and they are
  // advertised exactly as much as the others. They are listed under the phase they actually apply
  // to: an overlay being open.
  const overlay = /^export const SEARCH_HINTS[^\n]*= \[/m.exec(stripped);
  if (overlay) {
    const end = stripped.indexOf('];', overlay.index);
    const list = stripped.slice(overlay.index, end);
    for (const h of list.matchAll(/\{\s*key:\s*'([^']*)',\s*label:\s*'([^']*)'\s*\}/g)) {
      rows.push({ phase: 'overlayOpen', key: h[1]!, label: h[2]! });
    }
  }
  if (rows.length === 0) throw new Error('HINTS parsed to zero rows');
  return rows;
}

// ---------------------------------------------------------------------------
// dispatch.ts
// ---------------------------------------------------------------------------

const FLAG_BINDINGS: Record<string, string> = {
  escape: 'esc',
  return: 'enter',
  backspace: 'backspace',
  delete: 'delete',
  upArrow: 'up',
  downArrow: 'down',
  leftArrow: 'left',
  rightArrow: 'right',
  pageUp: 'pgup',
  pageDown: 'pgdn',
  home: 'home',
  end: 'end',
  tab: 'tab',
  paste: 'paste',
};

export type Binding = {
  readonly key: string;
  readonly effects: readonly string[];
  readonly scopes: readonly string[];
  readonly conditions: readonly string[];
};

/** The bindings `handleKey` matches, in first-appearance order, with the action each one returns. */
export function parseBindings(dispatchSrc: string): Binding[] {
  const lines = stripNoise(dispatchSrc).split('\n');
  const depth = depths(lines);
  const approvalAt = lines.findIndex((l) => l.trimStart().startsWith('if (state.approval)'));
  const approvalDepth = approvalAt === -1 ? -1 : depth[approvalAt]!;
  const order: string[] = [];
  const byKey = new Map<string, { effects: Set<string>; scopes: Set<string>; conditions: Set<string> }>();

  const add = (key: string, condition: string, block: readonly string[], inApproval: boolean): void => {
    const cond = condition.trim();
    let entry = byKey.get(key);
    if (!entry) {
      entry = { effects: new Set(), scopes: new Set(), conditions: new Set() };
      byKey.set(key, entry);
      order.push(key);
    }
    entry.conditions.add(cond);
    entry.scopes.add(inApproval ? 'approval prompt' : 'composer');
    for (const kind of block.join(' ').matchAll(/kind:\s*'([^']+)'/g)) entry.effects.add(`action: ${kind[1]}`);
    for (const s of block.join(' ').matchAll(/scroll:\s*'([^']+)'/g)) entry.effects.add(`scroll: ${s[1]}`);
    if (/\bscroll:/.test(block.join(' ')) && !/scroll:\s*'/.test(block.join(' '))) {
      // A scroll target computed by a ternary, e.g. `scroll: key.pageUp ? 'pageUp' : 'home'`. Naming
      // one of the branches would be a claim the code does not make.
      entry.effects.add('scroll: the target chosen in handleKey');
    }
    if (/insert:\s*key\.input/.test(block.join(' '))) entry.effects.add('insert the key as literal text');
    for (const ins of block.join(' ').matchAll(/insert:\s*'([^']*)'/g)) {
      // The source writes the two characters \ and n; both spellings are accepted so the label
      // does not depend on how the escape was written in the file.
      entry.effects.add(
        ins[1] === '\n' || ins[1] === '\\n' ? 'insert a newline' : `insert ${JSON.stringify(ins[1]!)}`,
      );
    }
  };

  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i]!;
    if (!line.trimStart().startsWith('if (')) continue;
    const cond = line.slice(line.indexOf('if (') + 3);
    if (!cond.includes('key.')) continue;

    // The block: every following line nested at least one brace deeper. `>=`, not `===`, because
    // a nested `if` pushes its own return one level further in, and those returns are this
    // binding's effects too. The condition line itself carries a single-line `if (...) return {...}`.
    const block: string[] = [cond];
    for (let j = i + 1; j < lines.length && depth[j]! >= depth[i]! + 1; j += 1) block.push(lines[j]!);
    const inApproval = approvalDepth !== -1 && depth[i]! > approvalDepth;

    const conditions: string[] = [];
    for (const m of cond.matchAll(/key\.ctrl\s*&&\s*key\.input\s*===\s*'([^']+)'/g)) {
      conditions.push(`ctrl+${m[1]!.toLowerCase()}`);
    }
    for (const m of cond.matchAll(/key\.input\s*===\s*'([^']+)'\s*&&\s*!key\.ctrl/g)) {
      conditions.push(m[1]!);
    }
    if (/key\.shift\s*&&\s*key\.tab/.test(cond)) conditions.push('shift+tab');
    if (/key\.shift\s*\|\|\s*key\.meta\s*\)\s*&&\s*key\.return/.test(cond)) {
      conditions.push('shift+enter', 'alt+enter');
    }
    for (const [flag, name] of Object.entries(FLAG_BINDINGS)) {
      // A negated flag is not a binding: `!key.escape` is the "ordinary character" catch-all.
      if (new RegExp(`key\\.${flag}\\b`).test(cond) && !new RegExp(`!\\s*key\\.${flag}\\b`).test(cond)) {
        conditions.push(name);
      }
    }
    // Only Shift+Tab is matched; a bare Tab is not, so it must not be advertised as one.
    const unique = conditions.filter((k, idx) => conditions.indexOf(k) === idx);
    const kept = unique.includes('shift+tab') ? unique.filter((k) => k !== 'tab') : unique;

    for (const key of kept) add(key, cond, block, inApproval);
  }

  return order.map((key) => {
    const e = byKey.get(key)!;
    return {
      key,
      effects: [...e.effects].sort(),
      scopes: [...e.scopes].sort(),
      conditions: [...e.conditions],
    };
  });
}

// ---------------------------------------------------------------------------
// Hints that are not bindings
// ---------------------------------------------------------------------------

/** Hint spellings that name a real key, mapped to the binding names the dispatcher would use. */
const HINT_KEY_ALIASES: Record<string, string[]> = {
  '↑↓': ['up', 'down'],
  '↑': ['up'],
  '↓': ['down'],
  pgup: ['pgup'],
  pgdn: ['pgdn'],
  pgupdn: ['pgup', 'pgdn'],
};

export type Unbound = { readonly key: string; readonly labels: readonly string[]; readonly why: string };

/**
 * Every hint the footer advertises that is not a key `handleKey` matches.
 *
 * This section is the reason the file is generated rather than written: a hint is a promise made
 * by the footer, and the only way to know the promise is kept is to compare the footer to the
 * dispatcher on every run.
 */
function unadvertised(bindings: readonly Binding[], hints: readonly HintRow[], reducerSrc: string): Unbound[] {
  const known = new Set(bindings.map((b) => b.key));
  const byKey = new Map<string, string[]>();
  for (const h of hints) {
    if (h.key === '') continue;
    const list = byKey.get(h.key) ?? [];
    list.push(h.label);
    byKey.set(h.key, list);
  }
  const out: Unbound[] = [];
  for (const [key, labels] of byKey) {
    const parts = HINT_KEY_ALIASES[key] ?? [key];
    if (parts.every((p) => known.has(p))) continue;
    if (/^[a-z?/]$/.test(key)) {
      const opensMenu = key === '/' && /startsWith\('\/'\)/.test(stripNoise(reducerSrc));
      out.push({
        key,
        labels: [...new Set(labels)].sort(),
        why: opensMenu
          ? 'an ordinary character. The slash menu opens in the reducer, from the composer text, not from a key binding.'
          : 'an ordinary character. Nothing in `handleKey` treats it as a binding, so it is inserted as text.',
      });
    } else if (key.length > 3 || key.includes(' ')) {
      out.push({ key, labels: [...new Set(labels)].sort(), why: 'prose in the hint, not a key.' });
    } else {
      out.push({
        key,
        labels: [...new Set(labels)].sort(),
        why: 'spelled by the footer as a key that `handleKey` does not match.',
      });
    }
  }
  return out.sort((a, b) => (a.key < b.key ? -1 : a.key > b.key ? 1 : 0));
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

const BEGIN = '<!-- keymap:begin -->';
const END = '<!-- keymap:end -->';

function cell(text: string): string {
  return text.replace(/\|/g, '\\|').replace(/\n/g, ' ');
}

function table(headers: readonly string[], rows: readonly (readonly string[])[]): string {
  const head = `| ${headers.join(' | ')} |`;
  const rule = `| ${headers.map(() => '---').join(' | ')} |`;
  const body = rows.map((r) => `| ${r.map(cell).join(' | ')} |`).join('\n');
  return [head, rule, body].join('\n');
}

export function render(
  commands: readonly CommandRow[],
  bindings: readonly Binding[],
  hints: readonly HintRow[],
  unbound: readonly Unbound[],
  srcs: { dispatch: string; reducer: string },
): string {
  const phases = [...new Set(hints.map((h) => h.phase))];
  const helpBound = /key\.input\s*===\s*'\?'/.test(stripNoise(srcs.dispatch));
  const helpOpens = /overlay:\s*'help'/.test(stripNoise(srcs.reducer));

  const bindingRows = bindings.map((b) => [
    `\`${b.key}\``,
    b.effects.length > 0 ? b.effects.join(' → ') : 'handled; no state change',
    b.scopes.join(', '),
    b.conditions.map((c) => `\`${c}\``).join('<br>'),
  ]);

  const commandRows = commands.map((c) => [
    `\`${c.name}\``,
    c.aliases.length > 0 ? c.aliases.map((a) => `\`${a}\``).join(' ') : '—',
    `\`${c.tier}\``,
    c.description,
    c.keywords.length > 0 ? c.keywords.map((k) => `\`${k}\``).join(' ') : '—',
  ]);

  const hintRows = hints.map((h) => [`\`${h.phase}\``, h.key === '' ? '—' : `\`${h.key}\``, h.label]);

  const unboundRows = unbound.map((u) => [`\`${u.key}\``, u.labels.map((l) => `\`${l}\``).join(' '), u.why]);

  const lines: string[] = [
    '<!-- GENERATED FILE. DO NOT EDIT BY HAND. -->',
    `<!-- Regenerate: ${REGENERATE} -->`,
    '<!-- Sources: shell/src/components/footer.tsx (COMMANDS, HINTS) and shell/src/dispatch.ts (handleKey). -->',
    '<!-- shell/test/keymap.test.ts fails if this file is stale, names a key handleKey does not, -->',
    '<!-- or names a command that is not in COMMANDS. -->',
    '',
    '# NIKI Shell — KEYMAP',
    '',
    `Generated from the registry. ${bindings.length} key bindings, ${commands.length} commands,`,
    `${hints.length} contextual hints, ${unbound.length} advertised hints that are not bindings.`,
    '',
    "The rule this file exists to hold is the owner's: nothing is advertised that the dispatcher",
    'does not handle. The tables below are read out of the two sources rather than transcribed,',
    'so a key added to the shell appears here on the next generator run and a key removed from the',
    'shell disappears here rather than lingering as a promise.',
    '',
    BEGIN,
    '',
    '## Key bindings',
    '',
    'Every key `handleKey` matches, in the order the dispatcher tests it. `Where` says whether the',
    'binding is live while an approval prompt owns the keyboard or in the ordinary composer.',
    '',
    table(['Key', 'Effect', 'Where', 'Condition in handleKey'], bindingRows),
    '',
    '## Commands',
    '',
    'The `COMMANDS` registry, in declaration order. `tier` decides whether the command is usable',
    'while a run is in flight: `always`, `immediateUi`, `sideEffectFree` and `queued`.',
    '',
    table(['Command', 'Aliases', 'Tier', 'Description', 'Keywords'], commandRows),
    '',
    '## Footer hints',
    '',
    'What the footer shows per phase, from `HINTS` and `SEARCH_HINTS` in the same file. These are',
    'advertised keys, not necessarily bindings — the section below is the difference between the two.',
    '',
    table(['Phase', 'Hint key', 'Label'], hintRows),
    '',
    '## Advertised hints that are not key bindings',
    '',
    unbound.length === 0
      ? 'None. Every hint key the footer can show is matched by `handleKey`.'
      : [
          'Each of these is shown by the footer and matched by no key in `handleKey`.',
          '',
          table(['Hint key', 'Labels', 'What actually happens'], unboundRows),
        ].join('\n'),
    '',
    helpBound
      ? '`?` is bound in `dispatch.ts`, so a keypress opens the overlay the reducer names.'
      : helpOpens
        ? '`?` is not bound in `dispatch.ts` today. The reducer can hold `overlay: \'help\'`, but no key reaches it.'
        : '`?` is not bound in `dispatch.ts` today, and no reducer arm opens the `help` overlay.',
    '',
    `Phases with hints: ${phases.map((p) => `\`${p}\``).join(' · ')}.`,
    '',
    END,
    '',
  ];
  return lines.join('\n');
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

function main(): number {
  const footerSrc = readFileSync(FOOTER, 'utf8');
  const dispatchSrc = readFileSync(DISPATCH, 'utf8');
  const reducerSrc = readFileSync(REDUCER, 'utf8');
  const commands = parseCommands(footerSrc);
  const bindings = parseBindings(dispatchSrc);
  const hints = parseHints(footerSrc);
  const unbound = unadvertised(bindings, hints, reducerSrc);
  const out = render(commands, bindings, hints, unbound, { dispatch: dispatchSrc, reducer: reducerSrc });

  if (process.argv.includes('--stdout')) {
    process.stdout.write(out);
    return 0;
  }
  if (process.argv.includes('--check')) {
    let current = '';
    try {
      current = readFileSync(OUT, 'utf8');
    } catch {
      process.stderr.write(`KEYMAP.md is missing: ${OUT}\n`);
      return 1;
    }
    if (current !== out) {
      process.stderr.write(
        `KEYMAP.md is stale. Regenerate with: npx tsx shell/scripts/gen-keymap.ts\n${OUT}\n`,
      );
      return 1;
    }
    return 0;
  }
  writeFileSync(OUT, out);
  process.stdout.write(
    `wrote ${OUT} — ${bindings.length} bindings, ${commands.length} commands, ${unbound.length} unbound hints\n`,
  );
  return 0;
}

process.exit(main());
