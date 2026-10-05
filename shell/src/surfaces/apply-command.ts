/**
 * What a command actually does.
 *
 * `state.ts` owns the state machine; this module owns the behaviour a command triggers. It is a
 * pure function of a state, so a test can run `/yolo` against a running turn and assert on the
 * result without a terminal, a socket or an engine.
 *
 * The rule from the owner, and the reason this file exists: **a command that is merely listed is
 * not enough**. Every branch here either changes state in a way the user can see, or records a
 * request for `cli.tsx` to send. Nothing here returns the state untouched.
 *
 * A second rule matters just as much: **never invent a value the engine did not send**. `/context`
 * with no `context.usage` says the engine has not reported one; it does not show a zero.
 */

import type { AppState, ConfirmRequest, LocalAction, SettingsChange } from '../state.js';
import { commandFor, selectedCommand } from './commands.js';
import { filterItems, overlayItems } from './items.js';
import { needsConfirmation, resolveSettings, EFFORT_PRESETS, SETTINGS, type SettingSpec } from './settings.js';
import { isThemeName, THEME_NAMES } from '../theme/index.js';
import { sanitizeSingleLine } from '../sanitize.js';

/** The reducer, passed in rather than imported, so this module and `state.ts` never import a cycle. */
export type Apply = (state: AppState, action: LocalAction) => AppState;

const RECENT_ACTIONS_LIMIT = 12;

function note(state: AppState, text: string, level: 'info' | 'warning' | 'error' = 'info'): AppState {
  return { ...state, notices: [...state.notices, { text: sanitizeSingleLine(text), level }] };
}

/** Remembers the command for the palette's "recent actions", bounded. */
function remember(state: AppState, name: string): AppState {
  const recent = [name, ...state.recentActions.filter((n) => n !== name)].slice(0, RECENT_ACTIONS_LIMIT);
  return { ...state, recentActions: recent };
}

/**
 * Runs one registry command. An unknown token produces a row in the transcript saying so, because a
 * command that silently does nothing is indistinguishable from a broken shell.
 */
export function applyCommand(state: AppState, name: string, args: string): AppState {
  const command = commandFor(name);
  const base: AppState = { ...state, composer: '', slashMenu: null };
  if (!command) return note(base, `no command named ${name}`, 'warning');
  return run(command.name, remember(base, command.name), args.trim());
}

function run(name: string, state: AppState, args: string): AppState {
  switch (name) {
    case '/help':
      return { ...state, overlay: 'help', overlayQuery: '', overlayIndex: 0, overlayScroll: 0 };

    case '/model':
    case '/effort':
    case '/theme':
    case '/threads': {
      // A command with an argument can be driven from the command line, so the popup's argument
      // hint is true rather than decorative. An argument the shell cannot honour is refused
      // explicitly, because quietly opening a picker would look like the argument worked.
      const honoured = withArgument(name, state, args);
      if (honoured) return honoured;
      return { ...state, overlay: pickerOverlayFor(name), overlayQuery: '', overlayIndex: 0, overlayScroll: 0 };
    }

    case '/prompts':
      return { ...state, overlay: 'history', overlayQuery: '', overlayIndex: 0, overlayScroll: 0 };

    case '/clear':
      // The transcript only. The session, the branch and the spend the engine reported are facts
      // about the run, not rows the user typed, so clearing the screen must not erase them.
      return {
        ...state,
        messages: [],
        tools: [],
        stages: [],
        plan: [],
        verdict: null,
        turnEnd: null,
        diffRefs: [],
        scrollOffset: 0,
      };

    case '/copy': {
      const last = [...state.messages].reverse().find((m) => m.kind === 'assistant');
      if (!last) return note(state, 'no response to copy yet');
      return note(
        { ...state, composer: last.text },
        `copied ${last.text.length} characters into the composer`,
      );
    }

    case '/context':
      return state.context
        ? note(state, `context ${state.context.used} of ${state.context.limit} tokens`)
        : note(state, 'the engine has not reported context usage yet');

    case '/cost':
      return state.costUsd === null
        ? note(state, 'the engine has not reported any cost yet')
        : note(state, `spend this run: $${state.costUsd.toFixed(4)}`);

    case '/tokens': {
      const stages = state.stages.filter((s) => s.tokensIn !== undefined || s.tokensOut !== undefined);
      if (stages.length === 0) return note(state, 'the engine has reported no token counts yet');
      const tokensIn = stages.reduce((n, s) => n + (s.tokensIn ?? 0), 0);
      const tokensOut = stages.reduce((n, s) => n + (s.tokensOut ?? 0), 0);
      return note(state, `${tokensIn} in · ${tokensOut} out across ${stages.length} stages`);
    }

    case '/editor':
      return { ...state, editorRequested: true };

    case '/version':
      return state.engineVersion
        ? note(state, `engine ${state.engineVersion}`)
        : note(state, 'the engine has not reported a version yet');

    case '/reload':
      return { ...state, reloadRequested: true };

    case '/quit':
      return { ...state, exitArmed: true };

    case '/auto':
      // The posture rides along with the next turn, so this is an override rather than a rewrite of
      // what the engine reported. The footer shows both, so neither is hidden.
      return note({ ...state, permissionOverride: 'auto' }, 'next turn runs in auto');

    case '/manual':
      return note({ ...state, permissionOverride: 'manual' }, 'next turn runs in manual');

    case '/yolo':
      // Bypass is never a single keystroke: it opens a confirmation the user has to accept.
      return { ...state, confirm: bypassConfirmation(state), overlay: null };

    case '/scrollbar':
      return toggleFlag(state, 'showScrollbar', 'ui.scrollbar', 'scrollbar');

    case '/timestamps':
      return toggleFlag(state, 'showTimestamps', 'ui.timestamps', 'timestamps');

    case '/line-numbers': {
      const next = toggleFlag(state, 'showLineNumbers', 'ui.diff_line_numbers', 'diff line numbers');
      const on = next.showLineNumbers;
      // Honesty about what the flag affects: the shell renders diffs only when the engine sends
      // one, so with no diff open the row says so rather than implying a diff gained a gutter.
      return on && state.diffRefs.length === 0
        ? note(next, 'diff line numbers on · no diff is open')
        : next;
    }

    default:
      // Unreachable while every registry row has a branch above. Failing loudly beats a command
      // that is listed and does nothing.
      return note(state, `${name} has no behaviour yet`, 'warning');
  }
}

