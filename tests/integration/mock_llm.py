#!/usr/bin/env python3
"""Minimal OpenAI + Anthropic compatible mock LLM server for integration testing.

Streams SSE responses in the correct format for each provider:
- OpenAI: data-only SSE with choices[0].delta.content + final usage chunk
- Anthropic: event+data SSE with content_block_delta + message_start/delta

Returns schema-valid JSON artifacts per agent role.
"""
import json
import os
import sys
import time
import uuid
import http.server
import socketserver
from datetime import datetime, timezone

PORT = int(os.environ.get("MOCK_LLM_PORT", "8080"))
TOKEN_DELAY = float(os.environ.get("MOCK_TOKEN_DELAY", "0.01"))  # delay between tokens

ROLE_RESPONSES = {
    "planner": {
        "summary": "Add health check endpoint returning JSON status",
        "approach": "Create a new GET /health route in the server file that responds with {\"status\":\"ok\"} and 200 status code.",
        "files_to_modify": [
            {
                "path": "server.js",
                "action": "create",
                "description": "New HTTP server with /health endpoint"
            }
        ],
        "acceptance_criteria": [
            "GET /health returns 200",
            "GET /health returns {\"status\":\"ok\"}",
            "No existing routes are broken"
        ],
        "constraints": [
            "Use only Node.js built-in modules",
            "Listen on port 3000"
        ],
        "estimated_complexity": "low"
    },
    "coder": {
        "edits": [
            {
                "search": "console.log(\"hello\");",
                "replace": (
                    "const http = require('http');\n\n"
                    "const server = http.createServer((req, res) => {\n"
                    "  if (req.url === '/health' && req.method === 'GET') {\n"
                    "    res.writeHead(200, { 'Content-Type': 'application/json' });\n"
                    "    res.end(JSON.stringify({ status: 'ok' }));\n"
                    "    return;\n"
                    "  }\n"
                    "  res.writeHead(404);\n"
                    "  res.end('Not Found');\n"
                    "});\n"
                    "server.listen(3000, () => {\n"
                    "  console.log('Server running on port 3000');\n"
                    "});"
                )
            }
        ],
        "files_changed": [
            {
                "path": "index.js",
                "action": "modify",
                "language": "javascript"
            }
        ],
        "implementation_notes": "Added HTTP server with /health endpoint returning JSON status.",
        "spec_adherence": "Fully implements the planner's specification."
    },
    "tester": {
        "tests_written": [
            {
                "name": "health endpoint returns 200",
                "file_path": "tests/health.test.js",
                "description": "Verify GET /health returns HTTP 200",
                "status": "passed",
                "error_message": None
            },
            {
                "name": "health endpoint returns correct JSON",
                "file_path": "tests/health.test.js",
                "description": "Verify GET /health returns {\"status\":\"ok\"}",
                "status": "passed",
                "error_message": None
            },
            {
                "name": "non-existent route returns 404",
                "file_path": "tests/health.test.js",
                "description": "Verify unknown routes return 404",
                "status": "passed",
                "error_message": None
            }
        ],
        "test_results": {
            "total": 3,
            "passed": 3,
            "failed": 0,
            "skipped": 0,
            "errors": 0
        },
        "coverage_summary": {
            "line_coverage_percent": 85.0,
            "branch_coverage_percent": None,
            "uncovered_files": []
        },
        "edge_cases_found": [
            "What happens if POST is sent to /health?",
            "Server behavior on port already in use"
        ],
        "tester_notes": "All core acceptance criteria are tested."
    },
    "reviewer": {
        "verdict": "approved",
        "overall_assessment": "The implementation correctly adds the health check endpoint.",
        "quality_scores": {
            "correctness": 9,
            "code_quality": 9,
            "test_coverage": 8,
            "spec_adherence": 10
        },
        "issues": [
            {
                "severity": "nit",
                "category": "style",
                "file_path": "index.js",
                "line_range": None,
                "description": "Consider adding a JSDoc comment for the request handler",
                "suggested_fix": None
            }
        ],
        "strengths": [
            "Clean implementation using only built-in modules",
            "Correct HTTP status codes"
        ],
        "red_reconciliation": None,
        "feedback": None
    },
    "security_auditor": {
        "verdict": "pass",
        "overall_assessment": "No security-relevant surface was added: the change adds a static route with no user input, no shell, no filesystem, and no network egress.",
        "findings": [],
        "strengths": [
            "No untrusted input reaches a sink",
            "No new dependencies or dynamic evaluation"
        ]
    },
    "critic": {
        "disposition": "no_material_issues",
        "summary": "The reviewer's single nit is a style comment, not a correctness or security claim, so there is nothing here for an adversarial pass to refute.",
        "unsupported_claims": [],
        "confirmed_findings": []
    }
}


