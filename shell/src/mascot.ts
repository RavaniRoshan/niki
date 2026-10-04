/**
 * NIKI's mascot: a small pixel orb with exactly one eye.
 *
 * The eye is the half-lit glyph from NIKI's own activity sweep, so the mascot and the spinner are
 * one visual idea. The mascot is a **truthful status indicator**, never decoration: it shows
 * `done` only after a `turn.end`, and `error` only after a failure event. It never schedules a
 * redraw by itself, and it never animates — the sweep belongs to the activity line, so there is
 * never a double spinner.
 *
 * Art lives here and nowhere else. `test/lint-mascot.test.ts` fails if these strings appear in
 * any other module.
 */

import type { ThemeName } from './theme/index.js';
import type { Charset } from './glyphs.js';

export type MascotState = 'idle' | 'working' | 'done' | 'error' | 'interrupted';

export type MascotTier = 'full' | 'compact' | 'tiny';

/**
 * The seven-column, three-row orb, built only from half blocks and full blocks: one eye, no
 * legs, no antennae, no second eye. The shoulders on rows one and three are what make the
 * silhouette an orb rather than a rectangle, and the leading space is what keeps all three rows
 * exactly seven columns wide so the header never jitters.
 */
export const FULL_ART: Readonly<Record<MascotState, readonly [string, string, string]>> =
  Object.freeze({
    idle: [' ▄████▄', '███◐███', ' ▀████▀'],
    // Flipped eye. Static on purpose: the sweep belongs to the activity line, so the mascot and
    // the spinner are never two animations at once.
    working: [' ▄████▄', '███◑███', ' ▀████▀'],
    done: [' ▄████▄', '███●███', ' ▀████▀'],
    error: [' ▄████▄', '███○███', ' ▀████▀'],
    interrupted: [' ▄████▄', '███◐███', ' ▀████▀'],
});

/** One-row form: the eye alone, then the header text supplies the rest. */
export const COMPACT_ART: Readonly<Record<MascotState, string>> = Object.freeze({
  idle: '◐',
  working: '◑',
  done: '●',
  error: '○',
  interrupted: '◐',
});

/** ASCII / NO_COLOR: no art, one marker. */
const TINY_ART: Readonly<Record<MascotState, string>> = Object.freeze({
  idle: 'o',
  working: 'o',
  done: '*',
  error: 'x',
  interrupted: 'o',
});

/** Which token the orb body takes in each state. Read from the theme, never a literal here. */
const BODY_TOKEN: Readonly<Record<MascotState, 'accent' | 'success' | 'error' | 'muted'>> =
  Object.freeze({
    idle: 'accent',
    working: 'accent',
    done: 'success',
    error: 'error',
    interrupted: 'muted',
  });

/** Which token the eye takes. The eye sits on the body, so it uses the background colour. */
const EYE_TOKEN = 'background' as const;

export const MASCOT_WIDTH = 7;

/** Width tiers from the spec: full art at 80+, compact at 50-79, tiny below 50. */
export function tierForWidth(cols: number): MascotTier {
  if (cols >= 80) return 'full';
  if (cols >= 50) return 'compact';
  return 'tiny';
}

/** The mascot for a tier, in a charset, in a state. Returns lines of text plus their colour tokens. */
export function mascot(
  state: MascotState,
  tier: MascotTier,
  charset: Charset,
  _theme: ThemeName,
): { lines: readonly string[]; bodyToken: string; eyeToken: string } {
  const bodyToken = BODY_TOKEN[state];
  if (charset === 'ascii') {
    return {
      lines: tier === 'full' ? [TINY_ART[state]] : [TINY_ART[state]],
      bodyToken,
      eyeToken: bodyToken,
    };
  }
  if (tier === 'full') {
    return { lines: FULL_ART[state], bodyToken, eyeToken: EYE_TOKEN };
  }
  return { lines: [COMPACT_ART[state]], bodyToken, eyeToken: EYE_TOKEN };
}

/** Display width of the orb in any state. Constant, so the header never jitters. */
export function mascotWidth(state: MascotState, tier: MascotTier, charset: Charset): number {
  if (charset === 'ascii') return TINY_ART[state].length;
  if (tier === 'full') return MASCOT_WIDTH;
  return COMPACT_ART[state].length;
}

/**
 * A width test helper: the owner asked for constant width across states, and the reason is
 * visible here — every eye glyph is one codepoint and one column wide.
 */
export function everyStateHasTheSameWidth(tier: MascotTier, charset: Charset): boolean {
  const widths = (['idle', 'working', 'done', 'error', 'interrupted'] as const).map((s) =>
    mascotWidth(s, tier, charset),
  );
  return widths.every((w) => w === widths[0]);
}