function pickerOverlayFor(name: string): 'model' | 'effort' | 'theme' | 'sessions' | 'history' {
  switch (name) {
    case '/model':
      return 'model';
    case '/effort':
      return 'effort';
    case '/theme':
      return 'theme';
    case '/threads':
      return 'sessions';
    default:
      return 'history';
  }
}

/**
 * `/command <argument>`, for the four commands that declare one. Returns `undefined` when there is
 * no argument to honour, so the caller falls through to opening the picker.
 */
function withArgument(name: string, state: AppState, args: string): AppState | undefined {
  if (args === '') return undefined;
  switch (name) {
    case '/model': {
      // Only a model the engine reported may be staged. Naming a model nobody reported would put a
      // value in the settings report that came from the user's typing, not from the engine.
      if (state.session?.model !== args) {
        return note(
          state,
          state.session
            ? `the engine reported ${state.session.model}, not ${args}`
            : 'the engine has reported no model to choose',
          'warning',
        );
      }
      return { ...state, settings: { ...state.settings, 'providers.default_model': args } };
    }
    case '/effort':
      if (!EFFORT_PRESETS.includes(args)) {
        return note(state, `${args} is not one of ${EFFORT_PRESETS.join(', ')}`, 'warning');
      }
      return { ...state, settings: { ...state.settings, 'agents.effort': args } };
    case '/theme':
      if (!isThemeName(args)) {
        return note(state, `${args} is not one of ${THEME_NAMES.join(', ')}`, 'warning');
      }
      return { ...state, activeTheme: args };
    case '/threads':
      return {
        ...state,
        outbox: [
          ...state.outbox,
          {
            method: 'session.load',
            params: { session_id: args, project_path: state.session?.project_path ?? '' },
            traceId: 'session-load',
          },
        ],
      };
    default:
      return undefined;
  }
}

function toggleFlag(
  state: AppState,
  flag: 'showScrollbar' | 'showTimestamps' | 'showLineNumbers',
  settingKey: string,
  label: string,
): AppState {
  const on = !state[flag];
  const next: AppState = { ...state, [flag]: on };
  return note(
    { ...next, settings: { ...state.settings, [settingKey]: on ? 'on' : 'off' } },
    `${label} ${on ? 'on' : 'off'}`,
  );
}

function bypassConfirmation(state: AppState): ConfirmRequest {
  const spec = SETTINGS.find((s) => s.key === 'permissions.bypass');
  return {
    key: spec?.key ?? 'permissions.bypass',
    label: 'Permission mode: bypass',
    value: 'bypass',
    from: state.session?.permission_mode ?? state.permissionOverride ?? 'unknown',
    destination: spec?.config ?? 'niki.toml [permissions] mode',
  };
}

/** Accepts the highlighted row of an overlay, then closes the overlay if it is still open. */
export function acceptOverlayItem(state: AppState, apply: Apply): AppState {
  const rows = filterItems(state.overlayQuery, overlayItems(state));
  const item = rows[Math.min(state.overlayIndex, Math.max(0, rows.length - 1))];
  if (!item) return apply(state, { kind: 'overlay.close' });
  const opened = state.overlay;
  let next = state;
  if (item.onMove) next = apply(next, item.onMove);
  next = apply(next, item.onAccept);
  // A row that opened another overlay keeps it, and so does a row that has something left to
  // show — the settings sheet, whose whole point is the report a save produces.
  if (item.keepOpen) return next;
  return next.overlay === opened ? apply(next, { kind: 'overlay.close' }) : next;
}

