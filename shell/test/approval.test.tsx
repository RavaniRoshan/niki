/**
 * The approval prompt: C5, and the two defects the owner said NIKI must not inherit.
 *
 * The old TUI opened its permission modal focused on **Approve**. That is the single worst default
 * a permission prompt can have: one stray Enter, and a command ran that the user never read. These
 * tests exist to make that impossible to reintroduce.
 */

import { describe, expect, it } from 'vitest';
import { App } from '../src/app.js';
import { render } from 'ink-testing-library';
import React from 'react';

import {
  classify,
  decisionFor,
  escapeDecision,
  firstDenyingOption,
  isApproving,
  safestFocus,
} from '../src/approval.js';
import { handleKey, type KeyEvent } from '../src/dispatch.js';
import { initialState, reduce, reduceLocal, type AppState, type ReduceOptions } from '../src/state.js';
import type { ServerNotification } from '../src/protocol/generated/index.js';

const T0: ReduceOptions = { nowMs: 0 };

const SESSION = {
  method: 'session.ready',
  params: {
    session_id: 's1',
    project_path: '/home/u/p',
    model: 'm',
    permission_mode: 'manual',
    branch: 'main',
    ahead: null,
    behind: null,
    resumed_messages: 0,
  },
} as ServerNotification;

/** The approval request exactly as a hostile-or-buggy engine could send it: safest = Allow. */
function approvalRequest(safestOptionId: string, options: [string, string][]): ServerNotification {
  return {
    method: 'approval.request',
    params: {
      id: 'a1',
      tool: 'bash',
      command: 'npm test',
      options: options.map(([id, label]) => ({ id, label })),
      safest_option_id: safestOptionId,
    },
  } as ServerNotification;
}

const ALLOW_THEN_DENY: [string, string][] = [
  ['allow', 'Allow'],
  ['deny', 'Deny'],
];