# ── Scripted runs ─────────────────────────────────────────────────────────
# Everything above is one fixed story: a JS health endpoint. That is the demo,
# and a demo can only ever prove the demo.
#
# Every other end-to-end path in this repository wants the same machinery over
# a *different* task — a Python fixture whose tests start red, a bug class, a
# refusal to emit a patch. Before this, each of those wanted its own server, so
# each of them did not exist, so the one server that exists is the only story
# anyone can tell end to end. That is how a codebase ends up unable to test
# its own second scenario.
#
# `MOCK_LLM_SCRIPT=<file.json>` overrides the per-role artifacts and,
# optionally, the model catalogue:
#
#   {"responses": {"coder": {...}}, "models": [{"id": "x"}]}
#
# Roles not named fall back to the built-in story, so a script only has to
# say the part it cares about.
SCRIPT_PATH = os.environ.get("MOCK_LLM_SCRIPT", "")
SCRIPTED_MODELS = None
SCRIPTED_TOOL_LOOP = False
# An explicit sequence of tool calls for the loop to follow, in order:
#   {"tool_calls": [{"name": "ask_user", "arguments": {...}}, {"name":
#    "submit_artifact"}]}
#
# Without it the loop's script is two hardcoded calls — read, then submit — so
# no test could drive any *other* tool. `ask_user` and `approval` were
# therefore unreachable from an end-to-end leg: the unit tests exercise the
# adapter and the modal, and nothing exercises the two together through a real
# run, which is the only place a modal can be wrong in a way no unit test sees
# — a key swallowed by the ladder, a question the loop never gets to.
SCRIPTED_TOOL_CALLS = []
if SCRIPT_PATH:
    try:
        with open(SCRIPT_PATH, encoding="utf-8") as fh:
            _script = json.load(fh)
    except (OSError, ValueError) as exc:  # pragma: no cover - test-infra guard
        sys.stderr.write(f"[mock_llm] MOCK_LLM_SCRIPT={SCRIPT_PATH} unreadable: {exc}\n")
        raise SystemExit(2)
    for _role, _body in (_script.get("responses") or {}).items():
        ROLE_RESPONSES[_role] = _body
    if _script.get("models") is not None:
        SCRIPTED_MODELS = _script["models"]
    SCRIPTED_TOOL_LOOP = bool(_script.get("tool_loop"))
    SCRIPTED_TOOL_CALLS = list(_script.get("tool_calls") or [])
    sys.stderr.write(
        f"[mock_llm] scripted: roles={sorted(_script.get('responses') or {})} "
        f"tool_loop={SCRIPTED_TOOL_LOOP} "
        f"models={'yes' if SCRIPTED_MODELS is not None else 'built-in'}\n"
    )


# Substring of the system prompt -> role. Order matters: the more specific
# persona lines are checked first.
ROLE_MARKERS = [
    ("security auditing agent", "security_auditor"),
    ("code review agent", "reviewer"),
    ("testing agent", "tester"),
    ("implementation agent", "coder"),
    ("planning agent", "planner"),
]

# What a request looks like when no marker matched. Previously this returned
# "planner" unconditionally, which meant a role nobody had wired up — a new
# stage, a renamed persona, a security_auditor — was answered with a TaskSpec
# and failed schema validation far downstream, with an error that pointed at the
# schema rather than at the mock. Every consumer of this script paid that
# discovery cost separately.
ROLE_FALLBACK = "critic"


def detect_role(body):
    """Detect agent role from system prompt content.

    Falls back to the Critic, which is the one role whose output is a critique of
    another agent's work and so is the least wrong thing to hand back when the
    prompt is not recognised. It is still a guess, so it is logged: a mock that
    quietly answers a question it did not understand is how a test ends up
    asserting on the wrong artifact.
    """
    text = json.dumps(body).lower()
    for marker, role in ROLE_MARKERS:
        if marker in text:
            return role
    print(
        f"[mock_llm] no role marker matched; answering as {ROLE_FALLBACK}. "
        f"If this run is meant to exercise another role, add a marker for it.",
        file=sys.stderr,
    )
    return ROLE_FALLBACK


def chunk_text(text, size=20):
    """Split text into chunks to simulate streaming tokens."""
    return [text[i:i+size] for i in range(0, len(text), size)]


