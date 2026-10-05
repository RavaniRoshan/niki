/**
 * The command palette: Ctrl+K over everything the shell can do right now.
 *
 * Its rows come from `paletteItems(state)`, which reads the command registry, the settings
 * registry, the themes and whatever the engine has actually reported. There is no list in this
 * file, and nothing here reaches the engine.
 */

import React from 'react';
import type { AppState } from '../state.js';
import { filterItems, overlayEmptyNote, overlayItems } from '../surfaces/items.js';
import type { ThemeName } from '../theme/index.js';
import type { Charset } from '../glyphs.js';
import { Panel, LIST_HINTS } from './panel.js';

export type PaletteProps = {
  readonly state: AppState;
  readonly theme: ThemeName;
  readonly charset: Charset;
  readonly height?: number;
};

export function Palette({ state, theme, charset, height }: PaletteProps): React.ReactElement | null {
  if (state.overlay !== 'palette') return null;
  const rows = filterItems(state.overlayQuery, overlayItems(state));
  const selected = Math.min(state.overlayIndex, Math.max(0, rows.length - 1));
  return (
    <Panel
      state={state}
      theme={theme}
      charset={charset}
      title="everything"
      rows={rows}
      selected={selected}
      emptyNote={overlayEmptyNote(state, 'palette')}
      height={height}
      hints={LIST_HINTS}
    />
  );
}