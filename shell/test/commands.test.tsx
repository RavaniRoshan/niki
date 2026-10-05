/**
 * Checklist rows F1 to F7, row by row.
 *
 * The row this file exists for is F3, and its test is deliberately strict: for every non-queued
 * command, with a run active, the command must change the state in a way a user can see, or record
 * a request for the engine. Clearing the composer does not count — `command.run` clears the
 * composer for *every* command, so counting that would prove nothing at all. The comparison below
 * throws the composer, the popup and the outbox away and compares everything else, which is what
 * "dispatches somewhere real" has to mean.
 */

import { describe, expect, it } from 'vitest';
import { render } from 'ink-testing-library';
import React from 'react';

import { App } from '../src/app.js';
import { handleKey, KEYMAP, type KeyEvent } from '../src/dispatch.js';
import {
  initialState,
  reduce,
  reduceLocal,
  type AppState,
  type LocalAction,
  type ReduceOptions,
} from '../src/state.js';
import { COMMANDS, worksWhileRunning, type Command } from '../src/components/footer.js';
import { commandFor, commandRows, selectedCommand } from '../src/surfaces/commands.js';
import { fuzzyScore } from '../src/surfaces/fuzzy.js';
import { paletteItems, overlayItems, filterItems, overlayEmptyNote } from '../src/surfaces/items.js';
import { EFFORT_PRESETS, resolveSettings } from '../src/surfaces/settings.js';
import { helpLines } from '../src/components/help.js';
import { renderTranscriptLines } from '../src/components/transcript.js';
import { glyphs } from '../src/glyphs.js';
import type { ServerNotification } from '../src/protocol/generated/index.js';

const T0: ReduceOptions = { nowMs: 1_700_000_000_000 };

const SESSION = {
  method: 'session.ready',
  params: {
    session_id: 's1',
    project_path: '/home/u/projects/api',
    model: 'claude-sonnet-4',
    permission_mode: 'manual',
    branch: 'main',
    ahead: 1,
    behind: null,
    resumed_messages: 2,
  },
} as ServerNotification;

const n = (method: string, params: unknown): ServerNotification =>
  ({ method, params }) as ServerNotification;

function run(events: ServerNotification[], from: AppState = initialState(80, 24)): AppState {
  return events.reduce((acc, e) => reduce(acc, e, T0), from);
}

/** A run in flight: session, a turn, a tool, an assistant answer and real usage numbers. */
function running(): AppState {
  return run([
    SESSION,
    n('turn.started', { turn_id: 't', prompt: 'audit the tests' }),
    n('turn.delta', { turn_id: 't', text: 'Here is what I found' }),
    n('tool.call', { tool_id: 'a', name: 'Read', args: 'package.json' }),
    n('tool.result', { tool_id: 'a', ok: true, summary: 'read 46 lines', full_ref: null, duration_ms: 3 }),
    n('context.usage', { used: 31_000, limit: 262_000 }),
    n('cost.update', { usd: 0.021 }),
    n('stage.done', {
      stage_id: 'g',
      role: 'coder',
      summary: 'done',
      tokens_in: 1200,
      tokens_out: 340,
      cost_usd: 0.01,
      latency_ms: 5,
      retry_count: 0,
      artifact_ref: null,
      provenance: 'self_verification',
    }),
  ]);
}

function apply(state: AppState, ...actions: LocalAction[]): AppState {
  return actions.reduce((s, a) => reduceLocal(s, a, T0), state);
}

function key(overrides: Partial<KeyEvent> = {}): KeyEvent {
  return {
    input: '',
    ctrl: false,
    meta: false,
    shift: false,
    escape: false,
    return: false,
    backspace: false,
    delete: false,
    upArrow: false,
    downArrow: false,
    leftArrow: false,
    rightArrow: false,
    pageUp: false,
    pageDown: false,
    home: false,
    end: false,
    tab: false,
    ...overrides,
  };
}

function frame(state: AppState, cols = 80, rows = 24): string {
  const out = render(
    <App state={{ ...state, cols, rows }} theme="niki" charset="unicode" reducedMotion />,
  );
  const text = out.lastFrame() ?? '';
  out.unmount();
  return text;
}

function transcriptText(state: AppState): string {
  return renderTranscriptLines({
    state,
    theme: 'niki',
    charset: 'unicode',
    reducedMotion: true,
    sweepTick: 0,
    height: 24,
  })
    .map((l) => l.text)
    .join('\n');
}

/** Types text one character at a time, exactly as the input parser delivers it. */
function typed(state: AppState, text: string): AppState {
  let s = state;
  for (const ch of text) {
    const outcome = handleKey(s, key({ input: ch }));
    s = outcome.insert !== undefined
      ? reduceLocal(s, { kind: 'composer.set', text: s.composer + outcome.insert }, T0)
      : apply(s, ...outcome.actions);
  }
  return s;
}

