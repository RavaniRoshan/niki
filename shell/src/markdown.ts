/**
 * Markdown for the transcript, streaming-safe by construction.
 *
 * The hard requirement is not "render markdown"; it is **no reflow flicker while streaming**. The
 * way that is achieved is structural: blocks are only emitted once they are *complete*. A half-typed
 * fenced code block, an unterminated table row and a paragraph still being written all render as
 * their in-progress form, and the moment the block closes the whole thing re-renders once. What
 * never happens is a line that was already on screen moving because the next token arrived.
 *
 * Everything the engine sent has already been through `sanitize` before it reaches here, so this
 * module never handles a control sequence.
 */

export type Span = {
  readonly text: string;
  /** One of: text, strong, em, code, link, dim. Never a colour — the renderer owns colour. */
  readonly style: SpanStyle;
  readonly href?: string;
};

export type SpanStyle = 'text' | 'strong' | 'em' | 'code' | 'link' | 'dim';

export type Block =
  | { kind: 'paragraph'; spans: readonly Span[] }
  | { kind: 'heading'; level: number; spans: readonly Span[] }
  | { kind: 'list'; ordered: boolean; items: readonly { depth: number; spans: readonly Span[]; done?: boolean }[] }
  | { kind: 'quote'; spans: readonly Span[] }
  | { kind: 'code'; lang: string | null; lines: readonly string[]; closed: boolean }
  | { kind: 'rule' }
  | { kind: 'table'; header: readonly string[]; rows: readonly (readonly string[])[] };

/**
 * Splits markdown into blocks, emitting a partial final block when the text ends mid-block.
 * Streaming-safe because a partial block is emitted in its in-progress form and never re-emitted
 * differently until it closes.
 */
export function parseBlocks(src: string): Block[] {
  const lines = src.split('\n');
  const blocks: Block[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i]!;

    if (line.trim() === '') {
      i += 1;
      continue;
    }

    // A fenced code block runs until its closing fence, or to the end of the text when the fence
    // has not arrived yet — which is exactly what a half-streamed code block looks like.
    const fence = /^\s*```(\w*)\s*$/.exec(line);
    if (fence) {
      const lang = fence[1] ? fence[1] : null;
      const body: string[] = [];
      i += 1;
      let closed = false;
      while (i < lines.length && !/^\s*```\s*$/.test(lines[i]!)) {
        body.push(lines[i]!);
        i += 1;
      }
      if (i < lines.length) {
        closed = true;
        i += 1;
      }
      blocks.push({ kind: 'code', lang, lines: body, closed });
      continue;
    }

    if (/^\s*(-{3,}|\*{3,}|_{3,})\s*$/.test(line)) {
      blocks.push({ kind: 'rule' });
      i += 1;
      continue;
    }

    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    if (heading) {
      blocks.push({
        kind: 'heading',
        level: heading[1]!.length,
        spans: parseInline(heading[2]!),
      });
      i += 1;
      continue;
    }

    if (/^\s*>\s?/.test(line)) {
      const body: string[] = [];
      while (i < lines.length && /^\s*>\s?/.test(lines[i]!)) {
        body.push(lines[i]!.replace(/^\s*>\s?/, ''));
        i += 1;
      }
      blocks.push({ kind: 'quote', spans: parseInline(body.join(' ')) });
      continue;
    }

    if (isTableRow(line) && i + 1 < lines.length && isTableSeparator(lines[i + 1]!)) {
      const header = splitTableRow(line);
      i += 2;
      const rows: string[][] = [];
      while (i < lines.length && isTableRow(lines[i]!)) {
        rows.push(splitTableRow(lines[i]!));
        i += 1;
      }
      blocks.push({ kind: 'table', header, rows });
      continue;
    }

    if (isListItem(line)) {
      const ordered = /^\s*\d+[.)]\s+/.test(line);
      const items: { depth: number; spans: Span[]; done?: boolean }[] = [];
      while (i < lines.length && isListItem(lines[i]!)) {
        const raw = lines[i]!;
        const indent = (raw.match(/^\s*/)?.[0].length ?? 0);
        const depth = Math.floor(indent / 2);
        const stripped = raw.replace(/^\s*(?:[-*+]|\d+[.)])\s+/, '');
        // A GitHub task item carries its state in the text; it is rendered as a real marker, not
        // as a word the user has to read past.
        const task = /^\[([ xX])\]\s+(.*)$/.exec(stripped);
        if (task) {
          items.push({ depth, spans: parseInline(task[2]!), done: task[1] !== ' ' });
        } else {
          items.push({ depth, spans: parseInline(stripped) });
        }
        i += 1;
      }
      blocks.push({ kind: 'list', ordered, items });
      continue;
    }

    // A paragraph runs until a blank line or the start of any other block.
    const para: string[] = [];
    while (
      i < lines.length &&
      lines[i]!.trim() !== '' &&
      !isListItem(lines[i]!) &&
      !/^(#{1,6})\s/.test(lines[i]!) &&
      !/^\s*```/.test(lines[i]!) &&
      !/^\s*>\s?/.test(lines[i]!)
    ) {
      para.push(lines[i]!);
      i += 1;
    }
    blocks.push({ kind: 'paragraph', spans: parseInline(para.join(' ')) });
  }

  return blocks;
}

/**
 * Inline parsing: `code`, **strong**, *em*, [links](href), and nothing else. Order matters —
 * code spans are extracted first so their contents are never re-parsed as emphasis, which is the
 * usual source of a flicker when a backtick arrives after a star.
 */
export function parseInline(src: string): Span[] {
  const spans: Span[] = [];
  const pattern = /(`[^`]+`)|(\*\*[^*]+\*\*)|(\*[^*]+\*)|(\[[^\]]+\]\([^)]+\))/g;
  let last = 0;
  let m: RegExpExecArray | null;

  while ((m = pattern.exec(src)) !== null) {
    if (m.index > last) spans.push({ text: src.slice(last, m.index), style: 'text' });
    const token = m[0];
    if (token.startsWith('`')) {
      spans.push({ text: token.slice(1, -1), style: 'code' });
    } else if (token.startsWith('**')) {
      spans.push({ text: token.slice(2, -2), style: 'strong' });
    } else if (token.startsWith('[')) {
      const link = /^\[([^\]]+)\]\(([^)]+)\)$/.exec(token);
      if (link) spans.push({ text: link[1]!, style: 'link', href: link[2]! });
      else spans.push({ text: token, style: 'text' });
    } else {
      spans.push({ text: token.slice(1, -1), style: 'em' });
    }
    last = m.index + token.length;
  }
  if (last < src.length) spans.push({ text: src.slice(last), style: 'text' });
  return spans.length > 0 ? spans : [{ text: src, style: 'text' }];
}

function isListItem(line: string): boolean {
  return /^\s*(?:[-*+]|\d+[.)])\s+/.test(line);
}

function isTableRow(line: string): boolean {
  return line.includes('|') && line.trim().startsWith('|');
}

function isTableSeparator(line: string): boolean {
  return /^\s*\|[\s:|-]+\|\s*$/.test(line) && line.includes('-');
}

function splitTableRow(line: string): string[] {
  return line
    .trim()
    .replace(/^\|/, '')
    .replace(/\|$/, '')
    .split('|')
    .map((c) => c.trim());
}

/**
 * How many blocks a given text produces. Streaming uses this to decide whether the visible block
 * count grew: a token that does not change it cannot move anything already on screen.
 */
export function blockCount(src: string): number {
  return parseBlocks(src).length;
}