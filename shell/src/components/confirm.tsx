/**
 * The confirmation that gates a safety-critical change.
 *
 * Bypass and the permission posture do not get applied by a keystroke. This panel is the only way
 * past them, it names the value it is about to stage and the file that would have to change, and
 * Esc cancels — Esc always cancels, never confirms.
 */

import React from 'react';
import { Box, Text } from 'ink';
import type { AppState } from '../state.js';
import { paletteFor, type ThemeName } from '../theme/index.js';
import type { Charset } from '../glyphs.js';
import { truncate } from './footer.js';

export type ConfirmProps = {
  readonly state: AppState;
  readonly theme: ThemeName;
  readonly charset: Charset;
};

export function Confirm({ state, theme }: ConfirmProps): React.ReactElement | null {
  const pending = state.confirm;
  if (!pending) return null;
  const c = paletteFor(theme);
  const width = state.cols;
  return (
    <Box flexDirection="column" width={width}>
      <Text color={c.warning} bold>
        {truncate(`confirm · ${pending.label}`, width)}
      </Text>
      <Text color={c.foreground}>
        {truncate(`${pending.from ?? 'unset'} → ${pending.value}`, width)}
      </Text>
      <Text color={c.muted}>{truncate(`would be written to ${pending.destination}`, width)}</Text>
      <Text color={c.muted}>{truncate('  enter confirms · esc cancels', width)}</Text>
    </Box>
  );
}