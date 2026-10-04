/**
 * Approval decisions, decided once.
 *
 * Two rules from the spec live here, and both are enforced by tests rather than by discipline:
 *
 *  1. **The prompt never opens focused on an approving option in manual mode.** The engine names a
 *     safest option, and the shell follows it — but if that named option is an approving one and
 *     the posture is manual (or not yet known), the shell resolves to a deny option anyway. A
 *     hostile or buggy engine must not be able to talk the user into one stray Enter.
 *  2. **Esc means deny.** Not "close", not "defer": deny.
 *
 * This module is pure. It has no Ink, no terminal and no engine, so it can be tested exhaustively.
 */

import type { ApprovalOption, ApprovalRequestParams } from './protocol/generated/index.js';
import type { PermissionMode } from './protocol/generated/index.js';

export type OptionKind = 'allow' | 'deny' | 'other';

/**
 * Classifies an option by its id and label. The wire format carries no kind, because the engine
 * does not know how a given host renders a decision — but it always tells us enough to tell an
 * approval from a refusal, and that is the distinction C5 is about.
 */
export function classify(option: Pick<ApprovalOption, 'id' | 'label'>): OptionKind {
  const haystack = `${option.id} ${option.label}`.toLowerCase();
  if (/\b(deny|den|no|reject|refuse|block|never)\b/.test(haystack)) return 'deny';
  if (/\b(allow|approve|yes|accept|grant|permit|always)\b/.test(haystack)) return 'allow';
  return 'other';
}

export function isApproving(option: Pick<ApprovalOption, 'id' | 'label'>): boolean {
  return classify(option) === 'allow';
}

export function isDenying(option: Pick<ApprovalOption, 'id' | 'label'>): boolean {
  return classify(option) === 'deny';
}

/** The first option that reads as a refusal, if there is one. */
export function firstDenyingOption(options: readonly ApprovalOption[]): ApprovalOption | undefined {
  return options.find(isDenying);
}

/**
 * Which option the prompt opens focused on.
 *
 * The engine's `safest_option_id` wins when it is safe. When the posture is manual — or when the
 * engine has not told us the posture yet, which we treat as the stricter of the two — an
 * approving option is never the initial focus.
 */
export function safestFocus(
  request: Pick<ApprovalRequestParams, 'options' | 'safest_option_id'>,
  mode: PermissionMode | undefined,
): string {
  const { options, safest_option_id: named } = request;
  if (options.length === 0) return named;

  const byId = new Map(options.map((o) => [o.id, o]));
  const namedOption = byId.get(named);

  // Manual, or unknown: treat as manual. Deny-first is the safe reading of "we do not know yet".
  const strict = mode === undefined || mode === 'manual';

  if (namedOption && (!strict || !isApproving(namedOption))) return named;
  if (strict) {
    const deny = firstDenyingOption(options);
    if (deny) return deny.id;
  }
  if (namedOption) return named;
  // Nothing was named usefully: fall back to a refusal, then to the first option.
  return firstDenyingOption(options)?.id ?? options[0]!.id;
}

/** Maps a chosen option to the decision that goes over the seam. */
export function decisionFor(option: ApprovalOption): 'allow' | 'deny' | 'allow_always' {
  const id = option.id.toLowerCase();
  const label = option.label.toLowerCase();
  if (/\b(always|all|session|permanent)\b/.test(`${id} ${label}`)) return 'allow_always';
  return classify(option) === 'allow' ? 'allow' : 'deny';
}

/**
 * What Esc does: choose the engine's own refusal option, or the last option as a fallback when the
 * engine offered none that reads as one.
 */
export function escapeDecision(request: Pick<ApprovalRequestParams, 'options'>): ApprovalOption | undefined {
  return firstDenyingOption(request.options) ?? request.options[request.options.length - 1];
}