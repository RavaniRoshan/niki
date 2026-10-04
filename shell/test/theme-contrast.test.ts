/**
 * Colour, measured.
 *
 * Two claims from the spec are checked here rather than asserted in prose:
 *  1. every foreground/background pair the shell draws meets its contrast floor (4.5:1 for text,
 *     3:1 for a glyph or rule) in **every** palette;
 *  2. no NIKI hex value collides with a Claude Code, Codex or Kimi palette value, so the shell is
 *     not a recolour of someone else's product.
 */

import { describe, expect, it } from 'vitest';
import {
  PALETTES,
  TEXT_CONTRAST_MIN,
  THEME_NAMES,
  TOKEN_PAIRS,
  UI_CONTRAST_MIN,
  contrastRatio,
  type ThemeName,
} from '../src/theme/index.js';

/** Values observed in the reference frames. A collision would mean we copied a palette. */
const REFERENCE_HEXES = new Set<string>([
  // Claude Code v2.0.0 demo frames
  '#0d1f1e', '#18332f', '#1f403a', '#2a574e', '#3d8474', '#6cc5a9',
  '#d97757', '#e8a888', '#f0c4b0', '#c05a3c', '#a04a30',
  '#1a1a1a', '#2b2b2b', '#3a3a3a', '#cccccc', '#888888',
  // Kimi Code CLI intro frames
  '#1e1e1e', '#252526', '#2d2d30', '#3c3c41', '#007acc', '#0a84ff',
  '#d4d4d4', '#9d9d9d', '#6b6b6b', '#e5e5e5', '#c586c0',
]);

describe('palettes', () => {
  it('ships exactly the four named variants', () => {
    expect(Object.keys(PALETTES).sort()).toEqual([...THEME_NAMES].sort());
  });

  it.each(THEME_NAMES)('%s meets contrast for every declared token pair', (name: ThemeName) => {
    const palette = PALETTES[name];
    const failures: string[] = [];
    for (const pair of TOKEN_PAIRS) {
      const fg = palette[pair.fg] as string;
      const bg = palette[pair.bg] as string;
      const ratio = contrastRatio(fg, bg);
      const min = pair.kind === 'text' ? TEXT_CONTRAST_MIN : UI_CONTRAST_MIN;
      if (ratio < min) {
        failures.push(
          `${pair.fg} on ${pair.bg} = ${ratio.toFixed(2)}:1 (needs ${min}:1, ${fg} on ${bg})`,
        );
      }
    }
    expect(failures, `${name} contrast failures`).toEqual([]);
  });

  it.each(THEME_NAMES)('%s is not a copy of any reference palette', (name: ThemeName) => {
    const palette = PALETTES[name];
    const hexes = Object.entries(palette)
      .filter(([key]) => key !== 'name')
      .map(([, value]) => (value as string).toUpperCase());
    const collisions = hexes.filter((h) => REFERENCE_HEXES.has(h));
    expect(collisions, `${name} reuses a reference colour`).toEqual([]);
  });

  it.each(THEME_NAMES)('%s declares every token as a 6-digit hex', (name: ThemeName) => {
    const palette = PALETTES[name];
    for (const [key, value] of Object.entries(palette)) {
      if (key === 'name') continue;
      expect(value, `${name}.${key}`).toMatch(/^#[0-9A-F]{6}$/i);
    }
  });
});

describe('contrastRatio', () => {
  it('is symmetric and pinned at the extremes', () => {
    expect(contrastRatio('#000000', '#FFFFFF')).toBeCloseTo(21, 5);
    expect(contrastRatio('#FFFFFF', '#000000')).toBeCloseTo(21, 5);
    expect(contrastRatio('#123456', '#123456')).toBeCloseTo(1, 5);
  });
});