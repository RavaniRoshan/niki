/**
 * The transcript: user turns, assistant turns, tool rows, stage rows, reasoning lines, the live
 * activity line, and inline failures.
 *
 * Row grammar, taken from the reference frames and fixed by the spec:
 *   user        raised row, `>` prefix
 *   assistant   `●` bullet, body weight
 *   tool        state glyph, bold name, dim args, then a dim result line under a `└` connector
 *   stage       the same grammar, with a retry marker when `attempt > 1` and a provenance label
 *   reasoning   one collapsed dim line with a duration, expandable
 *   failure     inline, error colour, the useful excerpt, and a recovery action when one exists
 *
 * A row exists only because an event created it. There is no code path here that invents a tool
 * call, a stage, a test result or a number.
 */

import React from 'react';
import { Box, Text } from 'ink';
import type { AppState, StageRow, ToolRow } from '../state.js';
import { paletteFor, type ThemeName } from '../theme/index.js';
import { glyphs, sweepFrame, type Charset } from '../glyphs.js';
import { formatCount, truncate } from './footer.js';

export type TranscriptProps = {
  readonly state: AppState;
  readonly theme: ThemeName;
  readonly charset: Charset;
  readonly reducedMotion: boolean;
  readonly sweepTick: number;
  /** How many body rows the caller can afford. The transcript is windowed to this. */
  readonly height: number;
};

type RenderedLine = { text: string; token: string; bold?: boolean };

/**
 * A bounded line buffer. It keeps the last `limit` lines and counts what it dropped, so building
 * a 10,000-row transcript costs the same as building a 30-row one: once the tail is full, the
 * builder stops asking the model for more rows entirely.
 */
class LineWindow {
  readonly #limit: number;
  #lines: RenderedLine[] = [];
  #dropped = 0;

  constructor(limit: number) {
    this.#limit = Math.max(1, limit);
  }

  get full(): boolean {
    return this.#lines.length >= this.#limit;
  }

  get dropped(): number {
    return this.#dropped;
  }

  push(line: RenderedLine): void {
    this.#lines.push(line);
    if (this.#lines.length > this.#limit) {
      this.#lines.shift();
      this.#dropped += 1;
    }
  }

  /** Appends a block of lines that were produced in order, keeping the tail of that block. */
  pushBlock(lines: RenderedLine[]): void {
    for (const line of lines) this.push(line);
  }

  toArray(): RenderedLine[] {
    return this.#lines;
  }
}

/**
 * Builds the visible transcript.
 *
 * Walks the state backwards, from the most recent row to the oldest, and stops as soon as the
 * window is full. That is what keeps per-token render cost flat as the transcript grows: the cost
 * is proportional to what is on screen, not to how much history exists.
 */
export function renderTranscriptLines(props: TranscriptProps): RenderedLine[] {
  const { state, theme, charset, reducedMotion, sweepTick } = props;
  const c = paletteFor(theme);
  const window = new LineWindow(props.height > 0 ? props.height : 1);

  // Reverse order: the newest thing is the first thing worth drawing.
  if (state.turnEnd) window.push({ text: renderTurnSummary(state), token: c.muted });
  if (state.interrupted) window.push({ text: 'Interrupted · type to continue', token: c.muted });
  if (state.protocolError) {
    window.push({ text: `engine error · ${state.protocolError}`, token: c.error });
  }

  // The activity line sits directly above the composer, so it is always in the window.
  if (state.activity) {
    const frame = sweepFrame(charset, sweepTick, reducedMotion);
    window.push({ text: `${frame} ${state.activity.text}`, token: c.accent });
    // The dim second line appears only when the engine supplied a "next".
    if (state.activity.next) {
      window.push({ text: `  ${glyphs(charset).connector} ${state.activity.next}`, token: c.muted });
    }
  }

  for (let i = state.notices.length - 1; i >= 0 && !window.full; i -= 1) {
    const notice = state.notices[i]!;
    window.push({
      text: notice.text,
      token: notice.level === 'error' ? c.error : notice.level === 'warning' ? c.warning : c.muted,
    });
  }

  for (let i = state.tools.length - 1; i >= 0 && !window.full; i -= 1) {
    window.pushBlock(renderTool(state.tools[i]!, c, charset, reducedMotion, sweepTick).reverse());
  }

  for (let i = state.stages.length - 1; i >= 0 && !window.full; i -= 1) {
    window.pushBlock(renderStage(state.stages[i]!, c, charset).reverse());
  }

  for (let i = state.messages.length - 1; i >= 0 && !window.full; i -= 1) {
    const message = state.messages[i]!;
    if (message.kind === 'user') {
      // The user's turn is a raised row: a `>` prefix and bold weight, the one moment in the
      // transcript that is unmistakably "you said this".
      window.push({ text: `> ${message.text}`, token: c.foreground, bold: true });
    } else {
      window.push({ text: `${glyphs(charset).bullet} ${message.text}`, token: c.foreground });
    }
  }

  // Width is applied here, in the pure function, so there is exactly one place that decides how
  // wide a row may be. The component below only maps lines to elements.
  const width = state.cols;
  return window.toArray().reverse().map((l) => ({ ...l, text: truncate(l.text, width) }));
}