def _tool_result_seen(body):
    """True once the conversation carries an answer back from a tool.

    OpenAI spells it `{"role": "tool"}`; Anthropic spells it a `tool_result`
    content block. Checking for the string "tool_result" alone — which is what
    this used to do — matches neither, so the mock never noticed it had been
    called back and answered identically forever.
    """
    for message in body.get("messages") or []:
        if message.get("role") == "tool":
            return True
        content = message.get("content")
        if isinstance(content, list):
            for block in content:
                if isinstance(block, dict) and block.get("type") == "tool_result":
                    return True
    return False


def seen_count(body):
    """How many tool results the conversation carries, on either wire format.

    OpenAI sends a `{role: tool}` turn; Anthropic puts `tool_result` blocks in
    a user turn. A scenario scripted on one provider and run on the other must
    count the same, or the same script replays a call for ever.
    """
    n = 0
    content = body.get("messages")
    if isinstance(content, list):
        for turn in content:
            if turn.get("role") == "tool":
                n += 1
            blocks = turn.get("content")
            if isinstance(blocks, list):
                for block in blocks:
                    if isinstance(block, dict) and block.get("type") == "tool_result":
                        n += 1
    return n


def _next_scripted_call(body):
    """The Nth scripted call, where N is how many tool results came back.

    `None` once the script is exhausted, so the caller falls back to whatever
    it does without one.

    Keyed on the count of results rather than a turn counter: a loop that
    retries, or that sees a rejection, still has exactly one more result per
    exchange, and a counter that drifts replays a call for ever.
    """
    if not SCRIPTED_TOOL_CALLS:
        return None
    seen = seen_count(body)
    return SCRIPTED_TOOL_CALLS[seen] if seen < len(SCRIPTED_TOOL_CALLS) else None


def _openai_tool_call(name, arguments, call_id):
    return {
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": call_id,
                    "type": "function",
                    "function": {"name": name, "arguments": json.dumps(arguments)},
                }],
            },
            "finish_reason": "tool_calls",
        }],
        "usage": {"prompt_tokens": 50, "completion_tokens": 100, "total_tokens": 150},
    }


def openai_json_response(role, body):
    """Non-streaming OpenAI response. Tool-loop path (Phase 3.3): a request
    carrying `tools` gets one `tool_calls` turn until a `tool_result` shows up
    in the conversation, then a final text turn. Stateless."""
    text = json.dumps(body)
    tools = body.get("tools") or []
    usage = {"prompt_tokens": 50, "completion_tokens": 100, "total_tokens": 150}

    scripted = _next_scripted_call(body)
    if scripted and tools:
        return _openai_tool_call(
            scripted["name"],
            scripted.get("arguments") or {},
            f"call_script{seen_count(body)}",
        )

    if SCRIPTED_TOOL_LOOP and tools:
        # Drive the loop the way a real model drives it: look at the file
        # first, then submit.
        #
        # Without this, every Coder tool loop against this server burned its
        # entire 12-step budget calling a `bash` probe and never called
        # `submit_artifact` — so the path a modern model actually takes was
        # never exercised by anything in CI, and every run silently fell back
        # to the one-shot call. The fallback is the old behaviour and works,
        # which is precisely why the gap was invisible: the run still
        # produced a correct diff.
        if not _tool_result_seen(body):
            return _openai_tool_call("read_file", {"path": "src/stats.py"}, "call_read1")
        return _openai_tool_call(
            "submit_artifact", ROLE_RESPONSES[role], "call_submit1"
        )

    if tools and "tool_result" not in text:
        # Left exactly as it was, deliberately. The check above cannot match
        # OpenAI's `{"role": "tool"}`, so this branch always wins and the
        # server keeps answering with a probe call — which is what every
        # existing consumer was built and measured against. "Fix" it and the
        # demo, the e2e job and the TUI smokes all change behaviour at once,
        # on a change whose subject is a new opt-in path. The scripted path
        # above is the corrected version; this one is frozen.
        return _openai_tool_call(
            "bash", {"command": "echo tool-loop-probe"}, "call_probe1"
        )
    if tools:
        content = "Tool research complete: the probe command returned as observed."
    else:
        content = json.dumps(ROLE_RESPONSES[role], indent=2)
    return {
        "choices": [{
            "message": {"role": "assistant", "content": content},
            "finish_reason": "stop",
        }],
        "usage": usage,
    }