/** Enter in the slash popup: run the highlighted row, exactly as if it had been typed in full. */
export function applySelectedFromPopup(state: AppState): AppState {
  const menu = state.slashMenu;
  if (!menu) return state;
  const command = selectedCommand(menu.query, menu.selected);
  if (!command) return { ...state, slashMenu: null };
  return applyCommand(state, command.name, '');
}

/** Tab in the slash popup: put the highlighted command in the composer, ready for its arguments. */
export function completeFromPopup(state: AppState): AppState {
  const menu = state.slashMenu;
  if (!menu) return state;
  const command = selectedCommand(menu.query, menu.selected);
  if (!command) return state;
  // The popup closes on completion: what the user types next is an argument, not a command name.
  return { ...state, composer: `${command.name} `, slashMenu: null, pendingChord: '' };
}

/** Cycles the focused row's value in the settings sheet. A row with no fixed set is left alone. */
export function cycleSetting(state: AppState, by: number): AppState {
  const rows = resolveSettings(state);
  const row = rows[Math.min(state.overlayIndex, Math.max(0, rows.length - 1))];
  const options = row?.spec.options;
  if (!row || !options || options.length === 0) return state;
  const current = state.settings[row.spec.key] ?? row.value ?? options[0] ?? '';
  const at = options.indexOf(current);
  const from = at === -1 ? 0 : at;
  const nextIndex = (((from + by) % options.length) + options.length) % options.length;
  const value = options[nextIndex] ?? current;
  return { ...state, settings: { ...state.settings, [row.spec.key]: value } };
}

/**
 * Saves the staged settings.
 *
 * Every staged change becomes one row of a report naming the file or env var that would have to
 * change. Protocol v1 has no settings request, so nothing is written anywhere, and the report says
 * exactly that rather than implying a save happened. A safety-critical change is held back for an
 * explicit confirmation before it even reaches the report.
 */
export function saveSettings(state: AppState): AppState {
  const pending = stagedChanges(state);
  if (pending.length === 0) return note({ ...state }, 'no staged settings to save');

  const safe: SettingsChange[] = [];
  const held: SettingsChange[] = [];
  for (const change of pending) {
    const spec = SETTINGS.find((s) => s.key === change.key);
    if (spec && needsConfirmation(spec, change.to)) held.push(change);
    else safe.push(change);
  }

  const applied = withoutSettings(state, safe.map((c) => c.key));
  const report = [...safe, ...state.settingsReport];
  if (held.length === 0) {
    return note({ ...applied, settingsReport: report }, unsavedNotice(report.length));
  }
  const first = held[0]!;
  return {
    ...applied,
    settingsReport: report,
    confirm: {
      key: first.key,
      label: first.label,
      value: first.to,
      from: first.from,
      destination: first.destination,
    },
  };
}

/** The honest sentence at the bottom of the report: what changed, and that it went nowhere yet. */
export function unsavedNotice(count: number): string {
  return (
    `${count} setting${count === 1 ? '' : 's'} staged · nothing written: ` +
    'protocol v1 has no settings request'
  );
}

/** Accepts the change on screen, then asks about the next one if there is one. */
export function confirmAccepted(state: AppState): AppState {
  const pending = state.confirm;
  if (!pending) return state;
  const accepted: SettingsChange = {
    key: pending.key,
    label: pending.label,
    from: pending.from,
    to: pending.value,
    destination: pending.destination,
  };
  const next = stagedChanges(state)
    .filter((change) => change.key !== pending.key)
    .find((change) => {
      const spec = SETTINGS.find((s) => s.key === change.key);
      return spec !== undefined && needsConfirmation(spec, change.to);
    });
  return {
    ...state,
    confirm: next
      ? { key: next.key, label: next.label, value: next.to, from: next.from, destination: next.destination }
      : null,
    settings: withoutSettings(state, [accepted.key]).settings,
    settingsReport: [...state.settingsReport, accepted],
  };
}

function stagedChanges(state: AppState): SettingsChange[] {
  const rows = resolveSettings(state);
  const changes: SettingsChange[] = [];
  for (const [key, value] of Object.entries(state.settings)) {
    const spec = SETTINGS.find((s) => s.key === key);
    if (!spec) continue;
    changes.push({
      key,
      label: spec.label,
      from: rows.find((r) => r.spec.key === key)?.value,
      to: value,
      destination: destinationFor(spec),
    });
  }
  return changes.sort((a, b) => a.key.localeCompare(b.key));
}

/** The exact place a value has to be written for the engine to read it. */
export function destinationFor(spec: SettingSpec): string {
  return spec.env === undefined ? spec.config : `${spec.config} (or ${spec.env})`;
}

function withoutSettings(state: AppState, keys: readonly string[]): AppState {
  const next: Record<string, string> = { ...state.settings };
  for (const key of keys) delete next[key];
  return { ...state, settings: next };
}