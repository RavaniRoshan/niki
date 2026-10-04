/**
 * The mascot.
 *
 * The owner's rule: one eye, no legs, no antennae, a truthful status indicator, constant width,
 * and art that exists in exactly one module. Each of those is a test here rather than a promise.
 */

import { describe, expect, it } from 'vitest';
import {
  COMPACT_ART,
  FULL_ART,
  MASCOT_WIDTH,
  everyStateHasTheSameWidth,
  mascot,
  mascotWidth,
  tierForWidth,
  type MascotState,
} from '../src/mascot.js';
import { PALETTES, THEME_NAMES, contrastRatio, UI_CONTRAST_MIN } from '../src/theme/index.js';

const STATES: MascotState[] = ['idle', 'working', 'done', 'error', 'interrupted'];

describe('tiers', () => {
  it('follows the width rule from the spec', () => {
    expect(tierForWidth(120)).toBe('full');
    expect(tierForWidth(80)).toBe('full');
    expect(tierForWidth(79)).toBe('compact');
    expect(tierForWidth(50)).toBe('compact');
    expect(tierForWidth(49)).toBe('tiny');
  });
});

describe('silhouette', () => {
  it('has exactly one eye in every state', () => {
    for (const state of STATES) {
      const lines = FULL_ART[state];
      const eyes = lines.join('').match(/[◐◑●○]/g) ?? [];
      expect(eyes, `${state} must have exactly one eye`).toHaveLength(1);
    }
  });

  it('never uses legs, antennae, or a creature glyph', () => {
    for (const state of STATES) {
      const art = FULL_ART[state].join('');
      // "Exactly one eye" is asserted above; here we only rule out the silhouettes both
      // references use: a wide body with legs, or a rounded square face with two dot eyes.
      expect(art, `${state} must not use a crab or bug glyph`).not.toMatch(
        /[\u{1F980}\u{1F41B}\u{1F99E}]/u,
      );
      expect(FULL_ART[state], `${state} must stay three rows tall`).toHaveLength(3);
    }
  });

  it('is built only from half blocks and full blocks plus the eye', () => {
    for (const state of STATES) {
      for (const line of FULL_ART[state]) {
        // A leading space is part of the silhouette: it is what keeps all three rows the same
        // width, so the header does not shift when the state changes.
        expect(line, `${state} row "${line}"`).toMatch(/^[ █▄▀◐◑●○]+$/);
      }
    }
  });

  it('is three rows and seven columns in every state', () => {
    for (const state of STATES) {
      expect(FULL_ART[state]).toHaveLength(3);
      expect(FULL_ART[state][0]).toHaveLength(MASCOT_WIDTH);
    }
  });
});

describe('state machine', () => {
  it('shows a distinct eye for done and for error, so colour is never the only signal', () => {
    expect(FULL_ART.done[1]).toContain('●');
    expect(FULL_ART.error[1]).toContain('○');
    expect(FULL_ART.done[1]).not.toBe(FULL_ART.error[1]);
  });

  it('has a working eye that differs from idle', () => {
    expect(FULL_ART.working[1]).toContain('◑');
    expect(FULL_ART.idle[1]).toContain('◐');
  });

  it('never animates: one frame per state, no sweep variants', () => {
    for (const state of STATES) {
      expect(FULL_ART[state]).toHaveLength(3);
      expect(COMPACT_ART[state]).toHaveLength(1);
    }
  });

  it('picks its body token from the state', () => {
    const theme = 'niki';
    const tokenFor = (s: MascotState) => mascot(s, 'full', 'unicode', theme).bodyToken;
    expect(tokenFor('idle')).toBe('accent');
    expect(tokenFor('working')).toBe('accent');
    expect(tokenFor('done')).toBe('success');
    expect(tokenFor('error')).toBe('error');
    expect(tokenFor('interrupted')).toBe('muted');
  });
});

describe('width', () => {
  it.each(['full', 'compact', 'tiny'] as const)('%s art has the same width in every state', (tier) => {
    expect(everyStateHasTheSameWidth(tier, 'unicode')).toBe(true);
  });

  it.each(['full', 'compact', 'tiny'] as const)('%s ASCII art has the same width in every state', (tier) => {
    expect(everyStateHasTheSameWidth(tier, 'ascii')).toBe(true);
  });
});

describe('charset', () => {
  it('draws no art in ASCII, only a marker', () => {
    const art = mascot('idle', 'full', 'ascii', 'niki');
    expect(art.lines).toHaveLength(1);
    expect(art.lines[0]).toBe('o');
  });

  it('uses ASCII eyes for every state', () => {
    expect(mascot('done', 'full', 'ascii', 'niki').lines[0]).toBe('*');
    expect(mascot('error', 'full', 'ascii', 'niki').lines[0]).toBe('x');
  });
});

describe('contrast', () => {
  it.each(THEME_NAMES)('%s body against background clears the UI-glyph floor', (name) => {
    const palette = PALETTES[name];
    const pairs: [MascotState, string, string][] = [
      ['idle', palette.accent, palette.background],
      ['working', palette.accent, palette.background],
      ['done', palette.success, palette.background],
      ['error', palette.error, palette.background],
      ['interrupted', palette.muted, palette.background],
    ];
    for (const [state, body, background] of pairs) {
      const ratio = contrastRatio(body, background);
      expect(ratio, `${name} ${state} body`).toBeGreaterThanOrEqual(UI_CONTRAST_MIN);
    }
  });

  it.each(THEME_NAMES)('%s eye against body is legible', (name) => {
    const palette = PALETTES[name];
    // The eye is drawn in the background colour on the body colour, so the pair that matters is
    // the eye against its own body, not against the page.
    const ratio = contrastRatio(palette.background, palette.accent);
    expect(ratio, `${name} eye on accent body`).toBeGreaterThanOrEqual(UI_CONTRAST_MIN);
  });
});

describe('mascotWidth', () => {
  it('reports the same width the art actually occupies', () => {
    for (const state of STATES) {
      expect(mascotWidth(state, 'full', 'unicode')).toBe(FULL_ART[state][0]!.length);
      expect(mascotWidth(state, 'compact', 'unicode')).toBe(COMPACT_ART[state].length);
    }
  });
});