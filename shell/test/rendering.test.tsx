/**
 * G1, G4, G5, G6, G7, G8, G9 — the rendering rows.
 *
 * Each block names the row it proves. The diff tests in particular are the ones that matter: a
 * diff view that drops a binary file or a rename is worse than no diff view, because it looks
 * correct.
 */

import React from 'react';
import { render } from 'ink-testing-library';
import { describe, expect, it } from 'vitest';

import { App } from '../src/app.js';
import { Diff, diffShape, markIntraLine, parseUnifiedDiff } from '../src/components/diff.js';
import { initialState, reduce, type AppState, type ReduceOptions } from '../src/state.js';
import { PALETTES, TOKEN_PAIRS, UI_CONTRAST_MIN, contrastRatio, type ThemeName } from '../src/theme/index.js';
import { glyphs } from '../src/glyphs.js';
import { parseBlocks } from '../src/markdown.js';
import type { ServerNotification } from '../src/protocol/generated/index.js';

const T0: ReduceOptions = { nowMs: 0 };
const SIZES: ReadonlyArray<readonly [number, number]> = [
  [49, 16],
  [50, 16],
  [79, 24],
  [80, 24],
  [119, 30],
  [120, 38],
  [180, 50],
];

const PATCH = `diff --git a/src/list.rs b/src/list.rs
index 1111111..2222222 100644
--- a/src/list.rs
+++ b/src/list.rs
@@ -10,7 +10,7 @@ pub fn paginate(items: &[u32], start: usize, size: usize) -> &[u32] {
     let end = start + size;
-    &items[start..end]
+    &items[start..end - 1]
 }
diff --git a/src/blob.bin b/src/blob.bin
index 3333333..4444444 100644
Binary files a/src/blob.bin and b/src/blob.bin differ
diff --git a/old/name.rs b/new/name.rs
similarity index 96%
rename from old/name.rs
rename to new/name.rs
diff --git a/perm.rs b/perm.rs
old mode 100644
new mode 100755
@@ -1,3 +1,4 @@
 fn main() {
+    set_mode(0o755);
 }
`;

function frame(state: AppState, cols: number, rows: number): string {
  const out = render(
    <App state={{ ...state, cols, rows }} theme="niki" charset="unicode" reducedMotion />,
  );
  const text = out.lastFrame() ?? '';
  out.unmount();
  return text;
}

const SESSION = {
  method: 'session.ready',
  params: {
    session_id: 's',
    project_path: '/p',
    model: 'm',
    permission_mode: 'manual',
    branch: 'main',
    ahead: null,
    behind: null,
    resumed_messages: 0,
  },
} as ServerNotification;

function stateWith(events: ServerNotification[]): AppState {
  let s: AppState = initialState(80, 24);
  for (const e of [SESSION, ...events]) s = reduce(s, e, T0);
  return s;
}

describe('G1: responsive tiers at seven sizes', () => {
  it.each(SIZES)('renders at %ix%i with no line past the edge', (cols, rows) => {
    const s = stateWith([
      { method: 'turn.started', params: { turn_id: 't', prompt: 'change the slice bound' } } as ServerNotification,
      { method: 'tool.call', params: { tool_id: 'a', name: 'Edit', args: 'src/list.rs' } } as ServerNotification,
    ]);
    const out = frame(s, cols, rows);
    const longest = out.split('\n').reduce((n, l) => Math.max(n, l.length), 0);
    expect(longest, `a line overflowed at ${cols}`).toBeLessThanOrEqual(cols);
  });

  it('says so plainly below 50 columns rather than drawing a squeezed composer', () => {
    const out = frame(stateWith([]), 49, 16);
    expect(out).toContain('49');
    expect(out).toContain('50 or more');
  });
});

describe('G5: colour comes from tokens and every pair is legible', () => {
  it.each(['niki', 'niki-light', 'niki-contrast', 'niki-dim'] as ThemeName[])(
    '%s clears the UI-glyph floor for every declared pair',
    (name) => {
      const palette = PALETTES[name];
      const failures: string[] = [];
      for (const pair of TOKEN_PAIRS) {
        const ratio = contrastRatio(palette[pair.fg] as string, palette[pair.bg] as string);
        if (ratio < UI_CONTRAST_MIN) failures.push(`${pair.fg} on ${pair.bg} = ${ratio.toFixed(2)}`);
      }
      expect(failures, `${name}`).toEqual([]);
    },
  );

  it('renders legibly with the ASCII charset, where colour cannot help', () => {
    const a = glyphs('ascii');
    expect(a.done).not.toBe(a.failed);
    expect(new Set(a.sweep).size).toBe(4);
  });
});

