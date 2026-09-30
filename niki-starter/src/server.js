'use strict';

// A deliberately incomplete HTTP server.
//
// `GET /health` is what the test suite asks for and what this file does not
// implement. That gap is the point: NIKI's first job on a beginner's machine
// is to close a real, failing, verifiable gap — not to produce a diff that
// looks busy.

const http = require('node:http');

function createServer() {
  return http.createServer((req, res) => {
    const url = new URL(req.url, `http://${req.headers.host || 'localhost'}`);

    if (url.pathname === '/') {
      res.writeHead(200, { 'content-type': 'application/json' });
      res.end(JSON.stringify({ name: 'niki-starter', version: '1.0.0' }));
      return;
    }

    // TODO(niki): implement `GET /health`.
    // It must respond 200 with a JSON body of exactly `{ "status": "ok" }`.
    // See test/server.test.js for what is actually asserted.

    res.writeHead(404, { 'content-type': 'application/json' });
    res.end(JSON.stringify({ error: 'not found' }));
  });
}

module.exports = { createServer };