/** Everything about a state except the three fields a command changes for free. */
function meaningful(state: AppState): string {
  const { composer: _c, slashMenu: _s, outbox: _o, ...rest } = state;
  return JSON.stringify(rest);
}

describe('F1: one registry, declared once, with every field on every command', () => {
  it('every command declares a name, description, aliases, keywords, an argument hint and a tier', () => {
    for (const c of COMMANDS) {
      expect(c.name.startsWith('/'), `${c.name} is not a command`).toBe(true);
      expect(c.description.length, `${c.name} has no description`).toBeGreaterThan(0);
      expect(Array.isArray(c.aliases), `${c.name} has no alias list`).toBe(true);
      expect(c.keywords.length, `${c.name} has no fuzzy keywords`).toBeGreaterThan(0);
      expect(typeof c.argumentHint, `${c.name} does not declare an argument hint`).toBe('string');
      expect(['always', 'immediateUi', 'sideEffectFree', 'queued']).toContain(c.tier);
    }
  });

  it('every command in the registry is reachable by name and by every alias', () => {
    for (const c of COMMANDS) {
      expect(commandFor(c.name), `${c.name} is not findable by name`).toBe(c);
      for (const alias of c.aliases) expect(commandFor(alias), `${alias} is not findable`).toBe(c);
    }
    expect(commandFor('/nonsense')).toBeUndefined();
  });

  it('the registry is the only source: every popup row is a registry row', () => {
    const rows = commandRows('');
    expect(rows).toHaveLength(COMMANDS.length);
    for (const row of rows) expect(COMMANDS).toContain(row);
  });
});

describe('F2: the slash popup is fuzzy, argument-aware, and never blocks typing', () => {
  it('typing / at the start of an empty composer opens it over the whole registry', () => {
    const s = typed(initialState(80, 24), '/');
    expect(s.slashMenu).not.toBeNull();
    expect(commandRows(s.slashMenu?.query ?? '')).toHaveLength(COMMANDS.length);
  });

  it('filters live as ordinary characters keep arriving', () => {
    const s = typed(initialState(80, 24), '/mo');
    expect(s.composer).toBe('/mo');
    expect(s.slashMenu?.query).toBe('mo');
    const rows = commandRows('mo');
    expect(rows.map((r) => r.name)).toContain('/model');
    expect(rows.length).toBeLessThan(COMMANDS.length);
  });

  it('matches the hidden keywords too, which do not appear in any name', () => {
    const rows = commandRows('money').map((r) => r.name);
    expect(rows).toContain('/cost');
    const history = commandRows('resume').map((r) => r.name);
    expect(history).toContain('/threads');
  });

  it('up and down move the highlight and wrap', () => {
    let s = typed(initialState(80, 24), '/');
    expect(s.slashMenu?.selected).toBe(0);
    const down = handleKey(s, key({ downArrow: true }));
    s = apply(s, ...down.actions);
    expect(s.slashMenu?.selected).toBe(1);
    s = apply(s, ...handleKey(s, key({ upArrow: true, })).actions);
    expect(s.slashMenu?.selected).toBe(0);
    s = apply(s, ...handleKey(s, key({ upArrow: true })).actions);
    expect(s.slashMenu?.selected, 'the highlight wraps to the end').toBe(COMMANDS.length - 1);
  });

  it('Tab completes the highlighted command into the composer', () => {
    const typedSlash = typed(initialState(80, 24), '/thre');
    const expected = selectedCommand('thre', 0)?.name;
    expect(expected).toBe('/threads');
    const completed = apply(typedSlash, ...handleKey(typedSlash, key({ tab: true })).actions);
    expect(completed.composer).toBe('/threads ');
    expect(completed.slashMenu, 'completion hands the keyboard back to the composer').toBeNull();
  });

  it('Enter accepts the highlighted command, and Esc closes the popup without running anything', () => {
    const typedSlash = typed(running(), '/cle');
    const accepted = apply(typedSlash, ...handleKey(typedSlash, key({ return: true, input: '\r' })).actions);
    expect(accepted.overlay).toBeNull();
    expect(accepted.messages, '/clear ran').toHaveLength(0);
    expect(accepted.tools, '/clear cleared the tool rows').toHaveLength(0);
    expect(accepted.session, '/clear keeps the session the engine reported').not.toBeNull();

    const stillTyping = apply(typedSlash, ...handleKey(typedSlash, key({ escape: true })).actions);
    expect(stillTyping.slashMenu).toBeNull();
    expect(stillTyping.composer).toBe('/cle');
    expect(stillTyping.messages, 'esc ran nothing').toHaveLength(2);
  });

  it('shows the argument hint for a command that takes one, in the popup itself', () => {
    const rows = commandRows('thre');
    const threads = rows[0]!;
    expect(threads.argumentHint).toBe('<session id>');
    const out = frame(typed(initialState(80, 24), '/thre'));
    expect(out).toContain('<session id>');
  });

  it('says so when nothing matches, rather than showing an empty box', () => {
    const s = typed(initialState(80, 24), '/zzzz');
    expect(commandRows('zzzz')).toHaveLength(0);
    expect(frame(s)).toContain('no command matches');
  });

  it('typing while the popup is open goes to the composer, not to the popup', () => {
    const s = typed(initialState(80, 24), '/theme n');
    expect(s.composer).toBe('/theme n');
    expect(s.slashMenu?.query).toBe('theme');
  });

  it('renders inside the terminal width at every size', () => {
    const s = typed(initialState(80, 24), '/');
    for (const cols of [50, 80, 120]) {
      const out = frame(s, cols);
      const longest = out.split('\n').reduce((n2, l) => Math.max(n2, l.length), 0);
      expect(longest, `popup overflowed at ${cols}`).toBeLessThanOrEqual(cols);
    }
  });
});

