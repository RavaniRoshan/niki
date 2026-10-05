/**
 * KEYMAP.md is generated, and this file is the reason anyone can trust that.
 *
 * Three assertions, in increasing order of how much they would cost to get wrong:
 *
 *  1. Every key the document lists is a key `handleKey` matches. The binding set here is derived
 *     from `dispatch.ts` by a second, deliberately cruder extractor than the generator's, so a bug
 *     in the generator's parser cannot hide a key that the dispatcher does not handle.
 *  2. Every command the document lists is in the `COMMANDS` registry, and every registry command is
 *     listed. A document that quietly drops `/yolo` is as wrong as one that invents `/nuke`.
 *  3. Running the generator twice produces byte-identical output, and that output is the committed
 *     file. A document nobody regenerates is a document that is wrong within a release.
 *
 * The extractor below is intentionally simpler than the generator's: it matches `key.<flag>` and
 * `key.input === '<char>'` shapes anywhere in `handleKey`, with no block or scope analysis. Being
 * wrong in that direction is safe — a superset can only make assertion 1 stricter.
 */

import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

const SHELL = join(import.meta.dirname, '..');
const REPO = join(SHELL, '..');
const KEYMAP = join(REPO, 'docs', 'foundation', 'KEYMAP.md');
const DISPATCH = join(SHELL, 'src', 'dispatch.ts');
const FOOTER = join(SHELL, 'src', 'components', 'footer.tsx');
const GENERATOR = join(SHELL, 'scripts', 'gen-keymap.ts');
const TSX = join(SHELL, 'node_modules', '.bin', 'tsx');

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

function commentsOut(src: string): string {
  return src.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/\/\/[^\n]*/g, ' ');
}

/** Every binding name that appears anywhere in a `key.` condition in `dispatch.ts`. */
function bindingsInDispatch(): Set<string> {
  const src = commentsOut(readFileSync(DISPATCH, 'utf8'));
  const found = new Set<string>();
  for (const line of src.split('\n')) {
    if (!line.includes('key.')) continue;
    for (const m of line.matchAll(/key\.ctrl\s*&&\s*key\.input\s*===\s*'([^']+)'/g)) {
      found.add(`ctrl+${m[1]!.toLowerCase()}`);
    }
    for (const m of line.matchAll(/key\.input\s*===\s*'([^']+)'\s*&&\s*!key\.ctrl/g)) {
      found.add(m[1]!);
    }
    if (/key\.shift\s*&&\s*key\.tab/.test(line)) found.add('shift+tab');
    if (/key\.shift\s*\|\|\s*key\.meta\s*\)\s*&&\s*key\.return/.test(line)) {
      found.add('shift+enter');
      found.add('alt+enter');
    }
    for (const [flag, name] of Object.entries(FLAG_BINDINGS)) {
      if (new RegExp(`key\\.${flag}\\b`).test(line) && !new RegExp(`!\\s*key\\.${flag}\\b`).test(line)) {
        found.add(name);
      }
    }
  }
  return found;
}

/** Every command name in the `COMMANDS` registry, with its aliases. */
function commandsInRegistry(): { names: Set<string>; aliases: Set<string> } {
  const src = commentsOut(readFileSync(FOOTER, 'utf8'));
  const start = src.indexOf('export const COMMANDS');
  expect(start, 'footer.tsx must still export COMMANDS').toBeGreaterThan(-1);
  const body = src.slice(start, src.indexOf('];', start));
  const names = new Set<string>();
  const aliases = new Set<string>();
  for (const chunk of body.split('{ name:').slice(1)) {
    const name = /^ ?'([^']+)'/.exec(chunk)?.[1];
    expect(name, `a COMMANDS entry lost its name: ${chunk.slice(0, 60)}`).toBeTruthy();
    names.add(name!);
    for (const alias of chunk.match(/aliases: \[([^\]]*)\]/)?.[1]?.matchAll(/'([^']+)'/g) ?? []) {
      aliases.add(alias[1]!);
    }
  }
  return { names, aliases };
}

