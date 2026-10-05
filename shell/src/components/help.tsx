/**
 * Help, generated rather than written.
 *
 * The rows come from two registries and nothing else: `KEYMAP` in `src/dispatch.ts` for the keys,
 * and `COMMANDS` in `src/components/footer.tsx` for the commands. The owner's rule — "nothing is
 * advertised that a test has not exercised" — is why there is no hand-written key list here: a key
 * that the dispatcher stops matching disappears from Help the moment its keymap row does, and
 * `test/commands.test.tsx` fails when a row stops describing the real dispatcher.
 *
 * It is searchable with the same keys as every other overlay, and it scrolls with Up/Down and
 * PgUp/PgDn because it is longer than most terminals.
 */

import React from 'react';
import { Box, Text } from 'ink';
import type { AppState } from '../state.js';
import { KEYMAP } from '../dispatch.js';
import { COMMANDS, type BypassTier, worksWhileRunning } from './footer.js';
import { fuzzyScore } from '../surfaces/fuzzy.js';
import { paletteFor, type ThemeName } from '../theme/index.js';
import type { Charset } from '../glyphs.js';
import { truncate } from './footer.js';

export type HelpLine = {
  readonly text: string;
  readonly kind: 'heading' | 'key' | 'command' | 'note';
};

const TIER_LABEL: Readonly<Record<BypassTier, string>> = {
  always: 'always',
  immediateUi: 'immediate ui',
  sideEffectFree: 'no side effects',
  queued: 'queued',
};

/** One line of context about the state the help is being read in. Never invented. */
function contextLine(state: AppState): string {
  const posture = state.session?.permission_mode ?? state.permissionOverride ?? 'not reported';
  const running = state.activity ? 'a run is active' : 'no run is active';
  return `phase ${state.phase} · ${running} · permission posture ${posture}`;
}

function matches(query: string, ...fields: string[]): boolean {
  if (query.trim() === '') return true;
  return fields.some((field) => fuzzyScore(query, field) !== null);
}

/** The whole of Help as plain lines, so a test can assert on content without rendering a frame. */
export function helpLines(state: AppState): readonly HelpLine[] {
  const query = state.overlayQuery;
  const lines: HelpLine[] = [{ text: contextLine(state), kind: 'note' }, { text: 'keys', kind: 'heading' }];

  for (const binding of KEYMAP) {
    if (!matches(query, binding.keys, binding.label, binding.action, binding.when)) continue;
    lines.push({ text: `${binding.keys}  ${binding.label}  (${binding.when})`, kind: 'key' });
  }

  lines.push({ text: 'commands', kind: 'heading' });
  for (const command of COMMANDS) {
    const haystack = [command.name, ...command.aliases, command.description, ...command.keywords];
    if (!matches(query, ...haystack)) continue;
    const argument = command.argumentHint ? ` ${command.argumentHint}` : '';
    const aliases = command.aliases.length > 0 ? ` (${command.aliases.join(', ')})` : '';
    const tier = worksWhileRunning(command.tier) ? '' : ' · queued while a run is active';
    lines.push({
      text: `${command.name}${aliases}${argument}  ${command.description} · ${TIER_LABEL[command.tier]}${tier}`,
      kind: 'command',
    });
  }

  if (query.trim() !== '' && lines.length === 2) {
    lines.push({ text: 'nothing matches', kind: 'note' });
  }
  return lines;
}

export type HelpProps = {
  readonly state: AppState;
  readonly theme: ThemeName;
  readonly charset: Charset;
  readonly height?: number;
};

export function Help({ state, theme, height }: HelpProps): React.ReactElement | null {
  if (state.overlay !== 'help') return null;
  const c = paletteFor(theme);
  const width = state.cols;
  const lines = helpLines(state);

  // The panel takes a fixed slice of the document; scrolling moves the slice rather than the
  // document, so a long registry costs the same to draw as a short one.
  const budget = Math.max(1, (height ?? 12) - 2);
  const maxScroll = Math.max(0, lines.length - budget);
  const scroll = Math.min(state.overlayScroll, maxScroll);
  const visible = lines.slice(scroll, scroll + budget);

  return (
    <Box flexDirection="column" width={width}>
      <Text color={c.accent} bold>
        {truncate(`help${state.overlayQuery ? ` ${state.overlayQuery}` : ''}`, width)}
      </Text>
      {visible.map((line, i) => (
        <Text
          key={i}
          color={line.kind === 'heading' ? c.secondary : line.kind === 'note' ? c.muted : c.foreground}
          bold={line.kind === 'heading'}
        >
          {truncate(line.kind === 'key' ? `  ${line.text}` : line.text, width)}
        </Text>
      ))}
      <Text color={c.muted}>
        {truncate(
          `  ${scroll + visible.length}/${lines.length} · type to search · up/down scrolls · esc closes`,
          width,
        )}
      </Text>
    </Box>
  );
}