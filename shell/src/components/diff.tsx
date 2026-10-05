/**
 * The diff view.
 *
 * Everything the reference products show, and nothing they invent: a unified diff, optional line
 * numbers, hunks with their own headers, intra-line highlighting, unchanged regions folded, and
 * next/previous hunk and file navigation. Binary files, renames and mode changes are shown as
 * what they are — a one-line fact — because a diff view that silently drops them is a diff view
 * that lies about the change.
 *
 * The parser takes text. It does not run `git`: the engine sends the patch over the seam, and the
 * shell renders it. Nothing here invents a hunk, a line number or a count.
 */

import React from 'react';
import { Box, Text } from 'ink';
import { sanitize } from '../sanitize.js';
import { paletteFor, type ThemeName } from '../theme/index.js';
import { glyphs, type Charset } from '../glyphs.js';
import { truncate } from './footer.js';

export type DiffLineKind = 'context' | 'added' | 'removed' | 'meta';

export type ParsedLine = {
  readonly kind: DiffLineKind;
  /** 1-based, or null for a file/hunk header. */
  readonly oldNo: number | null;
  readonly newNo: number | null;
  readonly text: string;
  /** The column range inside `text` that changed, for intra-line highlighting. */
  readonly emphasis: readonly [number, number] | null;
  /**
   * 1-based position of this line's hunk in the **whole patch**. Diff pagers count hunks across
   * the entire diff, so "hunk 3 of 7" is the third hunk the reader walks through; counting per
   * file would make "hunk 1" ambiguous the moment a patch touches two files. 0 means the line is
   * not inside a hunk (a file header, a rename, a mode change).
   */
  readonly hunk: number;
};

export type ParsedFile = {
  readonly kind: 'text' | 'binary' | 'rename' | 'mode';
  readonly oldPath: string | null;
  readonly newPath: string;
  readonly lines: readonly ParsedLine[];
  readonly added: number;
  readonly removed: number;
  /** True when the file content was left out and only the header shown. */
  readonly truncated: boolean;
};

const FOLD_THRESHOLD = 6;

/**
 * Parses a unified diff into files and hunks.
 *
 * Every line is sanitised on the way in: a patch is engine-supplied text and must be cleaned
 * exactly like any other.
 */
