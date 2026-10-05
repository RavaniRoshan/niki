/**
 * Every glyph the shell can draw, with its ASCII fallback.
 *
 * The rule from the spec: **colour is never the only state indicator**. Every glyph here has a
 * partner in `ascii` and a `label`, so a state survives a monochrome terminal, a screen reader,
 * and a colour-blind reader. Nothing in `src/` may contain a box-drawing or status character
 * except here and in `src/mascot.ts`; `test/lint-glyphs.test.ts` enforces that.
 */

export type Charset = 'unicode' | 'ascii';

export function charsetFromEnv(env: Record<string, string | undefined>): Charset {
  if (env.NO_UNICODE || env.NIKI_ASCII) return 'ascii';
  return 'unicode';
}

export type Glyphs = {
  /** Activity sweep, one frame per tick, ping-ponging. */
  readonly sweep: readonly [string, string, string, string];
  /** Static activity frame, used when motion is off. */
  readonly sweepStatic: string;
  /** Tool/stage states. */
  readonly running: string;
  readonly done: string;
  readonly failed: string;
  readonly queued: string;
  readonly skipped: string;
  /** Result line connector. */
  readonly connector: string;
  /** Assistant bullet. */
  readonly bullet: string;
  /** The expanded-from "reasoning collapsed" marker. */
  readonly collapsed: string;
  /** Composer prompt. */
  readonly prompt: string;
  /** Rule drawn either side of the composer. */
  readonly rule: string;
  /** Context meter's filled and empty cells. */
  readonly meterFull: string;
  readonly meterEmpty: string;
  /** Scrollbar: the thumb and the track behind it. */
  readonly scrollThumb: string;
  readonly scrollTrack: string;
};

const UNICODE: Glyphs = {
  sweep: ['◐', '◓', '◑', '◒'],
  sweepStatic: '◐',
  running: '◐',
  done: '✓',
  failed: '✗',
  queued: '○',
  skipped: '–',
  connector: '└',
  bullet: '●',
  collapsed: '⋯',
  prompt: '›',
  rule: '─',
  meterFull: '█',
  meterEmpty: '░',
  scrollThumb: '█',
  scrollTrack: '│',
};

const ASCII: Glyphs = {
  sweep: ['-', '|', '/', '\\'],
  sweepStatic: '*',
  running: '*',
  done: '+',
  failed: 'x',
  queued: 'o',
  skipped: '-',
  connector: '`',
  bullet: '*',
  collapsed: '...',
  prompt: '>',
  rule: '-',
  meterFull: '#',
  meterEmpty: '.',
  scrollThumb: '#',
  scrollTrack: '|',
};

export function glyphs(charset: Charset): Glyphs {
  return charset === 'ascii' ? ASCII : UNICODE;
}

/** The frame of the activity sweep for `frame`, honouring reduced motion. */
export function sweepFrame(charset: Charset, frame: number, reducedMotion: boolean): string {
  const g = glyphs(charset);
  if (reducedMotion) return g.sweepStatic;
  const set = g.sweep;
  return set[((frame % set.length) + set.length) % set.length]!;
}

/** Text label for a state, so the glyph is never the only signal. */
export function stateLabel(state: 'running' | 'done' | 'failed' | 'queued' | 'skipped'): string {
  switch (state) {
    case 'running':
      return 'running';
    case 'done':
      return 'done';
    case 'failed':
      return 'failed';
    case 'queued':
      return 'queued';
    case 'skipped':
      return 'skipped';
  }
}

/** True when the environment asks for no animation at all. */
export function reducedMotionFromEnv(env: Record<string, string | undefined>): boolean {
  return Boolean(env.NIKI_REDUCED_MOTION || env.NO_MOTION);
}

/** True when the environment asks for no colour at all. */
export function noColorFromEnv(env: Record<string, string | undefined>): boolean {
  return Boolean(env.NO_COLOR);
}

/**
 * Cadence in milliseconds. 100 ms while a tool is in flight, 200 ms once a single operation has
 * been running for 20 s, 120 ms otherwise — per the spec.
 */
export function sweepIntervalMs(opts: { toolInFlight: boolean; runningMs: number }): number {
  if (opts.runningMs >= 20_000) return 200;
  if (opts.toolInFlight) return 100;
  return 120;
}