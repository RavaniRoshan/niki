'use strict';

// Uses Node's built-in test runner. There are no dependencies to install,
// because a first run that needs `npm install` is a first run that needs the
// network, and the whole point of this project is that it works with nothing
// but Node and a local model.
//
//   node --test test/
//
// At the start this file fails. That is intentional and it is checked: see
// `test/starter-is-red.test.js`, which fails if these tests ever start
// passing on their own. A starter whose task has quietly been completed is a
// starter that teaches nothing.

const test = require('node:test');
const assert = require('node:assert');
const { createServer } = require('../src/server.js');

/** Start the server on an ephemeral port, run `fn`, always shut it down. */
async function withServer(fn) {
  const server = createServer();
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const { port } = server.address();
  try {
    await fn(`http://127.0.0.1:${port}`);
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
}

test('GET / returns the service name', async () => {
  await withServer(async (base) => {
    const res = await fetch(`${base}/`);
    assert.strictEqual(res.status, 200);
    assert.strictEqual((await res.json()).name, 'niki-starter');
  });
});

test('GET /health returns 200', async () => {
  await withServer(async (base) => {
    const res = await fetch(`${base}/health`);
    assert.strictEqual(
      res.status,
      200,
      '/health is not implemented yet — this is the task NIKI is given'
    );
  });
});

test('GET /health returns exactly { status: "ok" }', async () => {
  await withServer(async (base) => {
    const res = await fetch(`${base}/health`);
    assert.deepStrictEqual(await res.json(), { status: 'ok' });
  });
});

test('GET /health is served as JSON', async () => {
  await withServer(async (base) => {
    const res = await fetch(`${base}/health`);
    assert.match(res.headers.get('content-type') || '', /application\/json/);
  });
});

test('an unknown path is still a 404', async () => {
  await withServer(async (base) => {
    const res = await fetch(`${base}/nope`);
    assert.strictEqual(res.status, 404);
  });
});
