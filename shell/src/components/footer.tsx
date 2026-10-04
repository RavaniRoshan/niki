/**
 * The footer, and the single registry that feeds it.
 *
 * The footer, the in-app help and `--help` all read {@link hintsFor} and {@link COMMANDS}. A hint
 * that is not in this file is not shown anywhere, so the interface cannot advertise a key that
 * dispatches nothing. Checklist row D8.
 *
 * Collapse order when the terminal is narrow, straight from the spec: right hints first, then
 * cwd, then branch, then model. The permission posture is never dropped — it is the one ambient
 * fact a user must always be able to see.
 */

import React from 'react';
import { Box, Text } from 'ink';
import type { AppState, Phase } from '../state.js';
import { paletteFor, type ThemeName } from '../theme/index.js';
import { glyphs, type Charset } from '../glyphs.js';

export type Hint = { readonly key: string; readonly label: string };

/** The contextual hints, per state. Only these five sets exist; anything else is a bug. */
const HINTS: Readonly<Record<Phase, readonly Hint[]>> = {
  booting: [{ key: '', label: 'connecting to the engine' }],
  idle: [
    { key: '/', label: 'commands' },
    { key: '?', label: 'help' },
    { key: 'ctrl+k', label: 'everything' },
  ],
  composing: [
    { key: '/', label: 'commands' },
    { key: '?', label: 'help' },
    { key: 'ctrl+k', label: 'everything' },
  ],
  thinking: [
    { key: 'esc', label: 'interrupt' },
    { key: 'ctrl+o', label: 'details' },
    { key: '?', label: 'help' },
  ],
  streaming: [
    { key: 'esc', label: 'interrupt' },
    { key: 'ctrl+o', label: 'details' },
    { key: '?', label: 'help' },
  ],
  toolRunning: [
    { key: 'esc', label: 'interrupt' },
    { key: 'ctrl+o', label: 'details' },
    { key: '?', label: 'help' },
  ],
  awaitingApproval: [
    { key: '↑↓', label: 'choose' },
    { key: 'enter', label: 'confirm' },
    { key: 'esc', label: 'deny' },
  ],
  error: [
    { key: 'esc', label: 'dismiss' },
    { key: '?', label: 'help' },
  ],
  interrupted: [
    { key: 'type', label: 'to continue' },
    { key: '?', label: 'help' },
  ],
  done: [
    { key: '/', label: 'commands' },
    { key: '?', label: 'help' },
    { key: 'ctrl+k', label: 'everything' },
  ],
};

/** Hints while an overlay owns the screen. */
export const SEARCH_HINTS: readonly Hint[] = [
  { key: 'enter', label: 'select' },
  { key: 'esc', label: 'close' },
];

export function hintsFor(phase: Phase, overlayOpen = false): readonly Hint[] {
  return overlayOpen ? SEARCH_HINTS : HINTS[phase];
}

/** `/commands` etc. Each entry is declared once and reused by the popup, help and palette. */
export type BypassTier = 'always' | 'immediateUi' | 'sideEffectFree' | 'queued';

export type Command = {
  readonly name: string;
  readonly description: string;
  readonly aliases: readonly string[];
  /** Fuzzy keywords that do not appear in the name, e.g. "money" for /cost. */
  readonly keywords: readonly string[];
  readonly argumentHint?: string;
  /** Everything except `queued` works while a run is active. */
  readonly tier: BypassTier;
};

export const COMMANDS: readonly Command[] = [
  { name: '/help', description: 'Show this help', aliases: ['/?'], keywords: ['keys'], tier: 'always' },
  { name: '/model', description: 'Choose the model', aliases: [], keywords: ['llm', 'switch'], tier: 'always' },
  { name: '/theme', description: 'Choose the theme', aliases: [], keywords: ['colour', 'color', 'palette'], tier: 'always' },
  { name: '/clear', description: 'Clear the transcript', aliases: [], keywords: ['reset', 'wipe'], tier: 'always' },
  { name: '/copy', description: 'Copy the last response', aliases: [], keywords: ['clipboard'], tier: 'sideEffectFree' },
  { name: '/context', description: 'Show context usage', aliases: [], keywords: ['tokens', 'window'], tier: 'sideEffectFree' },
  { name: '/cost', description: 'Show spend for this run', aliases: [], keywords: ['money', 'usd', 'spend'], tier: 'sideEffectFree' },
  { name: '/tokens', description: 'Show token counts', aliases: [], keywords: ['usage'], tier: 'sideEffectFree' },
  { name: '/effort', description: 'Choose reasoning effort', aliases: [], keywords: ['thinking'], tier: 'always' },
  { name: '/threads', description: 'Sessions and resume', aliases: ['/sessions'], keywords: ['history', 'resume'], tier: 'always' },
  { name: '/prompts', description: 'Search prompt history', aliases: [], keywords: ['history', 'recall'], tier: 'always' },
  { name: '/editor', description: 'Edit the prompt in $EDITOR', aliases: [], keywords: ['external'], tier: 'always' },
  { name: '/version', description: 'Show the version', aliases: [], keywords: ['about'], tier: 'always' },
  { name: '/reload', description: 'Reconnect to the engine', aliases: [], keywords: ['reconnect'], tier: 'always' },
  { name: '/quit', description: 'Quit', aliases: ['/q'], keywords: ['exit', 'bye'], tier: 'always' },
  { name: '/auto', description: 'Permission mode: auto', aliases: [], keywords: ['permissions'], tier: 'always' },
  { name: '/manual', description: 'Permission mode: manual', aliases: [], keywords: ['permissions'], tier: 'always' },
  { name: '/yolo', description: 'Permission mode: bypass (asks first)', aliases: [], keywords: ['permissions', 'bypass'], tier: 'always' },
  { name: '/scrollbar', description: 'Toggle the scrollbar', aliases: [], keywords: ['scroll'], tier: 'sideEffectFree' },
  { name: '/timestamps', description: 'Toggle timestamps', aliases: [], keywords: ['time', 'clock'], tier: 'sideEffectFree' },
  { name: '/line-numbers', description: 'Toggle diff line numbers', aliases: [], keywords: ['diff', 'numbers'], tier: 'sideEffectFree' },
];

