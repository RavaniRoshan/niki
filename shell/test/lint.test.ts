/**
 * The rules the shell must not break, checked by reading its own source.
 *
 * These are lint tests rather than type tests because each one is an *absence*: a colour literal,
 * a mascot string or a key comparison appearing in the wrong file. A type checker cannot see an
 * absence; a test can.
 *
 * Two rules of the house apply to the lints themselves: they look at code, not at comments (a doc
 * comment is allowed to mention a glyph so a reader can find it), and each allow-list entry is the
 * one module that is *supposed* to own that thing.
 */

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { describe, expect, it } from 'vitest';

const SRC = join(import.meta.dirname, '..', 'src');

function walk(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    const p = join(dir, entry);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (p.endsWith('.ts') || p.endsWith('.tsx')) out.push(p);
  }
  return out;
}

/** Source lines with comments removed, so a doc comment can never fail a lint. */
function codeLines(file: string): { line: string; n: number }[] {
  const raw = readFileSync(file, 'utf8');
  // Block comments must be removed across the whole file first: a `/** ... *\/` header spans
  // many lines, and a per-line regex cannot match it.
  const withoutBlocks = raw.replace(/\/\*[\s\S]*?\*\//g, '');
  return withoutBlocks
    .split('\n')
    .map((line, i) => ({
      n: i + 1,
      line: line.replace(/^\s*\/\/.*$/, '').replace(/\s\/\/.*$/, ''),
    }))
    .filter((l) => l.line.trim().length > 0);
}

const FILES = walk(SRC);
const rel = (f: string) => relative(SRC, f);

const THEME_FILE = join(SRC, 'theme', 'index.ts');
const GLYPH_FILE = join(SRC, 'glyphs.ts');
const MASCOT_FILE = join(SRC, 'mascot.ts');
const DISPATCH_FILE = join(SRC, 'dispatch.ts');
const CLI_FILE = join(SRC, 'cli.tsx');

function offenders(
  allowed: string[],
  check: (line: string) => boolean,
  message: string,
): void {
  const found: string[] = [];
  for (const file of FILES) {
    if (allowed.includes(file)) continue;
    for (const { line, n } of codeLines(file)) {
      if (check(line)) found.push(`${rel(file)}:${n}: ${line.trim()}`);
    }
  }
  expect(found, message).toEqual([]);
}

describe('lint: one theme token system', () => {
  it('has no hex colour literal outside src/theme/index.ts', () => {
    offenders(
      [THEME_FILE],
      (line) => /#[0-9A-Fa-f]{6}\b|#[0-9A-Fa-f]{3}\b/.test(line),
      'colour literals belong in the theme module only',
    );
  });

  it('uses no named terminal colour as a component colour outside the theme module', () => {
    // Matching Ink's colour prop specifically, rather than any occurrence of the word: `red` is
    // also a pipeline role name, and role names are not colours.
    offenders(
      [THEME_FILE],
      (line) => /color\s*=\s*['"](red|green|blue|yellow|magenta|cyan|white|black|gray|grey)['"]/.test(line),
      'named terminal colours belong in the theme module only',
    );
  });
});

describe('lint: art lives in one module', () => {
  it('keeps mascot body art out of every module but src/mascot.ts and src/glyphs.ts', () => {
    offenders(
      [MASCOT_FILE, GLYPH_FILE],
      (line) => /[▄▀]/.test(line),
      'mascot art must exist only in the mascot module',
    );
  });

  it('keeps box-drawing glyphs out of every module but src/glyphs.ts and src/mascot.ts', () => {
    offenders(
      [GLYPH_FILE, MASCOT_FILE],
      (line) => /[─│└┌┐┘█░]/.test(line),
      'glyphs must exist only in src/glyphs.ts (the mascot body is the one exception)',
    );
  });
});

describe('lint: one dispatcher', () => {
  it('matches no key or escape sequence outside src/dispatch.ts', () => {
    offenders(
      [DISPATCH_FILE, GLYPH_FILE, MASCOT_FILE, CLI_FILE],
      (line) => /\bkey\.(escape|return|upArrow|downArrow|pageUp|pageDown)\b/.test(line),
      'key matching belongs to the dispatcher only',
    );
  });

  it('matches no raw escape byte outside the sanitizer, the input parser and terminal setup', () => {
    // cli.tsx is exempt only so it can send the two restore sequences on the way out; the count
    // of escape literals it is allowed is pinned by the next test, so the exemption cannot grow.
    offenders(
      [join(SRC, 'sanitize.ts'), join(SRC, 'input.ts'), DISPATCH_FILE, GLYPH_FILE, MASCOT_FILE, CLI_FILE],
      (line) => /\\u001[bB]|\\x1[bB]|\\e\[/.test(line),
      'escape-sequence knowledge belongs to the sanitizer and the input parser only',
    );
  });

  it('lets cli.tsx contain exactly one escape literal: the reset it sends on restore', () => {
    // Every other sequence is built from the ESC constant, so a raw escape byte here would mean
    // someone bypassed the one constant on purpose.
    const escapeLiterals = codeLines(CLI_FILE).filter((l) => /\\x1[bB]|\\u001[bB]/.test(l.line));
    expect(escapeLiterals.map((l) => l.line.trim())).toEqual([]);
  });
});

describe('lint: nothing writes to stdout while the interface is live', () => {
  it('has no direct console write outside src/cli.tsx', () => {
    offenders(
      [CLI_FILE],
      (line) => /process\.stdout\.(write|writeSync)|console\.(log|info|warn|error|debug)\s*\(/.test(line),
      'while the interface is live, logs go to a file and the TUI owns the terminal',
    );
  });
});

describe('lint: no invented values', () => {
  it('never hard-codes a cost, a token count or a percentage in a component', () => {
    offenders(
      [rel(THEME_FILE)],
      (line) => /ctx \d+%|\$\d+\.\d{2}|\d+k\/|\d+ms ·/.test(line) && !rel(THEME_FILE).startsWith('theme'),
      'cost, tokens and context percentages must come from the engine, not from a literal',
    );
  });
});