/** Rows of the markdown table under `heading`, as cells with backticks stripped. */
function tableUnder(markdown: string, heading: string): string[][] {
  const lines = markdown.split('\n');
  const at = lines.indexOf(heading);
  expect(at, `KEYMAP.md must contain the section ${heading}`).toBeGreaterThan(-1);
  const rows: string[][] = [];
  let seenRule = false;
  for (const line of lines.slice(at + 1)) {
    if (/^\|\s*-+(\s*\|\s*-+)*\s*\|\s*$/.test(line)) {
      // The rule line ends the header; everything after it is data.
      seenRule = true;
      continue;
    }
    if (!line.startsWith('|')) {
      // The prose between a heading and its table is not the end of the section; the first
      // non-table line after the table has started is.
      if (seenRule) break;
      continue;
    }
    if (!seenRule) continue;
    rows.push(
      line
        .replace(/^\|\s*/, '')
        .replace(/\s*\|\s*$/, '')
        .split(' | ')
        .map((c) => c.trim().replace(/^`|`$/g, '').replace(/\\\|/g, '|')),
    );
  }
  return rows;
}

function runGenerator(args: readonly string[]): string {
  return execFileSync(TSX, [GENERATOR, ...args], { encoding: 'utf8', cwd: SHELL });
}

describe('KEYMAP.md is generated from the registry', () => {
  it('lists no key binding that handleKey does not match', () => {
    expect(existsSync(KEYMAP), 'docs/foundation/KEYMAP.md must exist').toBe(true);
    const markdown = readFileSync(KEYMAP, 'utf8');
    const advertised = tableUnder(markdown, '## Key bindings').map((row) => row[0]!);
    expect(advertised.length, 'the bindings table must not be empty').toBeGreaterThan(0);

    const handled = bindingsInDispatch();
    const invented = advertised.filter((key) => !handled.has(key));
    expect(
      invented,
      `KEYMAP.md advertises keys that dispatch.ts does not handle: ${invented.join(', ')}. ` +
        'Regenerate with npx tsx shell/scripts/gen-keymap.ts.',
    ).toEqual([]);
  });

  it('lists exactly the commands in COMMANDS, and every one of them', () => {
    const markdown = readFileSync(KEYMAP, 'utf8');
    const rows = tableUnder(markdown, '## Commands');
    const listed = new Set(rows.map((row) => row[0]!));
    const mentioned = new Set(rows.flatMap((row) => row.join(' ').split(' ')));
    const { names, aliases } = commandsInRegistry();

    const invented = [...listed].filter((name) => !names.has(name));
    expect(invented, `KEYMAP.md lists commands that are not in COMMANDS: ${invented.join(', ')}`).toEqual([]);

    const missing = [...names].filter((name) => !listed.has(name));
    expect(missing, `COMMANDS declares commands KEYMAP.md omits: ${missing.join(', ')}`).toEqual([]);

    const lostAliases = [...aliases].filter((a) => !mentioned.has(a));
    expect(lostAliases, `COMMANDS declares aliases KEYMAP.md omits: ${lostAliases.join(', ')}`).toEqual([]);
  });

  it('is byte-identical when the generator runs twice, and matches the committed file', () => {
    const first = runGenerator(['--stdout']);
    const second = runGenerator(['--stdout']);
    expect(
      second,
      'the generator is not deterministic: two runs produced different bytes (a timestamp, a ' +
        'directory listing, or an unordered set leaked into the output)',
    ).toBe(first);
    expect(
      first,
      'docs/foundation/KEYMAP.md is stale. Regenerate with: npx tsx shell/scripts/gen-keymap.ts',
    ).toBe(readFileSync(KEYMAP, 'utf8'));
  });

  it('says how to regenerate it, in the file itself', () => {
    const markdown = readFileSync(KEYMAP, 'utf8');
    expect(markdown).toContain('GENERATED FILE');
    expect(markdown).toContain('scripts/gen-keymap.ts');
  });
});
