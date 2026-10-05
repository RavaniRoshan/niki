/**
 * The pickers: model, effort, theme, sessions and prompt history.
 *
 * One component draws all five because they are one control: the same fuzzy query, the same
 * Up/Down/Tab/Enter/Esc keys, the same frame. What differs is which rows `overlayItems` offers, and
 * that list is derived — a picker shows the models the engine reported, and when the engine
 * reported none it says so instead of showing a plausible-looking model nobody named.
 *
 * The theme picker previews on move and restores on cancel. The preview lives in the state, not in
 * a component, because rendering is a pure function of state: `App` picks the palette up from
 * `state.activeTheme` and closing the overlay puts it back.
 */

import React from 'react';
import type { AppState, OverlayName } from '../state.js';
import { filterItems, overlayEmptyNote, overlayItems } from '../surfaces/items.js';
import type { ThemeName } from '../theme/index.js';
import type { Charset } from '../glyphs.js';
import { Panel, LIST_HINTS } from './panel.js';

/** The title each picker opens with, and the picker names are the registry's own. */
const TITLES: Readonly<Record<string, string>> = {
  model: 'model',
  effort: 'effort',
  theme: 'theme',
  sessions: 'sessions',
  history: 'prompt history',
};

const PREVIEWING = 'theme';

export function Picker({ state, theme, charset, height }: PickerProps): React.ReactElement | null {
  const overlay = state.overlay;
  if (!overlay || !(overlay in TITLES)) return null;

  const rows = filterItems(state.overlayQuery, overlayItems(state));
  const selected = Math.min(state.overlayIndex, Math.max(0, rows.length - 1));
  const previewing = overlay === PREVIEWING;
  const hints = previewing
    ? `${LIST_HINTS} · the theme follows the selection and is restored on esc`
    : LIST_HINTS;

  return (
    <Panel
      state={state}
      theme={theme}
      charset={charset}
      title={TITLES[overlay] ?? 'pick'}
      rows={rows}
      selected={selected}
      emptyNote={overlayEmptyNote(state, overlay as OverlayName)}
      height={height}
      hints={hints}
      showGroups={false}
    />
  );
}

export type PickerProps = {
  readonly state: AppState;
  readonly theme: ThemeName;
  readonly charset: Charset;
  readonly height?: number;
};