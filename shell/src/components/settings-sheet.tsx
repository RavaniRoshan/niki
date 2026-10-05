/**
 * The settings sheet.
 *
 * Three rules from the row it exists for:
 *
 *  - **Every value says where it came from.** A value the engine reported says "engine session";
 *    a value nobody has reported says "not reported" rather than showing a plausible blank.
 *  - **Saving shows exactly what changed and where**, one row per change, naming the file or env
 *    var that would have to change. Nothing is written — protocol v1 has no settings request — and
 *    the sheet says that rather than implying a save landed somewhere.
 *  - **A safety-critical change cannot be applied by a keystroke.** It is held for the
 *    confirmation overlay, and it is never a default.
 */

import React from 'react';
import { Box, Text } from 'ink';
import type { AppState } from '../state.js';
import { resolveSettings } from '../surfaces/settings.js';
import { paletteFor, type ThemeName } from '../theme/index.js';
import { glyphs, type Charset } from '../glyphs.js';
import { truncate } from './footer.js';

export type SettingsSheetProps = {
  readonly state: AppState;
  readonly theme: ThemeName;
  readonly charset: Charset;
  readonly height?: number;
};

function sourceText(source: ReturnType<typeof resolveSettings>[number]['source']): string {
  if (source.kind === 'engine') return source.detail;
  if (source.kind === 'config') {
    return source.env === undefined ? source.path : `${source.path} or ${source.env}`;
  }
  return 'not reported';
}

export function SettingsSheet({ state, theme, charset, height }: SettingsSheetProps): React.ReactElement | null {
  if (state.overlay !== 'settings') return null;
  const c = paletteFor(theme);
  const g = glyphs(charset);
  const width = state.cols;
  const rows = resolveSettings(state);
  const focused = Math.min(state.overlayIndex, Math.max(0, rows.length - 1));

  // The sheet scrolls with the focus rather than growing: ten rows, four group headings, a report
  // and two hint lines have to fit the same twenty-four rows as everything else. The window is over
  // *cells* — a heading and the focused row's destination line are rows too.
  const body: React.ReactElement[] = [];
  const cells: {
    readonly kind: 'group' | 'row' | 'destination';
    readonly row?: (typeof rows)[number];
  }[] = [];
  rows.forEach((row, i) => {
    if (i === 0 || rows[i - 1]?.spec.group !== row.spec.group) cells.push({ kind: 'group', row });
    cells.push({ kind: 'row', row });
    // The destination only fits on its own line, and only for the row being read. Showing all ten
    // at once is a wall of file paths nobody reads.
    if (i === focused) cells.push({ kind: 'destination', row });
  });

  const budget = Math.max(3, (height ?? 12) - 5);
  const anchor = Math.max(0, cells.findIndex((cell) => cell.kind === 'row' && cell.row === rows[focused]));
  const first = Math.max(0, Math.min(anchor - budget + 1, Math.max(0, cells.length - budget)));

  cells.slice(first, first + budget).forEach((cell, i) => {
    const row = cell.row;
    if (!row) return;
    if (cell.kind === 'group') {
      body.push(
        <Text key={`group-${i}`} color={c.secondary} bold>
          {truncate(row.spec.group, width)}
        </Text>,
      );
      return;
    }
    if (cell.kind === 'destination') {
      body.push(
        <Text key={`where-${i}`} color={c.muted}>
          {truncate(
            `    in ${row.spec.config}${row.spec.env ? ` or ${row.spec.env}` : ''}${
              row.staged !== undefined ? ' · staged' : ''
            }`,
            width,
          )}
        </Text>,
      );
      return;
    }
    const selected = row.spec.key === rows[focused]?.spec.key;
    const value = row.staged ?? row.value ?? 'not reported';
    const marker = selected ? g.prompt : ' ';
    body.push(
      <Text key={row.spec.key} color={selected ? c.accent : c.foreground} bold={selected}>
        {truncate(`${marker} ${row.spec.label}  ${value}`, width)}
        <Text color={c.muted}>
          {truncate(
            `  ${sourceText(row.source)}${row.spec.safety === 'confirm' ? ' · confirm to apply' : ''}`,
            Math.max(0, width - `${marker} ${row.spec.label}  ${value}`.length),
          )}
        </Text>
      </Text>,
    );
  });

  return (
    <Box flexDirection="column" width={width}>
      <Text color={c.accent} bold>
        {truncate(`settings${state.overlayQuery ? ` ${state.overlayQuery}` : ''}`, width)}
      </Text>
      {body}

      <Text color={c.secondary} bold>
        {truncate('what a save would change', width)}
      </Text>
      {state.settingsReport.length === 0 ? (
        <Text color={c.muted}>{truncate('  nothing changed yet', width)}</Text>
      ) : (
        state.settingsReport.map((change) => (
          <Text key={`${change.key}-${change.to}`} color={c.warning}>
            {truncate(
              `  ${change.label}: ${change.from ?? 'unset'} → ${change.to}  in ${change.destination}`,
              width,
            )}
          </Text>
        ))
      )}
      <Text color={c.muted}>
        {truncate('  nothing is written: protocol v1 has no settings request', width)}
      </Text>
      <Text color={c.muted}>
        {truncate('  up/down moves · left/right changes · enter saves · esc closes', width)}
      </Text>
    </Box>
  );
}