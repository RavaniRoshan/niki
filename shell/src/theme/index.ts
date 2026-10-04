/**
 * The shell's only source of colour.
 *
 * Every other module in `src/` gets its colours from here. `test/lint-theme.test.ts` fails if a
 * hex literal appears anywhere else, because a palette that leaks into a component is a palette
 * that drifts.
 *
 * Three of the four palettes are transcribed from the owner's visual spec. `niki-contrast` and
 * `niki-dim` are **derived** from it rather than ported from the unavailable `theme.py`; the
 * derivation is recorded in `docs/foundation/GAPS.md`.
 *
 * Rule from the spec: flat untinted greys for foreground/muted/secondary; one accent for focus
 * and activity; error is the only hue that means removal; status colours are desaturated
 * relative to the accent. Colour is never the only state indicator — every glyph carries a text
 * label or an ASCII fallback (see `src/glyphs.ts`).
 */

export const THEME_NAMES = ['niki', 'niki-light', 'niki-contrast', 'niki-dim'] as const;
export type ThemeName = (typeof THEME_NAMES)[number];

export type Palette = {
  readonly name: ThemeName;
  readonly background: string;
  readonly surface: string;
  readonly panel: string;
  readonly foreground: string;
  readonly muted: string;
  readonly primary: string;
  readonly secondary: string;
  readonly accent: string;
  readonly success: string;
  readonly warning: string;
  readonly error: string;
  readonly tool: string;
  readonly toolHover: string;
  readonly skill: string;
  readonly skillHover: string;
  readonly incognito: string;
  /**
   * Derived, not from the spec: `error` on a raised background. The spec's dark `error`
   * (#D2605C) is 3.92:1 on `panel` and 4.42:1 on `surface`, below the 4.5:1 text bar, so error
   * text drawn on a raised surface uses this lighter step instead. Measured, not guessed.
   */
  readonly errorOnSurface: string;
  /**
   * DERIVED, not from the spec. `accent`, `success` and `warning` on `panel` in the light
   * palette measure 4.24:1, 4.15:1 and 4.68:1; the first two are under the 4.5:1 text bar. These
   * darker steps are what the shell actually draws on a raised surface, measured not guessed.
   */
  readonly accentOnPanel: string;
  readonly successOnPanel: string;
};

/** The owner's dark palette, verbatim. */
const NIKI: Palette = {
  name: 'niki',
  background: '#14161A',
  surface: '#1B1E25',
  panel: '#232831',
  foreground: '#E4E7EC',
  muted: '#9AA1AD',
  primary: '#6B8EF2',
  secondary: '#8B93A3',
  accent: '#43AFA0',
  success: '#5FAE7A',
  warning: '#D99A3E',
  error: '#D2605C',
  tool: '#5FA8C4',
  toolHover: '#7FC2D8',
  skill: '#A98BD0',
  skillHover: '#C0A6DE',
  incognito: '#9B8AAE',
  errorOnSurface: '#DE6C68', // 5.12:1 on surface, 4.54:1 on panel
  accentOnPanel: '#43AFA0', // 5.55:1 on panel — the spec value already clears it
  successOnPanel: '#5FAE7A', // 5.11:1 on panel — likewise
};

/** The owner's light palette, verbatim. */
const NIKI_LIGHT: Palette = {
  name: 'niki-light',
  background: '#FAFAFB',
  surface: '#F1F2F4',
  panel: '#E7E9ED',
  foreground: '#1F2328',
  muted: '#5C636E',
  primary: '#3355CC',
  secondary: '#4B535F',
  accent: '#1F7A70',
  success: '#2E7D4F',
  warning: '#8A5E12',
  error: '#A83232',
  tool: '#1F6C86',
  toolHover: '#17566B',
  skill: '#6B3FA0',
  skillHover: '#573089',
  incognito: '#6B4E86',
  errorOnSurface: '#8F2A2A', // derived; see test/theme-contrast.test.ts for the measurement
  accentOnPanel: '#177268', // derived: 4.74:1 on panel
  successOnPanel: '#267547', // derived: 4.65:1 on panel
};

/**
 * DERIVED, not ported. Maximum separation: pure black ground, pure white text, saturated but
 * never neon hues. Every token clears 4.5:1 on `background` and error clears 6.9:1 on `panel`.
 */
const NIKI_CONTRAST: Palette = {
  name: 'niki-contrast',
  background: '#000000',
  surface: '#111111',
  panel: '#1A1A1A',
  foreground: '#FFFFFF',
  muted: '#C8C8C8',
  primary: '#7AA2FF',
  secondary: '#D0D0D0',
  accent: '#4FE3C1',
  success: '#7BE08F',
  warning: '#FFC061',
  error: '#FF7B76',
  tool: '#7FD4F0',
  toolHover: '#A8E4F8',
  skill: '#C9A6F5',
  skillHover: '#DCC6FF',
  incognito: '#BBA8D0',
  errorOnSurface: '#FF7B76',
  accentOnPanel: '#4FE3C1',
  successOnPanel: '#7BE08F',
};