def anthropic_json_response(role, body):
    """Non-streaming Anthropic response with the same tool-loop behavior."""
    text = json.dumps(body)
    tools = body.get("tools") or []
    # `scripted` was never computed in this function. The OpenAI half of the
    # same feature had it, and the Rust test drove `/v1/chat/completions` — so
    # the Anthropic path raised `NameError` on the first scripted call and took
    # the whole server down with it, which the end-to-end leg saw as "connection
    # closed before message completed" and no reason at all.
    #
    # A feature scripted on one provider and run on the other has to be tested
    # on both, or it is not a feature and it is a trap.
    scripted = _next_scripted_call(body)
    if scripted and tools:
        return {
            "id": "msg_" + uuid.uuid4().hex[:24],
            "type": "message",
            "role": "assistant",
            "model": "mock-model",
            "content": [{
                "type": "tool_use",
                "id": f"toolu_script{seen_count(body)}",
                "name": scripted["name"],
                "input": scripted.get("arguments") or {},
            }],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 50, "output_tokens": 100},
        }

    if SCRIPTED_TOOL_LOOP and tools:
        # The Anthropic half of the scripted tool loop. Same two turns as the
        # OpenAI half, so a script behaves identically whichever provider the
        # end-to-end leg is pointed at — otherwise a scenario that passes on
        # one provider fails on the other for a reason that has nothing to do
        # with the scenario.
        if not _tool_result_seen(body):
            content = [{
                "type": "tool_use",
                "id": "toolu_read1",
                "name": "read_file",
                "input": {"path": "src/stats.py"},
            }]
            stop = "tool_use"
        else:
            content = [{
                "type": "tool_use",
                "id": "toolu_submit1",
                "name": "submit_artifact",
                "input": ROLE_RESPONSES[role],
            }]
            stop = "tool_use"
        return {
            "id": "msg_" + uuid.uuid4().hex[:24],
            "type": "message",
            "role": "assistant",
            "model": "mock-model",
            "content": content,
            "stop_reason": stop,
            "usage": {"input_tokens": 50, "output_tokens": 100},
        }
    if tools and "tool_result" not in text:
        content = [{
            "type": "tool_use",
            "id": "toolu_probe1",
            "name": "bash",
            "input": {"command": "echo tool-loop-probe"},
        }]
    elif tools:
        content = [{
            "type": "text",
            "text": "Tool research complete: the probe command returned as observed.",
        }]
    else:
        content = [{
            "type": "text",
            "text": json.dumps(ROLE_RESPONSES[role], indent=2),
        }]
    return {
        "id": "msg_" + uuid.uuid4().hex[:24],
        "type": "message",
        "role": "assistant",
        "model": "mock-model",
        "content": content,
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 50, "output_tokens": 100},
    }


def sse_openai_stream(role):
    """Generate OpenAI-format SSE stream events."""
    content = json.dumps(ROLE_RESPONSES[role], indent=2)
    model = "mock-model"
    fid = "chatcmpl-" + uuid.uuid4().hex[:24]
    created = int(datetime.now(timezone.utc).timestamp())

    # Stream content in chunks
    for chunk in chunk_text(content, size=25):
        event = {
            "id": fid,
            "object": "chat.completion.chunk",
            "created": created,
            "model": model,
            "choices": [{"index": 0, "delta": {"content": chunk}, "finish_reason": None}]
        }
        yield ("data: " + json.dumps(event) + "\n\n").encode()

    # Final chunk with finish_reason
    final = {
        "id": fid,
        "object": "chat.completion.chunk",
        "created": created,
        "model": model,
        "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]
    }
    yield ("data: " + json.dumps(final) + "\n\n").encode()

    # Usage chunk
    usage = {
        "id": fid,
        "object": "chat.completion.chunk",
        "created": created,
        "model": model,
        "choices": [],
        "usage": {"prompt_tokens": 50, "completion_tokens": 100, "total_tokens": 150}
    }
    yield ("data: " + json.dumps(usage) + "\n\n").encode()
    yield b"data: [DONE]\n\n"


