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
    testTimeout: 30_000,
    hookTimeout: 30_000,
  },
});
