/**
 * Derived views over the one command registry.
 *
 * Nothing here declares a command. Every row is {@link COMMANDS} from `components/footer.tsx`,
 * which is the single place a command is written down; this module only looks things up and
 * filters. `test/lint.test.ts` fails the build if a second module ever grows its own list.
 */

import { COMMANDS, type Command } from '../components/footer.js';
import { fuzzyFilter, type Candidate } from './fuzzy.js';

/** The strings a command can be found by: its name, then its aliases, then the hidden keywords. */
export function commandFields(command: Command): readonly string[] {
  return [command.name, ...command.aliases, ...command.keywords, command.description];
}

/** Exact lookup by name or alias, case-insensitively. `undefined` when the registry has no such row. */
export function commandFor(token: string): Command | undefined {
  const wanted = token.toLowerCase();
  return COMMANDS.find(
    (c) =>
      c.name.toLowerCase() === wanted ||
      c.aliases.some((a) => a.toLowerCase() === wanted),
  );
}

/**
 * The slash popup's rows for a query. An empty query keeps the registry's order; anything else is
 * fuzzy-ranked, so `/mo`, `/model` and the hidden keyword `llm` all land on the same command.
 */
export function commandRows(query: string): readonly Command[] {
  const candidates: Candidate<Command>[] = COMMANDS.map((c) => ({
    fields: commandFields(c),
    value: c,
  }));
  return fuzzyFilter(query, candidates).map((hit) => hit.value);
}

/** The row the popup has highlighted, clamped into range so an empty list highlights nothing. */
export function selectedCommand(query: string, selected: number): Command | undefined {
  const rows = commandRows(query);
  if (rows.length === 0) return undefined;
  const index = Math.min(Math.max(0, selected), rows.length - 1);
  return rows[index];
}