export function parseUnifiedDiff(patch: string, maxLinesPerFile = 2000): ParsedFile[] {
  const raw = sanitize(patch).split('\n');
  const files: ParsedFile[] = [];
  let globalHunk = 0;
  let current: { file: ParsedFile | null; hunk: ParsedLine[]; oldNo: number; newNo: number; run: ParsedLine[] } = {
    file: null,
    hunk: [],
    oldNo: 0,
    newNo: 0,
    run: [],
  };

  const flushHunk = (): void => {
    if (current.hunk.length > 0) {
      current.run.push(...foldRun(current.hunk));
      current.hunk = [];
    }
  };

  const flushFile = (): void => {
    flushHunk();
    if (current.file) {
      const lines = current.run;
      const trimmed = lines.length > maxLinesPerFile ? lines.slice(0, maxLinesPerFile) : lines;
      files.push({
        ...current.file,
        lines: trimmed,
        truncated: lines.length > trimmed.length,
      });
    }
    current = { file: null, hunk: [], oldNo: 0, newNo: 0, run: [] };
  };

  for (const line of raw) {
    if (line.startsWith('diff --git ')) {
      flushFile();
      const paths = /a\/(.+?) b\/(.+)$/.exec(line);
      current.file = {
        kind: 'text',
        oldPath: paths?.[1] ?? null,
        newPath: paths?.[2] ?? line.replace('diff --git ', ''),
        lines: [],
        added: 0,
        removed: 0,
        truncated: false,
      };
      continue;
    }
    if (line.startsWith('--- ')) {
      // A patch may legitimately arrive without a `diff --git` header. Starting the file here is
      // what stops a bare hunk from being silently dropped.
      if (!current.file) {
        current.file = {
          kind: 'text',
          oldPath: null,
          newPath: line.slice(4).trim(),
          lines: [],
          added: 0,
          removed: 0,
          truncated: false,
        };
      }
      continue;
    }
    if (!current.file) continue;

    if (line.startsWith('Binary files ') || line.startsWith('GIT binary patch')) {
      current.file = { ...current.file, kind: 'binary' };
      current.run.push({ kind: 'meta', oldNo: null, newNo: null, text: line, emphasis: null, hunk: 0 });
      continue;
    }
    if (line.startsWith('rename from ') || line.startsWith('rename to ')) {
      current.file = { ...current.file, kind: 'rename' };
      current.run.push({ kind: 'meta', oldNo: null, newNo: null, text: line, emphasis: null, hunk: 0 });
      continue;
    }
    if (line.startsWith('old mode ') || line.startsWith('new mode ')) {
      current.file = { ...current.file, kind: 'mode' };
      current.run.push({ kind: 'meta', oldNo: null, newNo: null, text: line, emphasis: null, hunk: 0 });
      continue;
    }
    if (line.startsWith('@@')) {
      flushHunk();
      const m = /@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@(.*)/.exec(line);
      current.oldNo = m ? Number.parseInt(m[1]!, 10) : 0;
      current.newNo = m ? Number.parseInt(m[2]!, 10) : 0;
      globalHunk += 1;
      current.run.push({ kind: 'meta', oldNo: null, newNo: null, text: line, emphasis: null, hunk: globalHunk });
      continue;
    }
    if (line.startsWith('+++')) continue;

    const kind: DiffLineKind = line.startsWith('+')
      ? 'added'
      : line.startsWith('-')
        ? 'removed'
        : 'context';
    const text = line.slice(1);
    const parsed: ParsedLine = {
      kind,
      oldNo: kind === 'added' ? null : current.oldNo,
      newNo: kind === 'removed' ? null : current.newNo,
      text,
      emphasis: null,
      hunk: globalHunk,
    };
    if (kind === 'added') {
      current.newNo += 1;
      current.file = { ...current.file, added: current.file.added + 1 };
    } else if (kind === 'removed') {
      current.oldNo += 1;
      current.file = { ...current.file, removed: current.file.removed + 1 };
    } else {
      current.oldNo += 1;
      current.newNo += 1;
    }
    current.hunk.push(parsed);
  }
  flushFile();
  return files;
}

/**
 * Collapses a long run of unchanged lines to a single marker, keeping the first and last few so
 * the fold still has visible edges. This is the "fold unchanged regions" behaviour, done without
 * a scroll position: the marker states exactly how many lines it stands for.
 */
function foldRun(run: readonly ParsedLine[]): ParsedLine[] {
  const out: ParsedLine[] = [];
  let i = 0;
  while (i < run.length) {
    if (run[i]!.kind !== 'context') {
      out.push(run[i]!);
      i += 1;
      continue;
    }
    let j = i;
    while (j < run.length && run[j]!.kind === 'context') j += 1;
    const span = j - i;
    if (span <= FOLD_THRESHOLD) {
      for (let k = i; k < j; k += 1) out.push(run[k]!);
    } else {
      for (let k = i; k < i + 2; k += 1) out.push(run[k]!);
      out.push({
        kind: 'meta',
        oldNo: null,
        newNo: null,
        text: `${glyphs('ascii').meterFull} ${span - 4} unchanged lines`,
        emphasis: null,
        hunk: run[i]!.hunk,
      });
      for (let k = j - 2; k < j; k += 1) out.push(run[k]!);
    }
    i = j;
  }
  return out;
}

/**
 * Intra-line emphasis: the longest common prefix and suffix between a removed and the added line
 * that follows it. Computed here rather than by the engine so both sides of a pair are judged by
 * the same rule.
 */