function renderTool(
  tool: ToolRow,
  c: ReturnType<typeof paletteFor>,
  charset: Charset,
  reducedMotion: boolean,
  tick: number,
): RenderedLine[] {
  const g = glyphs(charset);
  const lines: RenderedLine[] = [];
  const glyph =
    tool.state === 'running'
      ? sweepFrame(charset, tick, reducedMotion)
      : tool.state === 'done'
        ? g.done
        : tool.state === 'failed'
          ? g.failed
          : g.queued;
  const token =
    tool.state === 'running'
      ? c.accent
      : tool.state === 'done'
        ? c.success
        : tool.state === 'failed'
          ? c.error
          : c.muted;

  lines.push({ text: `${glyph} ${tool.name}(${tool.args})`, token, bold: true });

  // The dim result line under a corner connector, with the expand hint. A tool that has not
  // produced a result yet shows its progress note, or nothing.
  if (tool.state === 'running') {
    if (tool.progressNote) {
      lines.push({ text: `  ${g.connector} ${tool.progressNote} · ctrl+o expand`, token: c.muted });
    }
  } else if (tool.summary) {
    const hint = tool.fullRef ? ' · ctrl+o expand' : '';
    lines.push({ text: `  ${g.connector} ${tool.summary}${hint}`, token: c.muted });
  } else if (tool.state === 'failed') {
    // A failure with no summary still has to say something true, so it says the state, not a
    // fabricated error message.
    lines.push({ text: `  ${g.connector} failed`, token: c.muted });
  }

  return lines;
}

function renderStage(stage: StageRow, c: ReturnType<typeof paletteFor>, charset: Charset): RenderedLine[] {
  const g = glyphs(charset);
  const lines: RenderedLine[] = [];
  const token =
    stage.state === 'running' ? c.accent : stage.state === 'done' ? c.success : c.error;
  const glyph = stage.state === 'running' ? g.running : stage.state === 'done' ? g.done : g.failed;

  // A retry marker comes from `attempt`, which the engine increments. It is never inferred.
  const retry = stage.attempt > 1 ? ` · retry ${stage.attempt}` : '';
  lines.push({ text: `${glyph} ${stage.role}${retry}`, token, bold: true });

  if (stage.state === 'running') {
    if (stage.reasoning.length > 0) {
      lines.push({
        text: `  ${g.collapsed} reasoned · ctrl+o`,
        token: c.muted,
      });
    }
    return lines;
  }

  if (stage.state === 'failed') {
    lines.push({ text: `  ${g.connector} ${stage.failure ?? 'failed'}`, token: c.error });
    if (stage.recovery) {
      lines.push({ text: `    ${stage.recovery}`, token: c.muted });
    }
    return lines;
  }

  // Provenance is part of the claim, so it is on the row rather than hidden in an overlay.
  const provenance =
    stage.provenance === 'independent'
      ? ' · independent review'
      : stage.provenance === 'self_verification'
        ? ' · self-verified'
        : '';
  lines.push({ text: `  ${g.connector} ${stage.summary ?? 'done'}${provenance}`, token: c.muted });

  if (stage.retryCount && stage.retryCount > 0) {
    lines.push({ text: `    retried ${stage.retryCount}×`, token: c.muted });
  }
  return lines;
}

/** "Done in 42s · 3 tool calls · 1 file changed" — every number from a real counter. */
export function renderTurnSummary(state: AppState): string {
  const end = state.turnEnd;
  if (!end) return '';
  const parts = [`Done in ${formatDuration(end.duration_ms)}`];
  parts.push(`${formatCount(end.tool_calls)} tool call${end.tool_calls === 1 ? '' : 's'}`);
  parts.push(
    `${formatCount(end.files_changed)} file${end.files_changed === 1 ? '' : 's'} changed`,
  );
  return parts.join(' · ');
}

export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  const s = ms / 1000;
  if (s < 60) return `${s < 10 ? s.toFixed(1) : Math.round(s)}s`;
  const m = Math.floor(s / 60);
  return `${m}m ${Math.round(s % 60)}s`;
}

export function Transcript(props: TranscriptProps): React.ReactElement {
  const lines = renderTranscriptLines(props);
  const c = paletteFor(props.theme);
  // Windowing: the settled transcript never enters the hot repaint path. We keep the tail that
  // fits and drop the rest, so a 10,000-message session costs the same as a 10-line one.
  const visible = props.height > 0 ? lines.slice(Math.max(0, lines.length - props.height)) : lines;
  const culled = lines.length - visible.length;
  // `renderTranscriptLines` already truncated every row to `state.cols`.

  return (
    <Box flexDirection="column" width={props.state.cols}>
      {culled > 0 ? (
        <Text color={c.muted}>{truncate(`… ${culled} earlier rows`, props.state.cols)}</Text>
      ) : null}
      {visible.map((line, i) => (
        <Text key={i} color={line.token} bold={line.bold}>
          {line.text}
        </Text>
      ))}
    </Box>
  );
}