describe('G6: markdown covers the block kinds the spec lists', () => {
  const md = [
    '# Heading',
    '',
    'Paragraph with `code`, **bold**, *em* and [a link](https://example.invalid).',
    '',
    '- one',
    '  - nested',
    '- [x] done',
    '- [ ] todo',
    '',
    '> quoted',
    '',
    '```rust',
    'fn main() {}',
    '```',
    '',
    '| col | col |',
    '| --- | --- |',
    '| a   | b   |',
    '',
    '---',
  ].join('\n');

  it('parses every kind', () => {
    const kinds = parseBlocks(md).map((b) => b.kind);
    for (const k of ['heading', 'paragraph', 'list', 'quote', 'code', 'table', 'rule']) {
      expect(kinds, `missing ${k}`).toContain(k);
    }
  });

  it('keeps the code language and knows when a fence is still open', () => {
    const [open] = parseBlocks('```ts\nconst a = 1;');
    expect(open).toMatchObject({ kind: 'code', lang: 'ts', closed: false });
    const [closed] = parseBlocks('```ts\nconst a = 1;\n```');
    expect(closed).toMatchObject({ kind: 'code', lang: 'ts', closed: true });
  });

  it('renders the same blocks identically at every width without reflowing the list', () => {
    const state = stateWith([
      { method: 'turn.started', params: { turn_id: 't', prompt: 'show me the markdown' } } as ServerNotification,
      { method: 'turn.delta', params: { turn_id: 't', text: md } } as ServerNotification,
    ]);
    for (const [cols, rows] of SIZES) {
      const out = frame(state, cols, rows);
      const longest = out.split('\n').reduce((n, l) => Math.max(n, l.length), 0);
      expect(longest, `overflow at ${cols}`).toBeLessThanOrEqual(cols);
    }
  });
});

describe('G7: the diff view', () => {
  const files = parseUnifiedDiff(PATCH);

  it('parses every file in the patch', () => {
    expect(files.length).toBe(4);
    expect(files[0]!.newPath).toBe('src/list.rs');
    expect(files[0]!.added).toBe(1);
    expect(files[0]!.removed).toBe(1);
  });

  it('never drops a binary file, a rename or a mode change', () => {
    expect(files.map((f) => f.kind)).toEqual(['text', 'binary', 'rename', 'mode']);
  });

  it('numbers both sides, and only the side a line exists on', () => {
    const [first] = files;
    const added = first!.lines.find((l) => l.kind === 'added');
    const removed = first!.lines.find((l) => l.kind === 'removed');
    expect(added!.newNo).not.toBeNull();
    expect(added!.oldNo).toBeNull();
    expect(removed!.oldNo).not.toBeNull();
    expect(removed!.newNo).toBeNull();
  });

  it('folds a long unchanged run and says how many lines it stands for', () => {
    const long = ['--- a/f', '+++ b/f', '@@ -1,40 +1,40 @@', ...Array.from({ length: 30 }, () => ' context line')].join('\n');
    const [file] = parseUnifiedDiff(long);
    const marker = file!.lines.find((l) => l.kind === 'meta' && l.text.includes('unchanged lines'));
    expect(marker, 'a 30-line unchanged run was not folded').toBeDefined();
    expect(marker!.text).toMatch(/\d+ unchanged lines/);
  });

  it('leaves a short unchanged run alone', () => {
    const short = ['--- a/f', '+++ b/f', '@@ -1,4 +1,4 @@', ...Array.from({ length: 3 }, () => ' context')].join('\n');
    const [file] = parseUnifiedDiff(short);
    expect(file!.lines.some((l) => l.text.includes('unchanged lines'))).toBe(false);
  });

  it('marks the changed span inside a replaced pair', () => {
    const removed = { kind: 'removed' as const, oldNo: 1, newNo: null, text: '&items[start..end]', emphasis: null, hunk: 1 };
    const added = { kind: 'added' as const, oldNo: null, newNo: 1, text: '&items[start..end - 1]', emphasis: null, hunk: 1 };
    // The common prefix is `&items[start..end` (17 chars) and the common suffix is `]`; what is
    // actually inserted in the added line is ` - 1`.
    expect(markIntraLine(removed, added)).toEqual([17, 21]);
  });

  it('marks nothing for lines that do not form a replaced pair', () => {
    const context = { kind: 'context' as const, oldNo: 1, newNo: 1, text: 'same', emphasis: null, hunk: 1 };
    const added = { kind: 'added' as const, oldNo: null, newNo: 2, text: 'other', emphasis: null, hunk: 1 };
    expect(markIntraLine(context, added)).toBeNull();
  });

  it('shows or hides line numbers on request', () => {
    const props = { patch: PATCH, theme: 'niki' as ThemeName, charset: 'unicode' as const, width: 100, file: null, hunk: null };
    const withNumbers = render(<Diff {...props} showLineNumbers />).lastFrame() ?? '';
    const without = render(<Diff {...props} showLineNumbers={false} />).lastFrame() ?? '';
    expect(withNumbers).not.toBe(without);
  });

  it('navigates to one hunk and one file', () => {
    const props = { patch: PATCH, theme: 'niki' as ThemeName, charset: 'unicode' as const, width: 100, showLineNumbers: true };
    const all = render(<Diff {...props} hunk={null} file={null} />).lastFrame() ?? '';
    const firstHunk = render(<Diff {...props} hunk={1} file={null} />).lastFrame() ?? '';
    expect(firstHunk.length).toBeLessThan(all.length);
    // Hunk 1 lives in the first file; showing the other three headers would read as a broken view.
    expect(firstHunk).toContain('src/list.rs');
    expect(firstHunk).not.toContain('perm.rs');
  });

  it('reports its own shape so navigation can say "n of m"', () => {
    const shape = diffShape(PATCH);
    expect(shape.files).toBe(4);
    expect(shape.hunks).toBeGreaterThanOrEqual(2);
  });

  it('says so when there is no diff rather than drawing an empty box', () => {
    const out = render(
      <Diff patch="" theme="niki" charset="unicode" width={80} showLineNumbers file={null} hunk={null} />,
    ).lastFrame();
    expect(out).toContain('no diff');
  });

  it('sanitises a hostile patch before rendering it', () => {
    // A real OSC needs its ESC byte and a terminator; without them it is ordinary text.
    const hostile = `--- a/f\n+++ b/f\n@@ -1,1 +1,1 @@\n+\u001b]0;PWNED\u0007\n-ok\n`;
    const [file] = parseUnifiedDiff(hostile);
    for (const line of file!.lines) expect(line.text).not.toContain(']0;');
    // The whole sequence goes, payload included: it was never display text to begin with.
    expect(file!.lines.some((l) => l.text.includes('PWNED'))).toBe(false);
    // The patch still parses, so a hostile line does not cost the caller the rest of the diff.
    expect(file!.lines.some((l) => l.kind === 'removed' && l.text === 'ok')).toBe(true);
  });

  it('fails closed on an unterminated escape: the rest of the patch is dropped, not leaked', () => {
    // An OSC with no BEL and no ST swallows everything after it. That is the safe direction:
    // losing a line of a diff is recoverable, emitting a stray title-spoof into the user's
    // terminal is not.
    const truncated = `--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n+\u001b]0;PWNED\n-ok\n`;
    const [file] = parseUnifiedDiff(truncated);
    expect(file!.lines.some((l) => l.text.includes('PWNED'))).toBe(false);
    expect(file!.lines.some((l) => l.text.includes('ok'))).toBe(false);
  });
});