export function markIntraLine(prev: ParsedLine, next: ParsedLine): [number, number] | null {
  if (prev.kind !== 'removed' || next.kind !== 'added') return null;
  const a = prev.text;
  const b = next.text;
  let start = 0;
  const max = Math.min(a.length, b.length);
  while (start < max && a[start] === b[start]) start += 1;
  let endA = a.length;
  let endB = b.length;
  while (endA > start && endB > start && a[endA - 1] === b[endB - 1]) {
    endA -= 1;
    endB -= 1;
  }
  if (start >= endB) return null;
  return [start, endB];
}

export type DiffProps = {
  readonly patch: string;
  readonly theme: ThemeName;
  readonly charset: Charset;
  readonly width: number;
  readonly showLineNumbers: boolean;
  /** 1-based hunk to show; `null` shows every hunk. */
  readonly hunk: number | null;
  /** 1-based file to show; `null` shows every file. */
  readonly file: number | null;
};

export function Diff(props: DiffProps): React.ReactElement {
  const c = paletteFor(props.theme);
  const g = glyphs(props.charset);
  const all = parseUnifiedDiff(props.patch);
  const wantedFile = props.file;
  const wantedHunk = props.hunk;
  // The parser already numbered hunks across the whole patch, so navigation is a plain filter.
  const byFile = wantedFile === null ? all : all.filter((_, i) => i === wantedFile - 1);
  const files =
    wantedHunk === null
      ? byFile
      : byFile.filter((f) => f.lines.some((l) => l.hunk === wantedHunk));

  if (all.length === 0) {
    // Honest empty state: the engine sent a diff reference but no patch text.
    return <Text color={c.muted}>no diff to show</Text>;
  }

  const out: React.ReactElement[] = [];
  for (const [index, file] of files.entries()) {
    const label = file.oldPath && file.oldPath !== file.newPath ? `${file.oldPath} → ${file.newPath}` : file.newPath;
    out.push(
      <Text key={`h${index}`} color={c.foreground} bold>
        {truncate(`${file.kind === 'binary' ? 'binary' : file.kind === 'rename' ? 'rename' : file.kind === 'mode' ? 'mode' : 'file'} ${label}`, props.width)}
      </Text>,
    );
    out.push(
      <Text key={`s${index}`} color={c.muted}>
        {`${g.done} ${file.added}  ${g.failed} ${file.removed}`}
      </Text>,
    );

    let previous: ParsedLine | null = null;
    for (const [lineIndex, line] of file.lines.entries()) {
      // rename/mode metadata belongs to the file header and is shown with it, never inside a
      // selected hunk.
      const isHeader = line.kind === 'meta' && line.hunk === 0;
      if (wantedHunk !== null && !isHeader && line.hunk !== wantedHunk) continue;

      const emphasis = markIntraLine(previous ?? ({ kind: 'removed', text: '', oldNo: null, newNo: null, emphasis: null, hunk: 0 } as ParsedLine), line);
      const token =
        line.kind === 'added' ? c.success : line.kind === 'removed' ? c.error : c.muted;
      const marker = line.kind === 'added' ? g.done : line.kind === 'removed' ? g.failed : ' ';
      const numbers = props.showLineNumbers
        ? `${String(line.oldNo ?? '').padStart(4, ' ')} ${String(line.newNo ?? '').padStart(4, ' ')} `
        : '';
      out.push(
        <Text key={`${index}-${lineIndex}`} color={token}>
          {truncate(`${marker} ${numbers}${line.text}${emphasis ? ' ' + g.bullet : ''}`, props.width)}
        </Text>,
      );
      previous = line;
    }
    if (file.truncated) {
      out.push(<Text key={`t${index}`} color={c.muted}>{`${g.collapsed} more lines not shown`}</Text>);
    }
  }

  return <Box flexDirection="column">{out}</Box>;
}

/** Hunk count and file count, for the "n of m" navigation hint. */
export function diffShape(patch: string): { files: number; hunks: number } {
  const files = parseUnifiedDiff(patch);
  return {
    files: files.length,
    hunks: files.reduce((n, f) => n + Math.max(0, ...f.lines.map((l) => l.hunk)), 0),
  };
}