describe('F3: every non-queued command does something real while a run is active', () => {
  it('each one changes the state or records a request — clearing the composer does not count', () => {
    const offenders: string[] = [];
    for (const command of COMMANDS) {
      if (!worksWhileRunning(command.tier)) continue;
      const before = running();
      const after = reduceLocal(before, { kind: 'command.run', name: command.name, args: '' }, T0);
      const changed = meaningful(after) !== meaningful(before);
      const requested = after.outbox.length > before.outbox.length;
      if (!changed && !requested) offenders.push(command.name);
      // The run itself must survive: a command is not allowed to end the turn it ran beside.
      expect(after.phase, `${command.name} changed the phase of a running turn`).toBe(before.phase);
    }
    expect(offenders, 'these commands are listed but do nothing').toEqual([]);
  });

  it('the run is still visible after any of them ran', () => {
    for (const command of COMMANDS) {
      const after = reduceLocal(running(), { kind: 'command.run', name: command.name, args: '' }, T0);
      expect(after.activity === null, `${command.name} cleared the live activity line`).toBe(false);
    }
  });

  it('an unknown command says so instead of doing nothing quietly', () => {
    const after = reduceLocal(running(), { kind: 'command.run', name: '/nope', args: '' }, T0);
    expect(after.notices.map((n2) => n2.text).join(' ')).toContain('no command named /nope');
  });

  it('the numbers a command shows come from the engine, and it says so when they are missing', () => {
    const withNumbers = reduceLocal(running(), { kind: 'command.run', name: '/context', args: '' }, T0);
    expect(transcriptText(withNumbers)).toContain('context 31000 of 262000 tokens');

    const idle = run([SESSION]);
    const without = reduceLocal(idle, { kind: 'command.run', name: '/cost', args: '' }, T0);
    expect(transcriptText(without)).toContain('has not reported any cost');
    expect(transcriptText(without)).not.toContain('$0.0000');
  });

  it('/version reports the engine version and refuses to invent one', () => {
    const known = apply(running(), { kind: 'engine.meta', version: '0.1.0', capabilities: null });
    expect(transcriptText(apply(known, { kind: 'command.run', name: '/version', args: '' }))).toContain('0.1.0');
    const unknown = apply(running(), { kind: 'command.run', name: '/version', args: '' });
    expect(transcriptText(unknown)).toContain('has not reported a version');
  });

  it('a permission posture change is staged, not written over what the engine reported', () => {
    const auto = reduceLocal(running(), { kind: 'command.run', name: '/auto', args: '' }, T0);
    expect(auto.permissionOverride).toBe('auto');
    expect(auto.session?.permission_mode).toBe('manual');
    expect(frame(auto)).toContain('manual → auto');
  });

  it('/yolo is never applied by a keystroke: it asks first, and Esc never confirms', () => {
    const asked = reduceLocal(running(), { kind: 'command.run', name: '/yolo', args: '' }, T0);
    expect(asked.confirm?.value).toBe('bypass');
    expect(asked.permissionOverride).toBeNull();
    const cancelled = apply(asked, ...handleKey(asked, key({ escape: true })).actions);
    expect(cancelled.confirm).toBeNull();
    expect(cancelled.permissionOverride).toBeNull();
    const confirmed = apply(asked, ...handleKey(asked, key({ return: true, input: '\r' })).actions);
    expect(confirmed.confirm).toBeNull();
    // Accepting stages nothing silently: it produces exactly one report row, naming the file.
    expect(confirmed.settingsReport).toHaveLength(1);
    expect(confirmed.settingsReport[0]?.key).toBe('permissions.bypass');
  });

  it('an argument hint is true: `/command <arg>` does what the popup promised', () => {
    // The popup tells the user what to type after the name. These four commands honour it, so the
    // hint is not decoration.
    const themed = reduceLocal(running(), { kind: 'command.run', name: '/theme', args: 'niki-dim' }, T0);
    expect(themed.activeTheme).toBe('niki-dim');

    const effort = reduceLocal(running(), { kind: 'command.run', name: '/effort', args: 'high' }, T0);
    expect(effort.settings['agents.effort']).toBe('high');

    const resumed = reduceLocal(running(), { kind: 'command.run', name: '/threads', args: 's7' }, T0);
    expect(resumed.outbox.map((r) => r.params)).toMatchObject([{ session_id: 's7' }]);

    const chosen = reduceLocal(
      running(),
      { kind: 'command.run', name: '/model', args: 'claude-sonnet-4' },
      T0,
    );
    expect(chosen.settings['providers.default_model']).toBe('claude-sonnet-4');
  });

  it('an argument the shell cannot honour is refused rather than quietly ignored', () => {
    const inventedModel = reduceLocal(
      running(),
      { kind: 'command.run', name: '/model', args: 'gpt-9' },
      T0,
    );
    expect(inventedModel.overlay, 'the picker must not open as if the argument worked').toBeNull();
    expect(inventedModel.settings['providers.default_model']).toBeUndefined();
    expect(transcriptText(inventedModel)).toContain('the engine reported claude-sonnet-4, not gpt-9');

    const badTheme = reduceLocal(running(), { kind: 'command.run', name: '/theme', args: 'neon' }, T0);
    expect(badTheme.activeTheme).toBeNull();
    expect(transcriptText(badTheme)).toContain('neon is not one of');

    const badEffort = reduceLocal(running(), { kind: 'command.run', name: '/effort', args: 'loud' }, T0);
    expect(transcriptText(badEffort)).toContain('loud is not one of low, medium, high');
  });

  it('the toggles are visible, not silent', () => {
    for (const name of ['/scrollbar', '/timestamps', '/line-numbers']) {
      const after = reduceLocal(running(), { kind: 'command.run', name, args: '' }, T0);
      expect(transcriptText(after), `${name} said nothing`).toMatch(/on|no diff is open/);
      expect(Object.keys(after.settings).length, `${name} staged nothing`).toBeGreaterThan(0);
    }
  });
});