function withApproval(state: AppState, n: ServerNotification): AppState {
  return reduce(state, n, T0);
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

describe('C5: the prompt never opens focused on an approving option in manual mode', () => {
  it('resolves to Deny when the engine names Allow as safest, in manual mode', () => {
    const state = withApproval(
      withApproval(initialState(80, 24), SESSION),
      approvalRequest('allow', ALLOW_THEN_DENY),
    );
    expect(state.approval?.focusedOptionId).toBe('deny');
  });

  it('resolves to Deny when the posture has not been reported yet', () => {
    // No `session.ready` yet: we do not know the posture, so we take the stricter reading.
    const state = withApproval(initialState(80, 24), approvalRequest('allow', ALLOW_THEN_DENY));
    expect(state.session).toBeNull();
    expect(state.approval?.focusedOptionId).toBe('deny');
  });

  it('follows the engine when the engine names a deny option', () => {
    const state = withApproval(
      withApproval(initialState(80, 24), SESSION),
      approvalRequest('deny', ALLOW_THEN_DENY),
    );
    expect(state.approval?.focusedOptionId).toBe('deny');
  });

  it('follows the engine when the engine names an allow option outside manual mode', () => {
    const auto = {
      ...SESSION,
      params: { ...(SESSION.params as object), permission_mode: 'auto' },
    } as ServerNotification;
    const state = withApproval(
      withApproval(initialState(80, 24), auto),
      approvalRequest('allow', ALLOW_THEN_DENY),
    );
    expect(state.approval?.focusedOptionId).toBe('allow');
  });

  it('falls back to a refusal when the engine names an option that does not exist', () => {
    const state = withApproval(
      withApproval(initialState(80, 24), SESSION),
      approvalRequest('nonexistent', ALLOW_THEN_DENY),
    );
    expect(state.approval?.focusedOptionId).toBe('deny');
  });

  it('renders the focused option as the one the footer says it is', () => {
    const state = withApproval(
      withApproval(initialState(80, 24), SESSION),
      approvalRequest('allow', ALLOW_THEN_DENY),
    );
    const { lastFrame } = render(
      <App state={state} theme="niki" charset="unicode" reducedMotion />,
    );
    const out = lastFrame() ?? '';
    // The focused option carries the marker and the default hint; Approve must not.
    expect(out).toContain('Deny');
    expect(out).toContain('esc denies');
  });
});

describe('C5: Esc denies', () => {
  it('Esc resolves to the engine deny option, not to Allow and not to "close"', () => {
    const state = withApproval(
      withApproval(initialState(80, 24), SESSION),
      approvalRequest('allow', ALLOW_THEN_DENY),
    );
    const outcome = handleKey(state, key({ escape: true, input: '' }));
    expect(outcome.approval).toEqual({ id: 'a1', optionId: 'deny' });
  });

  it('Esc denies even when the user had moved focus to Allow first', () => {
    const state = withApproval(
      withApproval(initialState(80, 24), SESSION),
      approvalRequest('allow', ALLOW_THEN_DENY),
    );
    const moved = reduceLocal(state, { kind: 'approval.focus', optionId: 'allow' }, T0);
    expect(moved.approval?.focusedOptionId).toBe('allow');
    const outcome = handleKey(moved, key({ escape: true }));
    expect(outcome.approval?.optionId).toBe('deny');
  });

  it('Esc denies when the engine offered no option that reads as a refusal', () => {
    const state = withApproval(
      withApproval(initialState(80, 24), SESSION),
      approvalRequest('allow', [
        ['allow', 'Allow'],
        ['later', 'Ask again later'],
      ]),
    );
    const outcome = handleKey(state, key({ escape: true }));
    // Nothing is a refusal, so the fallback is the last option — never the first, which is Allow.
    expect(outcome.approval?.optionId).toBe('later');
  });

  it('Enter confirms the focused option, which is Deny by default', () => {
    const state = withApproval(
      withApproval(initialState(80, 24), SESSION),
      approvalRequest('allow', ALLOW_THEN_DENY),
    );
    expect(handleKey(state, key({ return: true })).approval).toEqual({
      id: 'a1',
      optionId: 'deny',
    });
  });

  it('every decision is logged as an action, so nothing disappears silently', () => {
    const state = withApproval(
      withApproval(initialState(80, 24), SESSION),
      approvalRequest('allow', ALLOW_THEN_DENY),
    );
    const outcome = handleKey(state, key({ escape: true }));
    expect(outcome.actions.map((a) => a.kind)).toContain('approval.decide');
  });
});

describe('the decision that goes over the seam matches the option chosen', () => {
  it('maps an allow option to allow and a deny option to deny', () => {
    expect(decisionFor({ id: 'allow', label: 'Allow' })).toBe('allow');
    expect(decisionFor({ id: 'deny', label: 'Deny' })).toBe('deny');
  });

  it('maps a session-wide allow to allow_always, and nothing else to it', () => {
    expect(decisionFor({ id: 'allow_always', label: 'Allow always' })).toBe('allow_always');
    expect(decisionFor({ id: 'allow', label: 'Allow' })).toBe('allow');
  });

  it('never sends allow for an option that does not read as an approval', () => {
    for (const id of ['deny', 'reject', 'no', 'block']) {
      expect(decisionFor({ id, label: id })).toBe('deny');
    }
  });
});

describe('option classification', () => {
  it('reads the label as well as the id', () => {
    expect(classify({ id: 'x', label: 'Deny' })).toBe('deny');
    expect(classify({ id: 'x', label: 'Allow always' })).toBe('allow');
  });

  it('treats an unrecognisable option as neither, rather than guessing allow', () => {
    expect(classify({ id: 'x', label: 'Ask again later' })).toBe('other');
    expect(isApproving({ id: 'x', label: 'Ask again later' })).toBe(false);
  });

  it('finds the first refusal when one exists', () => {
    expect(firstDenyingOption([{ id: 'allow', label: 'Allow' }, { id: 'deny', label: 'Deny' }])?.id).toBe(
      'deny',
    );
    expect(firstDenyingOption([{ id: 'allow', label: 'Allow' }])).toBeUndefined();
  });

  it('escapeDecision prefers a refusal and otherwise takes the last option', () => {
    expect(escapeDecision({ options: [{ id: 'deny', label: 'Deny' }] })?.id).toBe('deny');
    expect(
      escapeDecision({ options: [{ id: 'a', label: 'A' }, { id: 'b', label: 'B' }] })?.id,
    ).toBe('b');
  });
});

describe('moving focus inside the prompt', () => {
  it('arrows and j/k both move, and wrap', () => {
    const state = withApproval(
      withApproval(initialState(80, 24), SESSION),
      approvalRequest('deny', ALLOW_THEN_DENY),
    );
    const down = handleKey(state, key({ downArrow: true }));
    expect(down.actions).toEqual([{ kind: 'approval.focus', optionId: 'allow' }]);

    const up = handleKey(state, key({ upArrow: true }));
    expect(up.actions).toEqual([{ kind: 'approval.focus', optionId: 'allow' }]);

    const j = handleKey(state, key({ input: 'j' }));
    expect(j.actions).toEqual([{ kind: 'approval.focus', optionId: 'allow' }]);
  });

  it('focus never falls off the end of the list', () => {
    const state = withApproval(
      withApproval(initialState(80, 24), SESSION),
      approvalRequest('deny', ALLOW_THEN_DENY),
    );
    let next = state;
    for (let i = 0; i < 5; i += 1) {
      next = reduceLocal(next, { kind: 'approval.focus', optionId: 'allow' }, T0);
      next = reduceLocal(next, { kind: 'approval.focus', optionId: 'deny' }, T0);
    }
    expect(next.approval?.focusedOptionId).toBe('deny');
  });
});