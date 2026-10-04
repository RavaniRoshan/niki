import { defineConfig } from 'vitest/config';

export default defineConfig({
  esbuild: { jsx: 'automatic' },
  test: {
    globals: true,
    include: ['test/**/*.test.ts', 'test/**/*.test.tsx'],
    // Snapshot and property tests build large states; a single worker keeps peak memory bounded
    // on a machine that shares its RAM with another agent session.
    pool: 'forks',
    poolOptions: { forks: { singleFork: true } },
    testTimeout: 60_000,
    // The PTY suites boot the real shell through tsx, which compiles the import graph on the
    // way in. That is seconds, not milliseconds, and a `beforeAll` that boots one shared session
    // has to be allowed to take them.
    hookTimeout: 180_000,
  },
});
