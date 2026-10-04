import { defineConfig } from 'vitest/config';

export default defineConfig({
  esbuild: { jsx: 'automatic' },
  test: {
    globals: true,
    include: ['test/**/*.test.ts', 'test/**/*.test.tsx'],
    // One worker: the fast suites build large states and this box shares its RAM with another
    // agent session. The suites that spawn pseudo-terminals are excluded from the default run and
    // driven by `npm run test:pty` - a terminal-owning test that hangs the shared worker takes
    // every unrelated test with it, and it did exactly that.
    pool: 'forks',
    poolOptions: { forks: { singleFork: true } },
    testTimeout: 60_000,
    // The PTY suites boot the real shell through tsx, which compiles the import graph on the
    // way in. That is seconds, not milliseconds, and a `beforeAll` that boots one shared session
    // has to be allowed to take them.
    hookTimeout: 180_000,
  },
});