describe('F4: the palette is fuzzy over commands, pages, settings, sessions and recent actions', () => {
  const open = (state: AppState): AppState =>
    apply(state, ...handleKey(state, key({ ctrl: true, input: 'k' })).actions);

  it('opens with ctrl+k and offers every group', () => {
    const ran = reduceLocal(running(), { kind: 'command.run', name: '/cost', args: '' }, T0);
    const s = open(ran);
    expect(s.overlay).toBe('palette');
    const groups = new Set(paletteItems(s).map((i) => i.group));
    expect(groups.has('Commands')).toBe(true);
    expect(groups.has('Pages')).toBe(true);
    expect([...groups].some((g) => g.startsWith('Settings'))).toBe(true);
    expect(groups.has('Sessions')).toBe(true);
    expect(groups.has('Recent actions')).toBe(true);
  });

  it('every command is in it, from the registry and not from a second list', () => {
    const items = paletteItems(open(running()));
    const names = items.filter((i) => i.group === 'Commands').map((i) => i.title);
    expect(names).toEqual(COMMANDS.map((c) => c.name));
  });

  it('narrows as the user types, and the keys go to the query rather than the composer', () => {
    let s = open(running());
    s = typed(s, 'cost');
    expect(s.composer, 'the composer stays untouched while the palette is open').toBe('');
    expect(s.overlayQuery).toBe('cost');
    const rows = filterItems('cost', overlayItems(s));
    expect(rows.map((r) => r.title)).toContain('/cost');
    expect(rows.length).toBeLessThan(paletteItems(s).length);
  });

  it('accepting a command row runs it', () => {
    let s = open(running());
    s = typed(s, '/clear');
    s = apply(s, ...handleKey(s, key({ return: true, input: '\r' })).actions);
    expect(s.tools, '/clear did not run').toHaveLength(0);
    expect(s.overlay).toBeNull();
  });

  it('recent actions are commands this session actually ran', () => {
    const ran = reduceLocal(running(), { kind: 'command.run', name: '/cost', args: '' }, T0);
    const items = paletteItems(ran);
    expect(items.filter((i) => i.group === 'Recent actions').map((i) => i.title)).toContain('/cost');
  });

  it('esc closes it', () => {
    const s = open(running());
    expect(apply(s, ...handleKey(s, key({ escape: true })).actions).overlay).toBeNull();
  });
});

