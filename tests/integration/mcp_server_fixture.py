#!/usr/bin/env python3
"""A real MCP server, in one file, for tests to talk to.

An external boundary, and therefore scripted — but a *real* one: newline
delimited JSON-RPC 2.0 over stdio, the same protocol `src/mcp/client.rs`
speaks. A test that stubs `McpManager::call_tool` proves nothing about the
call path; this proves the client can initialise, list and call against a
process it did not control the inside of.

It advertises two tools:

  `echo`      read-only, returns its arguments. The tool every other MCP
              server has, and the one a round-trip needs.
  `write_note` NOT marked read-only, so `read_only` governance denies it.
              That is the point: the default posture has to be able to refuse
              a mutating tool from a server it has never seen.

Set `MCP_FIXTURE_FAIL=1` to make `tools/call` return a JSON-RPC error, so the
error path is a real transport error rather than a synthesised one.

Usage: tests/integration/mcp_server_fixture.py
"""

import json
import os
import sys

PROTOCOL_VERSION = "2024-11-05"

TOOLS = [
    {
        "name": "echo",
        "description": "Return the arguments you were given. Useful for checking "
        "that a call actually reached the server.",
        "inputSchema": {
            "type": "object",
            "properties": {"text": {"type": "string"}},
            "required": ["text"],
        },
        "annotations": {"readOnlyHint": True},
    },
    {
        "name": "write_note",
        "description": "Pretend to write a note somewhere. Not read-only, so a "
        "read-only governance policy must refuse it.",
        "inputSchema": {
            "type": "object",
            "properties": {"body": {"type": "string"}},
        },
        "annotations": {"readOnlyHint": False},
    },
]


def reply(msg_id, result):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": msg_id, "result": result}) + "\n")
    sys.stdout.flush()


def fail(msg_id, code, message):
    sys.stdout.write(
        json.dumps(
            {"jsonrpc": "2.0", "id": msg_id, "error": {"code": code, "message": message}}
        )
        + "\n"
    )
    sys.stdout.flush()


def handle(msg):
    method = msg.get("method")
    msg_id = msg.get("id")
    params = msg.get("params") or {}

    # A notification has no id and takes no reply.
    if msg_id is None:
        return

    if method == "initialize":
        reply(
            msg_id,
            {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "niki-test-fixture", "version": "1"},
            },
        )
    elif method == "tools/list":
        reply(msg_id, {"tools": TOOLS})
    elif method == "tools/call":
        if os.environ.get("MCP_FIXTURE_FAIL") == "1":
            fail(msg_id, -32000, "the fixture was told to fail")
            return
        name = params.get("name")
        if name == "echo":
            text = (params.get("arguments") or {}).get("text", "")
            reply(
                msg_id,
                {
                    "content": [{"type": "text", "text": f"echo: {text}"}],
                    "isError": False,
                },
            )
        elif name == "write_note":
            body = (params.get("arguments") or {}).get("body", "")
            reply(
                msg_id,
                {"content": [{"type": "text", "text": f"noted: {body}"}], "isError": False},
            )
        else:
            fail(msg_id, -32601, f"unknown tool {name!r}")
    else:
        fail(msg_id, -32601, f"unknown method {method!r}")


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            handle(json.loads(line))
        except Exception as exc:  # noqa: BLE001 - a fixture must not die silently
            sys.stderr.write(f"fixture error: {exc}\n")
            sys.stderr.flush()


if __name__ == "__main__":
    main()
