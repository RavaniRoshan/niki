/**
 * The frame every overlay draws inside.
 *
 * One panel means one place that decides how a title, a query line, a list of rows and a hint line
 * are laid out, so the palette and the five pickers cannot drift into five slightly different
 * boxes. Rows are text from the registries; each one is sanitised before it is handed to a widget.
 */

import React from 'react';
import { Box, Text } from 'ink';
import type { AppState } from '../state.js';
import type { SurfaceItem } from '../surfaces/items.js';
import { paletteFor, type ThemeName } from '../theme/index.js';
import { glyphs, type Charset } from '../glyphs.js';
import { sanitizeSingleLine } from '../sanitize.js';
import { truncate } from './footer.js';

export type PanelProps = {
  readonly state: AppState;
  readonly theme: ThemeName;
  readonly charset: Charset;
  readonly title: string;
  readonly rows: readonly SurfaceItem[];
  readonly selected: number;
  /** Rows the panel may spend, title and hint line included. */
  readonly height?: number;
  /** The honest sentence shown when there is nothing to list. */
  readonly emptyNote?: string;
  readonly hints?: string;
  /** One list per group: the panel renders a heading when the group changes. */
  readonly showGroups?: boolean;
};

export function Panel({
  state,
  theme,
  charset,
  title,
  rows,
  selected,
  height,
  emptyNote = '',
  hints = '',
  showGroups = true,
}: PanelProps): React.ReactElement {
  const c = paletteFor(theme);
  const g = glyphs(charset);
  const width = state.cols;
  // The panel spends at most `height` rows: a title, a rule, the list and a hint line. The list is
  // windowed over *rendered cells*, not rows, because a group heading is a row too and a window
  // that forgot that would push the composer off the bottom of the screen.
  const budget = Math.max(1, (height ?? 24) - 3);
  const cells: { readonly heading: boolean; readonly row?: SurfaceItem }[] = [];
  rows.forEach((row, i) => {
    if (showGroups && (i === 0 || rows[i - 1]?.group !== row.group)) cells.push({ heading: true, row });
    cells.push({ heading: false, row });
  });
  const anchor = Math.max(0, cells.findIndex((cell) => !cell.heading && cell.row === rows[selected]));
  const first = Math.max(0, Math.min(anchor - budget + 1, Math.max(0, cells.length - budget)));
  const visible = cells.slice(first, first + budget);

  return (
    <Box flexDirection="column" width={width}>
      <Text color={c.accent} bold>
        {truncate(`${title}${state.overlayQuery ? ` ${sanitizeSingleLine(state.overlayQuery)}` : ''}`, width)}
      </Text>
      <Text color={c.muted}>{truncate(g.rule.repeat(Math.max(0, width)), width)}</Text>

      {visible.length === 0 ? (
        <Text color={c.muted}>{truncate(`  ${sanitizeSingleLine(emptyNote)}`, width)}</Text>
      ) : (
        visible.map((cell, i) => {
          const row = cell.row;
          if (cell.heading || !row) {
            return (
              <Text key={`h-${i}`} color={c.muted}>
                {truncate(`  ${sanitizeSingleLine(row?.group ?? '').toLowerCase()}`, width)}
              </Text>
            );
          }
          const isSelected = rows.indexOf(row) === selected;
          const marker = isSelected ? g.prompt : ' ';
          const titleText = truncate(`${marker} ${sanitizeSingleLine(row.title)}`, width);
          const detail = row.detail ? sanitizeSingleLine(row.detail) : '';
          return (
            <Text key={row.id} color={isSelected ? c.accent : c.foreground} bold={isSelected}>
              {titleText}
              <Text color={c.muted}>{truncate(`  ${detail}`, Math.max(0, width - titleText.length))}</Text>
            </Text>
          );
        })
      )}

      {hints ? <Text color={c.muted}>{truncate(`  ${hints}`, width)}</Text> : null}
    </Box>
  );
}

/** The hints every list overlay shares, derived from the keys the dispatcher really matches. */
export const LIST_HINTS = 'up/down moves · tab or enter accepts · esc closes';