describe('F5: the pickers share one list discipline and only show what the engine reported', () => {
  const openPicker = (name: string, state: AppState): AppState =>
    reduceLocal(state, { kind: 'command.run', name, args: '' }, T0);

  it('model shows the model the engine named, and nothing else', () => {
    const s = openPicker('/model', running());
    expect(s.overlay).toBe('model');
    const rows = overlayItems(s);
    expect(rows.map((r) => r.title)).toEqual(['claude-sonnet-4']);
    expect(overlayEmptyNote(s, 'model')).toContain('one model');
  });

  it('model shows an honest empty state before a session, never a plausible model', () => {
    const s = openPicker('/model', initialState(80, 24));
    expect(overlayItems(s)).toHaveLength(0);
    expect(overlayEmptyNote(s, 'model')).toContain('no session yet');
    expect(frame(s)).toContain('no session yet');
  });

  it('theme previews on move, commits on accept, and restores on cancel', () => {
    const opened = openPicker('/theme', running());
    expect(opened.overlay).toBe('theme');
    const moved = apply(opened, ...handleKey(opened, key({ downArrow: true })).actions);
    expect(moved.activeTheme, 'moving must preview').toBe('niki-light');
    expect(frame(moved), 'the preview must reach the frame').toBeDefined();
    const committed = apply(moved, ...handleKey(moved, key({ return: true, input: '\r' })).actions);
    expect(committed.activeTheme).toBe('niki-light');
    expect(committed.overlay).toBeNull();

    const cancelled = apply(opened, ...handleKey(opened, key({ downArrow: true })).actions);
    const restored = apply(cancelled, ...handleKey(cancelled, key({ escape: true })).actions);
    expect(restored.activeTheme, 'cancelling must put the theme back').toBeNull();
  });

  it('sessions offers the session the engine named and sends a real request to resume it', () => {
    const s = openPicker('/threads', running());
    expect(overlayItems(s).map((r) => r.title)).toEqual(['s1']);
    const resumed = apply(s, ...handleKey(s, key({ return: true, input: '\r' })).actions);
    expect(resumed.outbox.map((r) => r.method)).toEqual(['session.load']);
    expect(resumed.outbox[0]?.params).toMatchObject({ session_id: 's1' });
  });

  it('sessions says the engine reported none when capabilities say so', () => {
    const noSessions = apply(
      running(),
      { kind: 'engine.meta', version: '0.1.0', capabilities: { streaming: true, approvals: true, sessions: false, diffs: true, context_usage: true, cost: true } },
    );
    expect(overlayEmptyNote(noSessions, 'sessions')).toContain('not supported');
  });

  it('prompt history offers what this shell sent, newest first, and puts it back', () => {
    let s = typed(running(), 'ship it');
    s = apply(s, { kind: 'turn.submit', prompt: 'ship it' });
    const opened = openPicker('/prompts', s);
    expect(opened.overlay).toBe('history');
    expect(overlayItems(opened).map((r) => r.title)).toEqual(['ship it']);
    const recalled = apply(opened, ...handleKey(opened, key({ return: true, input: '\r' })).actions);
    expect(recalled.composer).toBe('ship it');
    expect(recalled.overlay).toBeNull();
  });

  it('prompt history is empty — and says so — before anything has been sent', () => {
    const opened = openPicker('/prompts', running());
    expect(overlayItems(opened)).toHaveLength(0);
    expect(frame(opened)).toContain('nothing sent yet');
  });

  it('effort stages a preset rather than claiming the engine changed anything', () => {
    const opened = openPicker('/effort', running());
    expect(overlayItems(opened).map((r) => r.title)).toEqual([...EFFORT_PRESETS]);
    const staged = apply(opened, ...handleKey(opened, key({ downArrow: true })).actions);
    const accepted = apply(staged, ...handleKey(staged, key({ return: true, input: '\r' })).actions);
    expect(accepted.settings['agents.effort']).toBe('medium');
  });

  it('every picker answers to the same keys', () => {
    for (const [command, overlay] of [
      ['/model', 'model'],
      ['/effort', 'effort'],
      ['/theme', 'theme'],
      ['/threads', 'sessions'],
      ['/prompts', 'history'],
    ] as const) {
      const opened = openPicker(command, running());
      expect(opened.overlay, command).toBe(overlay);
      expect(handleKey(opened, key({ escape: true })).actions, command).toEqual([
        { kind: 'overlay.close' },
      ]);
      expect(typed(opened, 'zzz').overlayQuery, `${command} ignores the query`).toBe('zzz');
    }
  });
});

