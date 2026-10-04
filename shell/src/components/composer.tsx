/**
 * The composer: the anchor of the whole interface.
 *
 * It never moves, never scrolls away, and stays live during a run — the user can type and queue
 * while output streams above it. Rule-based chrome: one thin rule above, one below, no box. That
 * is the NIKI choice recorded in the design note (Claude Code's rules, not Kimi's cards).
 *
 * Everything here is driven by one value, `state.composer`, plus `state.queue` for the indicator.
 */

import React from 'react';
import { Box, Text } from 'ink';
import type { AppState } from '../state.js';
import { paletteFor, type ThemeName } from '../theme/index.js';
import { glyphs, type Charset } from '../glyphs.js';
import { truncate } from './footer.js';

export type ComposerProps = {
  readonly state: AppState;
  readonly theme: ThemeName;
  readonly charset: Charset;
  readonly focused: boolean;
};

export function Composer({ state, theme, charset, focused }: ComposerProps): React.ReactElement {
  const c = paletteFor(theme);
  const g = glyphs(charset);
  const ruleColor = focused ? c.accent : c.muted;
  const width = state.cols;

  return (
    <Box flexDirection="column" width={width}>
      {/* Queued messages wait here, visibly, above the composer — never hidden. */}
      {state.queue.length > 0 ? (
        <Box flexDirection="column">
          {state.queue.slice(-3).map((q, i) => (
            <Text key={i} color={c.muted}>
              {truncate(`  ${g.queued} queued · ${q}`, width)}
            </Text>
          ))}
          {state.queue.length > 3 ? (
            <Text color={c.muted}>{truncate(`  … ${state.queue.length - 3} more queued`, width)}</Text>
          ) : null}
        </Box>
      ) : null}

      <Text color={ruleColor}>{g.rule.repeat(Math.max(0, width))}</Text>
      <Box>
        <Text color={focused ? c.accent : c.muted}>{g.prompt} </Text>
        <Text color={c.foreground}>{state.composer}</Text>
        {focused ? <Text color={c.accent}>▏</Text> : null}
      </Box>
      <Text color={ruleColor}>{g.rule.repeat(Math.max(0, width))}</Text>
    </Box>
  );
}

/** Placeholder shown when the composer is empty. Honest, and never a fabricated prompt. */
export function composerPlaceholder(phase: AppState['phase']): string {
  return phase === 'awaitingApproval'
    ? 'waiting for a decision above'
    : phase === 'idle'
      ? 'Describe a change, or / for commands'
      : 'Describe a change — it will queue';
}