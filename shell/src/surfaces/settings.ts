/**
 * The settings registry: one row per configuration value, each with where its value comes from.
 *
 * Two things make this a registry rather than a form: the sheet renders whatever is here, and the
 * palette offers whatever is here. Adding a setting is adding a row, not editing a view.
 *
 * The `config` path and `env` name in each row are facts about the engine's configuration surface
 * (`niki.example.toml`), not guesses — the shell points the user at the place that actually has to
 * change. A row whose value the engine never sent resolves to `not reported`, because a settings
 * sheet that fills a blank is a settings sheet that invents.
 */

import type { AppState } from '../state.js';
import { THEME_NAMES } from '../theme/index.js';
import { permissionMode } from '../protocol/schemas.js';

export type SettingKey =
  | 'permissions.mode'
  | 'permissions.bypass'
  | 'providers.default_model'
  | 'agents.effort'
  | 'general.max_revision_rounds'
  | 'general.spend_cap_usd'
  | 'ui.theme'
  | 'ui.timestamps'
  | 'ui.scrollbar'
  | 'ui.diff_line_numbers';

export type SettingGroup = 'Permissions' | 'Model' | 'Run' | 'Interface';

export type SettingSpec = {
  readonly key: SettingKey;
  readonly label: string;
  readonly group: SettingGroup;
  /** Exactly where the value lives in `niki.toml`. */
  readonly config: string;
  /** The env var that overrides it, when the engine reads one. */
  readonly env?: string;
  /** The permitted values, when the value is an enumeration rather than free text. */
  readonly options?: readonly string[];
  /**
   * A value whose mistake costs the user something they cannot undo — a permission posture, and
   * bypass in particular. These are never applied by a single keystroke.
   */
  readonly safety: 'none' | 'confirm';
};

/** NIKI's own effort presets, as documented in `niki.example.toml` (`low | medium | high`). */
export const EFFORT_PRESETS: readonly string[] = ['low', 'medium', 'high'];

/** The permission modes the protocol declares, read off the wire schema rather than retyped. */
export const PERMISSION_MODES: readonly string[] = permissionMode.options;

export const SETTINGS: readonly SettingSpec[] = [
  {
    key: 'permissions.mode',
    label: 'Permission posture',
    group: 'Permissions',
    config: 'niki.toml [permissions] mode',
    options: PERMISSION_MODES,
    safety: 'confirm',
  },
  {
    key: 'permissions.bypass',
    label: 'Bypass prompts',
    group: 'Permissions',
    config: 'niki.toml [permissions] mode = "bypass"',
    safety: 'confirm',
  },
  {
    key: 'providers.default_model',
    label: 'Model',
    group: 'Model',
    config: 'niki.toml [providers.*] default_model',
    env: '<PROVIDER>_MODEL',
    safety: 'none',
  },
  {
    key: 'agents.effort',
    label: 'Effort preset',
    group: 'Model',
    config: 'niki.toml [agents.*] effort',
    options: EFFORT_PRESETS,
    safety: 'none',
  },
  {
    key: 'general.max_revision_rounds',
    label: 'Revision rounds',
    group: 'Run',
    config: 'niki.toml [general] max_revision_rounds',
    safety: 'none',
  },
  {
    key: 'general.spend_cap_usd',
    label: 'Spend cap (USD)',
    group: 'Run',
    config: 'niki.toml [general] spend_cap_usd',
    safety: 'none',
  },
  {
    key: 'ui.theme',
    label: 'Theme',
    group: 'Interface',
    config: 'niki-shell --theme',
    options: THEME_NAMES,
    safety: 'none',
  },
  { key: 'ui.timestamps', label: 'Timestamps', group: 'Interface', config: 'niki-shell (this session)', safety: 'none' },
  { key: 'ui.scrollbar', label: 'Scrollbar', group: 'Interface', config: 'niki-shell (this session)', safety: 'none' },
  { key: 'ui.diff_line_numbers', label: 'Diff line numbers', group: 'Interface', config: 'niki-shell (this session)', safety: 'none' },
];

export type SettingSource =
  /** The engine sent it, in this session. */
  | { readonly kind: 'engine'; readonly detail: string }
  /** It is in the config file, and/or overridable by an env var. */
  | { readonly kind: 'config'; readonly path: string; readonly env?: string }
  /** Nobody reported it, so it is not shown as a number. */
  | { readonly kind: 'unknown' };

export type ResolvedSetting = {
  readonly spec: SettingSpec;
  /** `undefined` when nobody has reported a value: the sheet says so rather than guessing. */
  readonly value?: string;
  readonly source: SettingSource;
  /** True when a staged edit differs from the reported value. */
  readonly staged?: string;
};

/** The value the engine has actually reported for this row, or `undefined`. */
function reportedValue(key: SettingKey, state: AppState): string | undefined {
  switch (key) {
    case 'permissions.mode':
      return state.session?.permission_mode ?? state.permissionOverride ?? undefined;
    case 'permissions.bypass':
      return state.permissionOverride === 'bypass' || state.session?.permission_mode === 'bypass'
        ? 'on'
        : state.session?.permission_mode === undefined
          ? undefined
          : 'off';
    case 'providers.default_model':
      return state.session?.model ?? undefined;
    case 'agents.effort':
      // No declared message carries an effort level, so there is nothing to report. The row shows
      // "not reported" rather than a level the engine never named.
      return undefined;
    case 'general.max_revision_rounds':
      return undefined;
    case 'general.spend_cap_usd':
      return undefined;
    case 'ui.theme':
      return state.activeTheme ?? undefined;
    case 'ui.timestamps':
      return state.showTimestamps ? 'on' : 'off';
    case 'ui.scrollbar':
      return state.showScrollbar ? 'on' : 'off';
    case 'ui.diff_line_numbers':
      return state.showLineNumbers ? 'on' : 'off';
  }
}

/** Where the reported value came from, which is never inferred from the value itself. */
function sourceFor(key: SettingKey, state: AppState): SettingSource {
  if (reportedValue(key, state) === undefined) return { kind: 'unknown' };
  switch (key) {
    case 'permissions.mode':
    case 'providers.default_model':
      return { kind: 'engine', detail: 'engine session' };
    case 'permissions.bypass':
      return { kind: 'engine', detail: 'engine session' };
    case 'ui.theme':
      return { kind: 'engine', detail: 'shell' };
    case 'ui.timestamps':
    case 'ui.scrollbar':
    case 'ui.diff_line_numbers':
      return { kind: 'engine', detail: 'this session' };
    case 'agents.effort':
      return { kind: 'unknown' };
    case 'general.max_revision_rounds':
    case 'general.spend_cap_usd':
      return { kind: 'unknown' };
  }
}

/**
 * Every row resolved against a state. `ui.*` rows report the shell's own flags, so they always
 * have a value; every other row reports nothing at all until the engine says something.
 */
export function resolveSettings(state: AppState): readonly ResolvedSetting[] {
  return SETTINGS.map((spec) => {
    const value = reportedValue(spec.key, state);
    const staged = state.settings[spec.key];
    return {
      spec,
      value,
      source: sourceFor(spec.key, state),
      staged: staged !== undefined && staged !== value ? staged : undefined,
    };
  });
}

/** `true` when a staged value needs an explicit confirmation before it can be applied. */
export function needsConfirmation(spec: SettingSpec, stagedValue: string): boolean {
  if (spec.safety === 'confirm') return true;
  // A value of `bypass` is safety-critical wherever it appears, so the check is on the value too.
  return stagedValue === 'bypass';
}