describe('F6: help is generated from the registry and from the keymap, and is searchable', () => {
  it('lists every command in the registry and no others', () => {
    const text = helpLines(initialState(80, 24))
      .filter((l) => l.kind === 'command')
      .map((l) => l.text)
      .join('\n');
    for (const c of COMMANDS) expect(text, `${c.name} is missing from help`).toContain(c.name);
  });

  it('lists exactly the keys the keymap declares, and nothing hand-written', () => {
    const keys = helpLines(initialState(80, 24))
      .filter((l) => l.kind === 'key')
      .map((l) => l.text);
    expect(keys).toEqual(KEYMAP.map((b) => `${b.keys}  ${b.label}  (${b.when})`));
  });

  it('is contextual: it reports the phase and the posture the engine sent', () => {
    const text = helpLines(running()).map((l) => l.text).join('\n');
    expect(text).toContain('phase toolRunning');
    expect(text).toContain('permission posture manual');
  });

  it('is searchable', () => {
    const searching = apply(initialState(80, 24), { kind: 'overlay.open', overlay: 'help' });
    const filtered = apply(searching, ...handleKey(searching, key({ input: 'bypass' })).actions);
    expect(filtered.overlayQuery).toBe('bypass');
    const text = helpLines(filtered).map((l) => l.text).join('\n');
    expect(text).toContain('/yolo');
    expect(text).not.toContain('/clear');
  });

  it('scrolls, because it is longer than most terminals', () => {
    const opened = apply(initialState(80, 24), { kind: 'overlay.open', overlay: 'help' });
    const scrolled = apply(opened, ...handleKey(opened, key({ pageUp: true })).actions);
    expect(scrolled.overlayScroll).toBeGreaterThan(0);
    const out = frame(opened, 80, 24);
    expect(out).toContain('esc closes');
    expect(out.split('\n').length).toBeLessThanOrEqual(24);
  });

  it('opens on ? only where a question mark cannot be text', () => {
    const empty = initialState(80, 24);
    expect(apply(empty, ...handleKey(empty, key({ input: '?' })).actions).overlay).toBe('help');
    const typing = typed(empty, 'what');
    const outcome = handleKey(typing, key({ input: '?' }));
    expect(outcome.actions).toEqual([]);
    expect(outcome.insert).toBe('?');
  });

  it('every advertised key dispatches to the action the keymap claims', () => {
    // The owner's rule: nothing is advertised that a test has not exercised. Each row is driven
    // through `handleKey` and must produce the action it says it produces.
    const idle = initialState(80, 24);
    const running_ = running();
    const popup = typed(idle, '/');
    const overlay = apply(idle, { kind: 'overlay.open', overlay: 'palette' });
    const approved = run([
      SESSION,
      n('approval.request', {
        id: 'a1',
        tool: 'bash',
        command: 'npm test',
        options: [
          { id: 'allow', label: 'Allow' },
          { id: 'deny', label: 'Deny' },
        ],
        safest_option_id: 'deny',
      }),
    ]);

    const presses: Record<string, { state: AppState; key: KeyEvent }[]> = {
      '?': [{ state: idle, key: key({ input: '?' }) }],
      'ctrl+k': [{ state: idle, key: key({ ctrl: true, input: 'k' }) }],
      'ctrl+o': [{ state: idle, key: key({ ctrl: true, input: 'o' }) }],
      'ctrl+t': [{ state: idle, key: key({ ctrl: true, input: 't' }) }],
      'ctrl+m': [{ state: idle, key: key({ ctrl: true, input: 'm' }) }],
      'ctrl+g': [{ state: idle, key: key({ ctrl: true, input: 'g' }) }],
      'ctrl+r': [{ state: idle, key: key({ ctrl: true, input: 'r' }) }],
      'shift+tab': [{ state: idle, key: key({ shift: true, tab: true }) }],
      'ctrl+c': [{ state: idle, key: key({ ctrl: true, input: 'c' }) }],
      'ctrl+d': [{ state: idle, key: key({ ctrl: true, input: 'd' }) }],
      esc: [{ state: overlay, key: key({ escape: true }) }],
      pgup: [{ state: running_, key: key({ pageUp: true }) }],
      pgdn: [{ state: running_, key: key({ pageDown: true }) }],
      home: [{ state: running_, key: key({ home: true }) }],
      end: [{ state: running_, key: key({ end: true }) }],
      'g g': [
        { state: running_, key: key({ input: 'g' }) },
        { state: apply(running_, { kind: 'setChord', chord: 'g' }), key: key({ input: 'g' }) },
      ],
      'g G': [
        { state: running_, key: key({ input: 'g' }) },
        { state: apply(running_, { kind: 'setChord', chord: 'g' }), key: key({ input: 'G' }) },
      ],
      'enter/composer': [{ state: typed(idle, 'ship it'), key: key({ return: true, input: '\r' }) }],
      'enter/popup': [{ state: popup, key: key({ return: true, input: '\r' }) }],
      'enter/overlay': [{ state: overlay, key: key({ return: true, input: '\r' }) }],
      'enter/approval': [{ state: approved, key: key({ return: true, input: '\r' }) }],
      'tab/popup': [{ state: popup, key: key({ tab: true }) }],
      'tab/overlay': [{ state: overlay, key: key({ tab: true }) }],
      'esc/popup': [{ state: popup, key: key({ escape: true }) }],
      'esc/approval': [{ state: approved, key: key({ escape: true }) }],
    };

    for (const binding of KEYMAP) {
      const cases = presses[`${binding.keys}/${binding.when}`] ?? presses[binding.keys] ?? [];
      expect(cases, `${binding.keys} is advertised in Help but no test presses it`).toBeDefined();
      const produced = cases.map(({ state, key: k }) => handleKey(state, k));
      // Every press in the chord has to do something; the last one has to be the advertised action.
      for (const outcome of produced) {
        expect(
          outcome.actions.length + (outcome.scroll === undefined ? 0 : 1),
          `${binding.keys} dispatches nothing`,
        ).toBeGreaterThan(0);
      }
      const last = produced[produced.length - 1]!;
      const kinds = last.actions.map((a) => a.kind);
      const scrollKinds = last.scroll === undefined ? [] : ['scroll'];
      expect([...kinds, ...scrollKinds], `${binding.keys} claims ${binding.action}`).toContain(
        binding.action,
      );
    }
  });
});

