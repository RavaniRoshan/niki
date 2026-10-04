/**
 * The header: NIKI's orb mascot, then the three identity lines.
 *
 * Scrolling away with the transcript is the intent — the header is part of the conversation, not
 * a permanent chrome strip. At 80 columns and up it shows the full 7x3 art beside name and
 * version, model and permission posture, and short cwd with branch and ahead/behind. At 50-79 it
 * collapses to one row; below 50 there is no art at all.
 *
 * Every value comes from `session.ready`. A value the engine has not sent renders as nothing, not
 * as a placeholder, and a missing session renders an honest "connecting" row.
 */

import React from 'react';
import { Box, Text } from 'ink';
import type { AppState } from '../state.js';
import { mascot, mascotWidth, tierForWidth } from '../mascot.js';
import { paletteFor, type ThemeName } from '../theme/index.js';
import type { Charset } from '../glyphs.js';

export const APP_NAME = 'Niki';

export type MastheadProps = {
  readonly state: AppState;
  readonly theme: ThemeName;
  readonly charset: Charset;
  readonly version?: string;
};

export function Masthead({ state, theme, charset, version = '0.1.0' }: MastheadProps): React.ReactElement | null {
  const c = paletteFor(theme);
  const session = state.session;

  // The mascot state follows the real state machine and nothing else. There is no timer here and
  // no self-scheduled redraw: `done` appears only after `turn.end`, `error` only after a failure.
  const mascotState =
    state.protocolError !== null || state.phase === 'error'
      ? 'error'
      : state.interrupted
        ? 'interrupted'
        : state.phase === 'done'
          ? 'done'
          : state.phase === 'thinking' ||
              state.phase === 'streaming' ||
              state.phase === 'toolRunning' ||
              // Waiting on the user is still work in progress: the run has not finished.
              state.phase === 'awaitingApproval'
            ? 'working'
            : 'idle';

  const tier = tierForWidth(state.cols);
  const art = mascot(mascotState, tier, charset, theme);

  if (tier === 'compact' || tier === 'tiny') {
    const oneLine = [
      `${APP_NAME} ${version}`,
      session?.model,
      session?.permission_mode,
      session?.branch,
    ]
      .filter(Boolean)
      .join(' · ');
    return (
      <Box>
        <Text color={c[art.bodyToken as 'accent']}>{art.lines[0]}</Text>
        <Text> </Text>
        <Text color={c.foreground} bold>
          {APP_NAME}
        </Text>
        <Text color={c.muted}> {version}</Text>
        {oneLine ? <Text color={c.muted}> · {oneLine.split(` ${APP_NAME} ${version} `)[0]}</Text> : null}
      </Box>
    );
  }

  const branch = branchWithCounts(state);
  const posture = session?.permission_mode;

  return (
    <Box flexDirection="column">
      <Box>
        <Box flexDirection="column" width={mascotWidth(mascotState, tier, charset) + 1}>
          {art.lines.map((line, i) => (
            <Text key={i} color={c[art.bodyToken as 'accent']}>
              {line}
            </Text>
          ))}
        </Box>
        <Box flexDirection="column">
          <Box>
            <Text color={c.foreground} bold>
              {APP_NAME}
            </Text>
            <Text color={c.muted}> {version}</Text>
          </Box>
          <Box>
            {/* Both facts come from `session.ready`. Before it arrives there is no row at all. */}
            {session?.model ? <Text color={c.muted}>{session.model}</Text> : null}
            {session?.model && posture ? <Text color={c.muted}> · </Text> : null}
            {posture ? <Text color={c.secondary}>{posture}</Text> : null}
          </Box>
          <Box>
            {session?.project_path ? (
              <Text color={c.muted}>{shortPath(session.project_path, state.cols)}</Text>
            ) : (
              <Text color={c.muted}>connecting to the engine…</Text>
            )}
            {branch ? <Text color={c.secondary}> ({branch})</Text> : null}
          </Box>
        </Box>
      </Box>
    </Box>
  );
}

function branchWithCounts(state: AppState): string {
  const branch = state.session?.branch;
  if (!branch) return '';
  const ahead = state.session?.ahead;
  const behind = state.session?.behind;
  // No arrow until git actually reported the numbers.
  if (ahead === null || ahead === undefined) return branch;
  const parts = [branch];
  if (ahead > 0) parts.push(`↑${ahead}`);
  if (behind !== null && behind !== undefined && behind > 0) parts.push(`↓${behind}`);
  return parts.join(' ');
}

export function shortPath(path: string, cols: number): string {
  const home = process.env.HOME ?? '';
  let p = path;
  if (home && p.startsWith(home)) p = `~${p.slice(home.length)}`;
  const room = Math.max(8, Math.floor(cols / 2));
  if (p.length > room) return `…${p.slice(p.length - room + 1)}`;
  return p;
}