'use strict';

// This test is about the starter itself, not about the task.
//
// `niki-starter` exists so that a first run of NIKI has something real to fix:
// a test suite that genuinely fails before the agents touch it. The moment
// somebody merges a fix for `/health` into this branch — the obvious way for
// this project to "tidy itself up" — every subsequent student run would start
// from green, the Coder would have nothing to do, and the whole thing would
// quietly stop teaching anything.
//
// So: this test fails if `test/server.test.js` passes. A starter that has been
// solved is a broken starter, and this is what notices.

const test = require('node:test');
const assert = require('node:assert');
const { spawnSync } = require('node:child_process');
const path = require('node:path');

test('the starter task is still unsolved', () => {
  // `spawnSync` rather than `execFileSync`, and the status is read from the
  // result rather than inferred from a throw. Inferring a failure from a
  // thrown exception is the kind of cleverness that produces a test which
  // passes for the wrong reason: the first version of this file caught the
  // non-zero exit as an exception, and under the test runner it did not — so it
  // asserted the starter was red while it was red for an unrelated reason.
  // The child needs `NODE_TEST_CONTEXT` scrubbed. Node sets it in the
  // environment of a process that is itself running under `--test`, and a
  // child that inherits it treats `--test` as already-satisfied: the file is
  // executed rather than run by the harness, the assertions inside it still
  // throw, and the process exits 0. So the child "passes" while the starter is
  // plainly red — which is how the first two versions of this test both
  // reported a green starter and, when that was fixed, reported it red for a
  // reason that had nothing to do with /health.
  const env = { ...process.env };
  delete env.NODE_TEST_CONTEXT;
  delete env.NODE_OPTIONS;

  const run = spawnSync(
    process.execPath,
    ['--test', path.join(__dirname, 'server.test.js')],
    { encoding: 'utf8', env }
  );

  const output = `${run.stdout || ''}${run.stderr || ''}`;
  const failed = run.status !== 0;

  assert.ok(
    failed,
    `the starter's own test suite now passes, so there is no task left for NIKI to do.\n` +
      `Either revert the fix in src/server.js, or give the starter a new unsolved ` +
      `requirement. A starter that is already solved teaches nothing.\n\n${output}`
  );

  // And the failure should be about /health, not about a typo that would make
  // the exercise look broken rather than unfinished.
  assert.match(
    output,
    /health/i,
    `the starter's tests fail, but not for the reason this exercise is about.\n` +
      `They should fail because /health is unimplemented.\n\n${output}`
  );
});