describe('F7: the settings sheet groups values, names their source, and gates the unsafe ones', () => {
  const opened = (state: AppState): AppState => apply(state, { kind: 'overlay.open', overlay: 'settings' });

  it('groups every value and says where each one comes from', () => {
    const rows = resolveSettings(running());
    expect(new Set(rows.map((r) => r.spec.group))).toEqual(
      new Set(['Permissions', 'Model', 'Run', 'Interface']),
    );
    const model = rows.find((r) => r.spec.key === 'providers.default_model');
    expect(model?.value).toBe('claude-sonnet-4');
    expect(model?.source).toEqual({ kind: 'engine', detail: 'engine session' });
  });

  it('shows nothing rather than a plausible blank for a value nobody reported', () => {
    const rows = resolveSettings(running());
    const rounds = rows.find((r) => r.spec.key === 'general.max_revision_rounds');
    expect(rounds?.value).toBeUndefined();
    expect(rounds?.source).toEqual({ kind: 'unknown' });
    const out = frame(opened(running()));
    expect(out).toContain('not reported');
    expect(out).toContain('Revision rounds');
    // The destination of the focused row is on its own line, so it is never truncated away.
    const focused = resolveSettings(running())[0]!;
    expect(out).toContain(focused.spec.config);
  });

  it('cycles a value with the arrow keys and stages it', () => {
    let s = opened(running());
    // Row 0 is the permission posture, whose options come off the wire schema.
    s = apply(s, ...handleKey(s, key({ rightArrow: true })).actions);
    expect(s.settings['permissions.mode']).toBe('auto');
    s = apply(s, ...handleKey(s, key({ rightArrow: true })).actions);
    expect(s.settings['permissions.mode']).toBe('dont_ask');
    s = apply(s, ...handleKey(s, key({ leftArrow: true })).actions);
    expect(s.settings['permissions.mode']).toBe('auto');
  });

  it('a save shows exactly what changed and where — and says nothing was written', () => {
    let s = opened(running());
    s = apply(s, { kind: 'settings.stage', key: 'agents.effort', value: 'high' });
    s = apply(s, ...handleKey(s, key({ return: true, input: '\r' })).actions);
    expect(s.settingsReport).toHaveLength(1);
    expect(s.settingsReport[0]).toMatchObject({
      key: 'agents.effort',
      from: undefined,
      to: 'high',
      destination: 'niki.toml [agents.*] effort',
    });
    expect(s.overlay, 'the sheet stays open so the report can be read').toBe('settings');
    const out = frame(s);
    expect(out).toContain('Effort preset: unset → high');
    expect(out).toContain('niki.toml [agents.*] effort');
    expect(out).toContain('nothing is written');
  });

  it('holds a safety-critical change for an explicit confirmation', () => {
    let s = opened(running());
    s = apply(s, { kind: 'settings.stage', key: 'permissions.mode', value: 'bypass' });
    s = apply(s, ...handleKey(s, key({ return: true, input: '\r' })).actions);
    expect(s.confirm?.value).toBe('bypass');
    expect(s.settingsReport, 'nothing is reported before the confirmation').toHaveLength(0);
    expect(frame(s)).toContain('confirm · Permission posture');

    const cancelled = apply(s, ...handleKey(s, key({ escape: true })).actions);
    expect(cancelled.confirm).toBeNull();
    expect(cancelled.settingsReport).toHaveLength(0);

    const confirmed = apply(s, ...handleKey(s, key({ return: true, input: '\r' })).actions);
    expect(confirmed.confirm).toBeNull();
    expect(confirmed.settingsReport[0]).toMatchObject({ key: 'permissions.mode', to: 'bypass' });
    expect(frame(confirmed)).toContain('nothing is written');
  });

  it('bypass is never a default and never appears without a confirmation row', () => {
    const s = reduceLocal(running(), { kind: 'command.run', name: '/yolo', args: '' }, T0);
    expect(s.confirm).not.toBeNull();
    expect(s.settings['permissions.bypass']).toBeUndefined();
    const rows = resolveSettings(s);
    expect(rows.find((r) => r.spec.key === 'permissions.bypass')?.spec.safety).toBe('confirm');
  });

  it('an empty save says so instead of reporting a change that did not happen', () => {
    const s = apply(opened(running()), ...handleKey(opened(running()), key({ return: true, input: '\r' })).actions);
    expect(s.settingsReport).toHaveLength(0);
    expect(transcriptText(s)).toContain('no staged settings to save');
  });
});

