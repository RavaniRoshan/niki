/**
 * The one fuzzy matcher the popup, the palette and the pickers share.
 *
 * Written here rather than pulled in: the shell has no runtime dependency budget to spend on a
 * five-line subsequence walk, and a matcher this small is easier to pin with a test than to audit
 * in a package. Scoring prefers a match that starts at a word boundary, rewards runs of adjacent
 * characters, and breaks ties towards the shorter candidate, so `/mo` prefers `/model` over
 * `/line-numbers`.
 *
 * It is a pure function of two strings. Nothing here reads state, and nothing here mutates.
 */

export type Candidate<T> = {
  /** The haystacks, in the order they should be preferred: name before keyword before prose. */
  readonly fields: readonly string[];
  /** Carried through untouched so a filter can return the original row. */
  readonly value: T;
};

/** Fields are weighted: an exact-ish hit on `name` beats a hit buried in a description. */
const FIELD_WEIGHT = [1, 0.7, 0.45];

/**
 * Subsequence score, or `null` when `needle` cannot be read out of `haystack` in order.
 * Higher is better; `0` is a perfect match on an empty needle.
 */
export function fuzzyScore(needle: string, haystack: string): number | null {
  if (needle === '') return 0;
  const n = needle.toLowerCase();
  const h = haystack.toLowerCase();
  if (n.length > h.length) return null;

  let score = 0;
  let cursor = 0;
  let streak = 0;
  for (let i = 0; i < n.length; i += 1) {
    const ch = n[i]!;
    const at = h.indexOf(ch, cursor);
    if (at === -1) return null;
    streak = at === cursor && i > 0 ? streak + 1 : 0;
    score += 10 + streak * 5;
    // A hit at a word boundary is a hit the user meant.
    if (at === 0 || h[at - 1] === ' ' || h[at - 1] === '/' || h[at - 1] === '-') score += 8;
    cursor = at + 1;
  }
  if (h.startsWith(n)) score += 25;
  // Shorter haystacks win ties, so a keyword never outranks the command it belongs to.
  return score - h.length * 0.1;
}

/** Best weighted score across a candidate's fields, or `null` when nothing matches. */
export function scoreCandidate(query: string, fields: readonly string[]): number | null {
  let best: number | null = null;
  for (let i = 0; i < fields.length; i += 1) {
    const score = fuzzyScore(query, fields[i] ?? '');
    if (score === null) continue;
    const weighted = score * (FIELD_WEIGHT[i] ?? 0.3);
    if (best === null || weighted > best) best = weighted;
  }
  return best;
}

/**
 * Filters and orders candidates for a query. An empty query keeps the registry's own order,
 * because "everything, in the order it was declared" is a better answer than "everything, scored".
 */
export function fuzzyFilter<T>(
  query: string,
  candidates: readonly Candidate<T>[],
): readonly { readonly value: T; readonly score: number }[] {
  if (query.trim() === '') {
    return candidates.map((c) => ({ value: c.value, score: 0 }));
  }
  const scored: { value: T; score: number }[] = [];
  for (const candidate of candidates) {
    const score = scoreCandidate(query.trim(), candidate.fields);
    if (score !== null) scored.push({ value: candidate.value, score });
  }
  return scored.sort((a, b) => b.score - a.score);
}