/** Commands that must work while a run is in flight. */
export function worksWhileRunning(tier: BypassTier): boolean {
  return tier !== 'queued';
}

export function formatHintList(hints: readonly Hint[]): string {
  return hints.map((h) => (h.key ? `${h.key} ${h.label}` : h.label)).join(' · ');
}

function shortCwd(path: string | undefined, cols: number): string {
  if (!path) return '';
  const home = process.env.HOME ?? '';
  let p = path;
  if (home && p.startsWith(home)) p = `~${p.slice(home.length)}`;
  if (p.length > cols) return `…${p.slice(p.length - cols + 1)}`;
  return p;
}

export type FooterProps = {
  readonly state: AppState;
  readonly theme: ThemeName;
  readonly charset: Charset;
  readonly overlayOpen?: boolean;
};

export function Footer({ state, theme, charset, overlayOpen = false }: FooterProps): React.ReactElement {
  const c = paletteFor(theme);
  const g = glyphs(charset);
  const session = state.session;
  const permission = session?.permission_mode ?? 'unknown';

  const pieces = {
    model: session?.model,
    posture: permission,
    branch: session?.branch,
    cwd: shortCwd(session?.project_path, 28),
  };

  // Collapse order, narrowest first: hints, cwd, branch, model. Posture is never collapsed.
  const width = state.cols;
  const showHints = width >= 50;
  const showCwd = width >= 60;
  const showBranch = width >= 70;
  const showModel = width >= 80;

  const branchArrow = (): string => {
    if (!session?.branch) return '';
    const ahead = session.ahead;
    const behind = session.behind;
    if (ahead === null || ahead === undefined) return session.branch;
    // Ahead/behind only appear when git reported them; neither number is guessed.
    const up = ahead > 0 ? `↑${ahead}` : '';
    const down = behind && behind > 0 ? `↓${behind}` : '';
    return [session.branch, up, down].filter(Boolean).join(' ');
  };

  const context =
    state.context && state.context.limit > 0
      ? `ctx ${Math.round((state.context.used / state.context.limit) * 100)}% (${formatCount(state.context.used)}/${formatCount(state.context.limit)})`
      : '';

  const hints = showHints ? formatHintList(hintsFor(state.phase, overlayOpen)) : '';

  // Order on the right is the spec's: contextual hints, then the context meter. Dropping never
  // reorders what stays.
  const postureWidth = permission.length + 3;
  const rightParts = [hints, context].filter((p): p is string => p.length > 0);
  const rightWidth = (parts: readonly string[]): number => parts.join('   ').length;

  // Hints go first when keeping them would leave the posture nowhere to go; only if there is
  // still no room does the meter yield, because the posture wins over both.
  if (postureWidth + rightWidth(rightParts) + 3 > width) {
    const withoutHints = rightParts.filter((p) => p !== hints);
    if (withoutHints.length !== rightParts.length) {
      rightParts.length = 0;
      rightParts.push(...withoutHints);
    }
  }
  if (postureWidth + rightWidth(rightParts) + 3 > width) {
    const withoutMeter = rightParts.filter((p) => p !== context);
    rightParts.length = 0;
    rightParts.push(...withoutMeter);
  }
  const right = rightParts.join('   ');

  // Lower priority goes first on the left: cwd, then branch, then model. The posture is priority
  // 0 and is never a candidate for removal.
  const room = Math.max(postureWidth, width - right.length - 3);
  const candidates: { text: string; priority: number }[] = [
    { text: pieces.model ?? '', priority: 3 },
    { text: permission, priority: 0 },
    { text: branchArrow(), priority: 2 },
    { text: showCwd ? pieces.cwd : '', priority: 1 },
  ].filter((c) => c.text.length > 0);

  let kept = [...candidates];
  const joinedLength = (): number => kept.map((c) => c.text).join(' · ').length;
  while (joinedLength() > room && kept.some((c) => c.priority > 0)) {
    const droppable = kept.filter((c) => c.priority > 0);
    const victim = droppable.reduce((a, b) => (a.priority < b.priority ? a : b));
    kept = kept.filter((c) => c !== victim);
  }
  const left = kept.map((c) => c.text).join(' · ');

  return (
    <Box width={width} justifyContent="space-between">
      <Text color={c.muted}>{left}</Text>
      <Text color={c.secondary}>
        {right}
        {state.queue.length > 0 ? `   ${g.queued} ${state.queue.length} queued` : ''}
      </Text>
    </Box>
  );
}

export function formatCount(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${Math.round(n / 1000)}k`;
  return `${(n / 1_000_000).toFixed(1)}M`;
}

/** Width-aware truncation. A footer that overflows pushes the composer, which must never move. */
export function truncate(text: string, width: number): string {
  if (width <= 0) return '';
  if (text.length <= width) return text;
  return width <= 1 ? text.slice(0, width) : `${text.slice(0, width - 1)}…`;
}