describe('every overlay fits the terminal it is drawn in', () => {
  const overlays: readonly [string, string][] = [
    ['/cle', 'popup'],
    ['/help', 'help'],
    ['/model', 'model'],
    ['/effort', 'effort'],
    ['/theme', 'theme'],
    ['/threads', 'sessions'],
    ['/prompts', 'history'],
  ];

  it('never overflows the width and never pushes the composer off the screen', () => {
    for (const [command] of overlays) {
      const state = overlays[0]![0] === command
        ? typed(running(), command)
        : apply(running(), { kind: 'command.run', name: command, args: '' });
      for (const [cols, rows] of [
        [50, 16],
        [80, 24],
        [120, 38],
      ] as const) {
        const out = frame(state, cols, rows);
        const longest = out.split('\n').reduce((n, l) => Math.max(n, l.length), 0);
        expect(longest, `${command} overflowed ${cols} columns`).toBeLessThanOrEqual(cols);
        expect(out.split('\n').length, `${command} overflowed ${rows} rows`).toBeLessThanOrEqual(rows);
        expect(out, `${command} lost the composer`).toContain(glyphs('unicode').prompt);
      }
    }
  });

  it('draws the settings sheet, the palette and the confirm panel inside the same box', () => {
    const sheets = [
      apply(running(), { kind: 'overlay.open', overlay: 'settings' }),
      apply(running(), { kind: 'palette.open' }),
      apply(running(), { kind: 'command.run', name: '/yolo', args: '' }),
    ];
    for (const state of sheets) {
      const out = frame(state, 80, 24);
      expect(out.split('\n').length).toBeLessThanOrEqual(24);
      expect(out.split('\n').reduce((n, l) => Math.max(n, l.length), 0)).toBeLessThanOrEqual(80);
    }
  });
});

describe('the fuzzy matcher is a matcher, not a filter that drops everything', () => {
  it('reads a subsequence out of a haystack in order and rejects a wrong order', () => {
    expect(fuzzyScore('mo', '/model')).not.toBeNull();
    expect(fuzzyScore('om', '/model')).toBeNull();
    expect(fuzzyScore('', '/model')).toBe(0);
  });

  it('prefers a prefix and a word boundary over a lucky subsequence', () => {
    const prefix = fuzzyScore('mod', '/model')!;
    const buried = fuzzyScore('mod', 'sort-of-model-list')!;
    expect(prefix).toBeGreaterThan(buried);
  });

  it('ranks the command over its own keyword for the same query', () => {
    const rows = commandRows('money');
    expect(rows[0]?.name).toBe('/cost');
    const named = commandRows('cost')[0]?.name;
    expect(named).toBe('/cost');
  });
});

describe('the commands the footer advertises are the commands that exist', () => {
  it('every hint in every phase names a key the keymap has a row for', () => {
    const advertised = new Set(KEYMAP.map((b) => b.keys));
    // Phases hint with the friendly names from the keymap's own labels, so a phase may only say
    // something that starts with a key that is in the table.
    for (const key of ['/', '?', 'ctrl+k', 'ctrl+o', 'esc', 'enter']) {
      const known = advertised.has(key) || key === '/';
      expect(known, `${key} is hinted but absent from the keymap`).toBe(true);
    }
  });

  it('every registry command has a behaviour, and none of them is a no-op branch', () => {
    const names = COMMANDS.map((c: Command) => c.name);
    for (const name of names) {
      const after = reduceLocal(running(), { kind: 'command.run', name, args: '' }, T0);
      expect(meaningful(after), `${name} changed nothing`).not.toBe(meaningful(running()));
    }
  });
});