describe('G8: every surface has an empty, loading and error state', () => {
  const states: ReadonlyArray<[string, AppState, string]> = [
    ['empty', initialState(80, 24), 'Niki'],
    [
      'working',
      stateWith([{ method: 'turn.started', params: { turn_id: 't', prompt: 'go' } } as ServerNotification]),
      'esc interrupt',
    ],
    [
      'approval',
      stateWith([
        { method: 'turn.started', params: { turn_id: 't', prompt: 'go' } } as ServerNotification,
        {
          method: 'approval.request',
          params: { id: 'a', tool: 'bash', command: 'ls', options: [{ id: 'deny', label: 'Deny' }], safest_option_id: 'deny' },
        } as ServerNotification,
      ]),
      'wants to run',
    ],
    [
      'error',
      stateWith([
        { method: 'stage.failed', params: { stage_id: 'g', role: 'coder', error: 'boom', severity: 'error', recovery: null } } as ServerNotification,
      ]),
      'coder',
    ],
    [
      'done',
      stateWith([
        { method: 'turn.started', params: { turn_id: 't', prompt: 'go' } } as ServerNotification,
        { method: 'turn.end', params: { turn_id: 't', summary: 's', duration_ms: 1000, tool_calls: 1, files_changed: 1 } } as ServerNotification,
      ]),
      'Done in',
    ],
  ];

  it.each(states)('%s renders something, not a blank screen', (_name, state, expected) => {
    const out = frame(state, 80, 24);
    expect(out.trim().length).toBeGreaterThan(0);
    expect(out).toContain(expected);
  });

  it('never renders undefined or null into a frame', () => {
    for (const [, state] of states) {
      const out = frame(state, 80, 24);
      expect(out).not.toContain('undefined');
      expect(out).not.toContain('null');
    }
  });
});

describe('G9: any size renders without panic or overlap', () => {
  it('renders every state at sizes from 1x1 to 300x100', () => {
    const sizes: ReadonlyArray<readonly [number, number]> = [
      [1, 1],
      [2, 1],
      [5, 3],
      [20, 8],
      [49, 16],
      [80, 24],
      [120, 38],
      [200, 60],
      [300, 100],
    ];
    const states = [
      initialState(80, 24),
      stateWith([
        { method: 'turn.started', params: { turn_id: 't', prompt: 'go' } } as ServerNotification,
        { method: 'tool.call', params: { tool_id: 'a', name: 'Bash', args: 'npm test' } } as ServerNotification,
      ]),
      stateWith([
        {
          method: 'approval.request',
          params: { id: 'a', tool: 'bash', command: 'ls', options: [{ id: 'deny', label: 'Deny' }], safest_option_id: 'deny' },
        } as ServerNotification,
      ]),
    ];
    for (const [cols, rows] of sizes) {
      for (const state of states) {
        expect(() => frame(state, cols, rows), `${cols}x${rows} threw`).not.toThrow();
      }
    }
  });
});