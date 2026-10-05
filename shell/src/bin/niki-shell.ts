/**
 * The shell's executable entry point.
 *
 * `src/cli.tsx` exports `main()` so tests can drive it with a scripted engine. That also means
 * importing it does nothing by itself, which made `npm start` exit silently on the first frame.
 * This file is the thing you actually run.
 *
 *   npx tsx src/bin/niki-shell.ts [--engine <path>] [--engine-arg <arg>]... [--theme <name>]
 */

import { main } from '../cli.js';

main(process.argv.slice(2))
  .then((code) => {
    process.exitCode = code;
  })
  .catch((error: unknown) => {
    // Startup failures happen before the interface exists, so stderr is the only place left.
    process.stderr.write(`niki-shell: ${error instanceof Error ? error.stack : String(error)}\n`);
    process.exitCode = 1;
  });
