/**
 * The approval prompt.
 *
 * Two rules from the spec, both enforced here rather than left to discipline:
 *  1. The focused option is always the one the engine named as safest, resolved in `state.ts`.
 *     It is never "the first" and never "Approve" in manual mode.
 *  2. Esc denies. There is no path where Esc closes the prompt and leaves the run undecided.
 */

import React from 'react';
import { Box, Text } from 'ink';
import type { AppState } from '../state.js';
import { paletteFor, type ThemeName } from '../theme/index.js';
import { glyphs, type Charset } from '../glyphs.js';
import { truncate } from './footer.js';

export type ApprovalProps = {
  readonly state: AppState;
  readonly theme: ThemeName;
  readonly charset: Charset;
};

export function Approval({ state, theme, charset }: ApprovalProps): React.ReactElement | null {
  const approval = state.approval;
  if (!approval) return null;
  const c = paletteFor(theme);
  const g = glyphs(charset);
  const { request, focusedOptionId } = approval;
  const width = state.cols;

  return (
    <Box flexDirection="column" width={width} borderStyle="single" borderColor={c.warning}>
      <Text color={c.warning} bold>
        {request.tool} wants to run
      </Text>
      <Text color={c.foreground}>{truncate(`  $ ${request.command}`, width - 4)}</Text>
      <Box flexDirection="column" marginTop={1}>
        {request.options.map((option) => {
          const focused = option.id === focusedOptionId;
          return (
            <Text key={option.id} color={focused ? c.accent : c.muted} bold={focused}>
              {focused ? `${g.done} ` : '  '}
              {option.label}
              {focused ? '  (default · esc denies)' : ''}
            </Text>
          );
        })}
      </Box>
    </Box>
  );
}