/**
 * Every row the palette and the pickers can show, derived from the command registry and the state.
 *
 * Two rules shape this module:
 *
 *  - **Nothing is declared here.** Commands come from `COMMANDS`; setting keys come from
 *    `SETTINGS`; themes come from `THEME_NAMES`; permission modes come off the wire schema. This
 *    module only turns those into rows and attaches the action each row performs.
 *  - **A row exists only because something reported it.** The sessions picker shows the session the
 *    engine named; the prompt-history picker shows the prompts this shell sent. When there is
 *    nothing, {@link overlayEmptyNote} says so instead of the list showing an empty box.
 */

import type { AppState, LocalAction, OverlayName } from '../state.js';
import { THEME_NAMES, type ThemeName } from '../theme/index.js';
import { sanitizeSingleLine } from '../sanitize.js';
import { COMMANDS } from '../components/footer.js';
import { commandFields } from './commands.js';
import { EFFORT_PRESETS, resolveSettings } from './settings.js';
import { fuzzyFilter, type Candidate } from './fuzzy.js';

export type SurfaceItem = {
  readonly id: string;
  readonly title: string;
  /** The secondary line: an argument hint, a source, a state. Never invented. */
  readonly detail?: string;
  readonly group: string;
  /** Extra text the fuzzy matcher may match on. */
  readonly fields: readonly string[];
  /** What happens when the row is accepted. */
  readonly onAccept: LocalAction;
  /** What happens when the row is moved onto, for pickers that preview. */
  readonly onMove?: LocalAction;
  /** The overlay stays open after the row is accepted — the sheet, which has something to show. */
  readonly keepOpen?: boolean;
};

function item(
  id: string,
  group: string,
  title: string,
  onAccept: LocalAction,
  extra: {
    detail?: string;
    fields?: readonly string[];
    onMove?: LocalAction;
    keepOpen?: boolean;
  } = {},
): SurfaceItem {
  return {
    id,
    group,
    title: sanitizeSingleLine(title),
    detail: extra.detail === undefined ? undefined : sanitizeSingleLine(extra.detail),
    fields: [title, extra.detail ?? '', ...(extra.fields ?? [])],
    onAccept,
    onMove: extra.onMove,
    keepOpen: extra.keepOpen,
  };
}

/** The rows a searchable overlay offers, before any query is applied. */
export function overlayItems(state: AppState): readonly SurfaceItem[] {
  switch (state.overlay) {
    case 'model':
      return modelItems(state);
    case 'effort':
      return EFFORT_PRESETS.map((preset) =>
        item(`effort:${preset}`, 'Effort', preset, {
          kind: 'settings.stage',
          key: 'agents.effort',
          value: preset,
        }),
      );
    case 'theme':
      return THEME_NAMES.map((name) =>
        item(`theme:${name}`, 'Theme', name, { kind: 'theme.commit', theme: name as ThemeName }, {
          onMove: { kind: 'theme.preview', theme: name as ThemeName },
        }),
      );
    case 'sessions':
      return sessionItems(state);
    case 'history':
      return historyItems(state);
    case 'settings':
      return settingsItems(state);
    case 'palette':
      return paletteItems(state);
    case 'help':
    case 'confirm':
      return [];
    case null:
      return [];
  }
}

/** The models the engine actually reported. Protocol v1 carries no catalogue, so this is at most
 * the one model named by `session.ready` — and nothing at all before that. */
function modelItems(state: AppState): readonly SurfaceItem[] {
  const model = state.session?.model;
  if (!model) return [];
  return [
    item(`model:${model}`, 'Models reported by the engine', model, {
      kind: 'settings.stage',
      key: 'providers.default_model',
      value: model,
    }, {
      detail: 'staged for niki.toml; protocol v1 has no model request',
    }),
  ];
}

function sessionItems(state: AppState): readonly SurfaceItem[] {
  const session = state.session;
  if (!session) return [];
  const detail =
    session.resumed_messages > 0
      ? `session ${session.session_id} · ${session.resumed_messages} messages resumed`
      : `session ${session.session_id} · new`;
  return [item(`session:${session.session_id}`, 'Sessions', session.session_id, {
    kind: 'session.resume',
    sessionId: session.session_id,
  }, { detail })];
}

/** Newest first, because the prompt a user wants again is the one they just sent. */
function historyItems(state: AppState): readonly SurfaceItem[] {
  return [...state.promptHistory].reverse().map((prompt, i) =>
    item(`history:${i}`, 'Prompt history', prompt, { kind: 'composer.insert', text: prompt }),
  );
}