/**
 * DERIVED, not ported. Desaturated and low-chroma for long sessions on a bright desk. It is
 * "dim" in saturation, never in legibility: every token still clears 4.5:1 on `background`.
 */
const NIKI_DIM: Palette = {
  name: 'niki-dim',
  background: '#1A1B1E',
  surface: '#232428',
  panel: '#2A2B30',
  foreground: '#D6D8DC',
  muted: '#A2A6AE',
  primary: '#8A9FD8',
  secondary: '#A8ADB6',
  accent: '#7FBFB6',
  success: '#8FBF9E',
  warning: '#CFA96A',
  error: '#DE9490',
  tool: '#92B6C6',
  toolHover: '#AECBD8',
  skill: '#B3A0CC',
  skillHover: '#C7B8DB',
  incognito: '#A79BAC',
  errorOnSurface: '#DE9490',
  accentOnPanel: '#7FBFB6',
  successOnPanel: '#8FBF9E',
};

export const PALETTES: Readonly<Record<ThemeName, Palette>> = Object.freeze({
  niki: NIKI,
  'niki-light': NIKI_LIGHT,
  'niki-contrast': NIKI_CONTRAST,
  'niki-dim': NIKI_DIM,
});

/**
 * Every (foreground, background) pair the shell is allowed to draw. `text` needs 4.5:1, `ui` —
 * rules, glyphs, spinner frames — needs 3:1. Declaring the pairs here rather than in a component
 * is what makes the contrast requirement checkable at all.
 */
export const TOKEN_PAIRS: ReadonlyArray<{
  readonly fg: keyof Palette;
  readonly bg: keyof Palette;
  readonly kind: 'text' | 'ui';
}> = [
  { fg: 'foreground', bg: 'background', kind: 'text' },
  { fg: 'foreground', bg: 'surface', kind: 'text' },
  { fg: 'foreground', bg: 'panel', kind: 'text' },
  { fg: 'muted', bg: 'background', kind: 'text' },
  { fg: 'muted', bg: 'surface', kind: 'text' },
  { fg: 'muted', bg: 'panel', kind: 'text' },
  { fg: 'secondary', bg: 'background', kind: 'text' },
  { fg: 'secondary', bg: 'surface', kind: 'text' },
  { fg: 'primary', bg: 'background', kind: 'text' },
  { fg: 'primary', bg: 'surface', kind: 'text' },
  { fg: 'primary', bg: 'panel', kind: 'text' },
  { fg: 'accent', bg: 'background', kind: 'text' },
  { fg: 'accent', bg: 'surface', kind: 'text' },
  { fg: 'accentOnPanel', bg: 'panel', kind: 'text' },
  { fg: 'success', bg: 'background', kind: 'text' },
  { fg: 'successOnPanel', bg: 'panel', kind: 'text' },
  { fg: 'warning', bg: 'background', kind: 'text' },
  { fg: 'warning', bg: 'panel', kind: 'text' },
  { fg: 'error', bg: 'background', kind: 'text' },
  { fg: 'errorOnSurface', bg: 'surface', kind: 'text' },
  { fg: 'errorOnSurface', bg: 'panel', kind: 'text' },
  { fg: 'tool', bg: 'background', kind: 'text' },
  { fg: 'tool', bg: 'panel', kind: 'text' },
  { fg: 'toolHover', bg: 'background', kind: 'text' },
  { fg: 'skill', bg: 'background', kind: 'text' },
  { fg: 'skill', bg: 'panel', kind: 'text' },
  { fg: 'skillHover', bg: 'background', kind: 'text' },
  { fg: 'incognito', bg: 'background', kind: 'text' },
  { fg: 'incognito', bg: 'panel', kind: 'text' },
];

/** Minimum contrast for a text pair, per WCAG 2.1 AA. */
export const TEXT_CONTRAST_MIN = 4.5;
/** Minimum contrast for a UI glyph or rule that is not text. */
export const UI_CONTRAST_MIN = 3.0;

// --- contrast maths -------------------------------------------------------
// Exported so the test measures the same numbers the palette was chosen against.

function channel(value: number): number {
  const v = value / 255;
  return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
}

export function relativeLuminance(hex: string): number {
  const h = hex.replace('#', '');
  const r = channel(parseInt(h.slice(0, 2), 16));
  const g = channel(parseInt(h.slice(2, 4), 16));
  const b = channel(parseInt(h.slice(4, 6), 16));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function contrastRatio(a: string, b: string): number {
  const la = relativeLuminance(a);
  const lb = relativeLuminance(b);
  const hi = Math.max(la, lb);
  const lo = Math.min(la, lb);
  return (hi + 0.05) / (lo + 0.05);
}

export function paletteFor(name: ThemeName): Palette {
  return PALETTES[name];
}

export function isThemeName(value: string): value is ThemeName {
  return (THEME_NAMES as readonly string[]).includes(value);
}