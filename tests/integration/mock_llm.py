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
    }
}


def detect_role(body):
    """Detect agent role from system prompt content."""
    text = json.dumps(body).lower()
    if "code review agent" in text:
        return "reviewer"
    if "testing agent" in text:
        return "tester"
    if "implementation agent" in text:
        return "coder"
    if "planning agent" in text:
        return "planner"
    return "planner"


def chunk_text(text, size=20):
    """Split text into chunks to simulate streaming tokens."""
    return [text[i:i+size] for i in range(0, len(text), size)]


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
        """Stream SSE events with small delays to simulate real-time."""
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Connection", "keep-alive")
        self.end_headers()
        try:
            for event_bytes in event_generator:
                self.wfile.write(event_bytes)
                self.wfile.flush()
                time.sleep(TOKEN_DELAY)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def do_GET(self):
        if self.path == "/health":
            self._send_json({"status": "mock-llm-ready"})
        else:
            self._send_json({"message": "Mock LLM server running"})

    def do_POST(self):
        body = self._read_body()
        role = detect_role(body)

        if "/v1/messages" in self.path:
            self._stream_sse(sse_anthropic_stream(role))
        elif "/v1/chat/completions" in self.path or "/chat/completions" in self.path:
            self._stream_sse(sse_openai_stream(role))
        else:
            # Default: treat as openai
            self._stream_sse(sse_openai_stream(role))


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
