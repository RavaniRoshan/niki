/**
 * The `/` popup.
 *
 * It renders the filtered rows the registry produces and nothing else: there is no list in this
 * file. Ordinary typing keeps going to the composer while the popup is open, and the popup
 * re-filters on every keystroke because the query it filters by is the composer's own first word.
 *
 * A command that takes an argument says so on its row, so the argument hint is visible *before*
 * the user commits to the command rather than after.
 */

import React from 'react';
import { Box, Text } from 'ink';
import type { AppState } from '../state.js';
import { commandRows } from '../surfaces/commands.js';
import { paletteFor, type ThemeName } from '../theme/index.js';
import { glyphs, type Charset } from '../glyphs.js';
import { sanitizeSingleLine } from '../sanitize.js';
import { truncate } from './footer.js';

/** Rows shown at once. A popup that grows to twenty rows has stopped being a popup. */
export const SLASH_MENU_ROWS = 8;

export type SlashMenuProps = {
  readonly state: AppState;
  readonly theme: ThemeName;
  readonly charset: Charset;
};

export function SlashMenu({ state, theme, charset }: SlashMenuProps): React.ReactElement | null {
  const menu = state.slashMenu;
  if (!menu) return null;

  const c = paletteFor(theme);
  const g = glyphs(charset);
  const rows = commandRows(menu.query);
  const width = state.cols;

  if (rows.length === 0) {
    return (
      <Box flexDirection="column" width={width}>
        <Text color={c.muted}>{truncate(`  no command matches /${sanitizeSingleLine(menu.query)}`, width)}</Text>
      </Box>
    );
  }

  // The highlight follows the selection, and the window scrolls only when it has to.
  const first = Math.max(0, Math.min(menu.selected, rows.length - SLASH_MENU_ROWS));
  const visible = rows.slice(first, first + SLASH_MENU_ROWS);

  return (
    <Box flexDirection="column" width={width}>
      {visible.map((command, i) => {
        const selected = first + i === menu.selected;
        const name = sanitizeSingleLine(command.name);
        const hint = command.argumentHint ? ` ${sanitizeSingleLine(command.argumentHint)}` : '';
        const text = `${selected ? g.prompt : ' '} ${name}${hint}`;
        return (
          <Text key={command.name} color={selected ? c.accent : c.foreground} bold={selected}>
            {truncate(text, width)}
            <Text color={c.muted}>
              {truncate(
                `${hint ? '' : '  '}${sanitizeSingleLine(command.description)}`,
                Math.max(0, width - text.length),
              )}
            </Text>
          </Text>
        );
      })}
      {rows.length > SLASH_MENU_ROWS ? (
        <Text color={c.muted}>
          {truncate(`  ${rows.length} commands · up/down · tab completes · enter runs · esc closes`, width)}
        </Text>
      ) : null}
    </Box>
  );
}