/** One row per setting, with its value and where the value comes from. */
function settingsItems(state: AppState): readonly SurfaceItem[] {
  return resolveSettings(state).map((row) => {
    const value = row.staged ?? row.value ?? 'not reported';
    const source =
      row.source.kind === 'engine'
        ? `from ${row.source.detail}`
        : row.source.kind === 'config'
          ? `from ${row.source.path}${row.source.env ? ` or ${row.source.env}` : ''}`
          : 'no value reported yet';
    const detail = row.spec.safety === 'confirm' ? `${value} · ${source} · needs confirmation` : `${value} · ${source}`;
    return item(`setting:${row.spec.key}`, row.spec.group, row.spec.label, { kind: 'settings.save' }, {
      detail,
      fields: [row.spec.key, value, source],
      // Saving leaves the sheet open: the point of a save is the report the user then reads.
      keepOpen: true,
    });
  });
}

/** The palette: commands, pages, settings, sessions, models and recent actions. */
export function paletteItems(state: AppState): readonly SurfaceItem[] {
  const commands = COMMANDS.map((c) =>
    item(`command:${c.name}`, 'Commands', c.name, { kind: 'command.run', name: c.name, args: '' }, {
      detail: c.argumentHint ? `${c.description} · ${c.argumentHint}` : c.description,
      fields: commandFields(c),
    }),
  );

  const pages: readonly SurfaceItem[] = [
    item('page:chat', 'Pages', state.workspaceMode ? 'Chat' : 'Workspace', {
      kind: 'mode.cycle',
    }, { detail: state.workspaceMode ? 'back to the workspace view' : 'the workspace view' }),
    item('page:stages', 'Pages', 'Pipeline stages', { kind: 'stages.toggle' }, {
      detail: state.showStages ? 'shown' : 'hidden',
    }),
    item('page:details', 'Pages', 'Turn details', { kind: 'details.toggle' }, {
      detail: state.showDetails ? 'expanded' : 'collapsed',
    }),
    item('page:settings', 'Pages', 'Settings', { kind: 'overlay.open', overlay: 'settings' }),
    item('page:mouse', 'Pages', 'Mouse capture', { kind: 'mouse.toggle' }, {
      detail: state.mouseCapture ? 'on — one-key release is ctrl+m' : 'off — the terminal keeps its own selection',
    }),
  ];

  const settings = resolveSettings(state).map((row, index) =>
    item(`setting:${row.spec.key}`, `Settings · ${row.spec.group}`, row.spec.label, {
      kind: 'overlay.open',
      overlay: 'settings',
      index,
    }, {
      detail: row.staged ?? row.value ?? 'not reported',
      fields: [row.spec.key, row.spec.label, row.spec.config],
    }),
  );

  return [...commands, ...pages, ...settings, ...modelItems(state), ...sessionItems(state), ...recentItems(state)];
}

function recentItems(state: AppState): readonly SurfaceItem[] {
  return state.recentActions.map((name) =>
    item(`recent:${name}`, 'Recent actions', name, { kind: 'command.run', name, args: '' }, {
      detail: 'run again',
    }),
  );
}

/** What an empty picker says. Every sentence is a fact about the engine, not about a missing file. */
export function overlayEmptyNote(state: AppState, overlay: OverlayName | null): string {
  switch (overlay) {
    case 'model':
      return state.session
        ? 'the engine reported one model; protocol v1 carries no model catalogue'
        : 'no session yet — the engine has reported no model';
    case 'sessions':
      return state.capabilities && !state.capabilities.sessions
        ? 'the engine reported that sessions are not supported'
        : 'no session reported yet';
    case 'history':
      return 'nothing sent yet';
    case 'effort':
      return 'no effort preset is reported by the engine';
    case 'palette':
      return 'nothing matches';
    case 'settings':
      return 'no settings';
    default:
      return '';
  }
}

/** Filters a set of rows for a query. An empty query keeps declaration order. */
export function filterItems(query: string, rows: readonly SurfaceItem[]): readonly SurfaceItem[] {
  const candidates: Candidate<SurfaceItem>[] = rows.map((r) => ({ fields: r.fields, value: r }));
  return fuzzyFilter(query, candidates).map((hit) => hit.value);
}

/** Overlays whose rows are narrowed by typing: every overlay with a list, except the confirm. */
export function acceptsQuery(overlay: OverlayName | null): boolean {
  return overlay !== null && overlay !== 'confirm';
}