def sse_anthropic_stream(role):
    """Generate Anthropic-format SSE stream events."""
    content = json.dumps(ROLE_RESPONSES[role], indent=2)
    msg_id = "msg_" + uuid.uuid4().hex[:24]
    model = "mock-model"

    # message_start
    start = {
        "type": "message_start",
        "message": {
            "id": msg_id,
            "type": "message",
            "role": "assistant",
            "model": model,
            "content": [],
            "stop_reason": None,
            "stop_sequence": None,
            "usage": {"input_tokens": 50, "output_tokens": 0}
        }
    }
    yield ("event: message_start\ndata: " + json.dumps(start) + "\n\n").encode()

    # content_block_start
    block_start = {
        "type": "content_block_start",
        "index": 0,
        "content_block": {"type": "text", "text": ""}
    }
    yield ("event: content_block_start\ndata: " + json.dumps(block_start) + "\n\n").encode()

    # content_block_delta chunks
    for chunk in chunk_text(content, size=25):
        delta = {
            "type": "content_block_delta",
            "index": 0,
            "delta": {"type": "text_delta", "text": chunk}
        }
        yield ("event: content_block_delta\ndata: " + json.dumps(delta) + "\n\n").encode()

    # content_block_stop
    block_stop = {"type": "content_block_stop", "index": 0}
    yield ("event: content_block_stop\ndata: " + json.dumps(block_stop) + "\n\n").encode()

    # message_delta
    msg_delta = {
        "type": "message_delta",
        "delta": {"stop_reason": "end_turn", "stop_sequence": None},
        "usage": {"output_tokens": 100}
    }
    yield ("event: message_delta\ndata: " + json.dumps(msg_delta) + "\n\n").encode()

    # message_stop
    yield ("event: message_stop\ndata: " + json.dumps({"type": "message_stop"}) + "\n\n").encode()


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        pass

    def _read_body(self):
        length = int(self.headers.get("Content-Length", 0))
        if length:
            return json.loads(self.rfile.read(length))
        return {}

    def _send_json(self, obj, status=200):
        data = json.dumps(obj).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def _stream_sse(self, event_generator):
        """Stream SSE events then close connection to signal completion."""
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Connection", "close")
        self.end_headers()
        try:
            for event_bytes in event_generator:
                self.wfile.write(event_bytes)
                self.wfile.flush()
                time.sleep(TOKEN_DELAY)
        except (BrokenPipeError, ConnectionResetError):
            pass
        # Connection closes when this method returns (Connection: close)

    def do_GET(self):
        if self.path == "/health":
            self._send_json({"status": "mock-llm-ready"})
        elif self.path.rstrip("/").endswith("/models"):
            # The model catalogue, in OpenAI's `{"data": [...]}` shape.
            #
            # This was missing, and it made a shipped feature untestable: the
            # endpoint every other handler fell through to answered a
            # 200 with `{"message": "Mock LLM server running"}`, which is not
            # a model list. So `niki providers models`, `niki recommend`'s
            # availability check, and the doctor all read as "no catalogue
            # here" against this server and were reported as `Unknown` —
            # indistinguishable from a provider with no `/models` endpoint.
            # Every assertion about the catalogue therefore had to be made
            # against a hand-built struct in a unit test, and the HTTP path
            # that a real user exercises was never run at all.
            #
            # The contents are chosen, not arbitrary. `claude-opus-4` is
            # absent on purpose: it is what the recommendation table reaches
            # for most roles, so this is the one that proves the "not offered,
            # here is what this account *can* run" path fires over a real
            # socket. The vendor-qualified `anthropic/...` spelling is what
            # OpenRouter uses, and the bare-id match has to survive it.
            self._send_json({"object": "list", "data": SCRIPTED_MODELS
                             if SCRIPTED_MODELS is not None else [
                {"id": "anthropic/claude-sonnet-4", "object": "model",
                 "pricing": {"prompt": "0.000003", "completion": "0.000015"}},
                {"id": "anthropic/claude-haiku-4-5", "object": "model",
                 "pricing": {"prompt": "0.000001", "completion": "0.000005"}},
                {"id": "openai/gpt-4o-mini", "object": "model",
                 "pricing": {"prompt": "0.00000015", "completion": "0.0000006"}},
                {"id": "openai/o3-mini", "object": "model",
                 "pricing": {"prompt": "0.0000011", "completion": "0.0000044"}},
                {"id": "mock-model", "object": "model",
                 "pricing": {"prompt": "0", "completion": "0"}},
            ]})
        else:
            self._send_json({"message": "Mock LLM server running"})

    def do_POST(self):
        body = self._read_body()
        role = detect_role(body)
        stream = body.get("stream", False)

        if "/v1/messages" in self.path:
            if stream:
                self._stream_sse(sse_anthropic_stream(role))
            else:
                self._send_json(anthropic_json_response(role, body))
        elif "/v1/chat/completions" in self.path or "/chat/completions" in self.path:
            if stream:
                self._stream_sse(sse_openai_stream(role))
            else:
                self._send_json(openai_json_response(role, body))
        else:
            # Default: treat as openai
            if stream:
                self._stream_sse(sse_openai_stream(role))
            else:
                self._send_json(openai_json_response(role, body))


class ThreadedTCPServer(socketserver.ThreadingMixIn, socketserver.TCPServer):
    allow_reuse_address = True
    daemon_threads = True


def start_server():
    with ThreadedTCPServer(("0.0.0.0", PORT), Handler) as httpd:
        sys.stderr.write("Mock LLM server listening on port {}\n".format(PORT))
        sys.stderr.flush()
        httpd.serve_forever()


if __name__ == "__main__":
    start_server()
