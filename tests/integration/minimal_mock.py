#!/usr/bin/env python3
"""Minimal raw-socket mock LLM server for debugging streaming issues."""
import json
import socket
import time
import uuid


def handle(conn):
    data = b''
    while b'\r\n\r\n' not in data:
        chunk = conn.recv(4096)
        if not chunk:
            break
        data += chunk

    body_start = data.index(b'\r\n\r\n') + 4
    body = data[body_start:]

    for line in data.split(b'\r\n'):
        if line.lower().startswith(b'content-length:'):
            cl = int(line.split(b':')[1].strip())
            while len(body) < cl:
                chunk = conn.recv(4096)
                if not chunk:
                    break
                body += chunk
            break

    content = json.dumps({
        "summary": "Add health check endpoint",
        "approach": "Create GET /health route",
        "files_to_modify": [{"path": "server.js", "action": "create", "description": "HTTP server"}],
        "acceptance_criteria": ["GET /health returns 200"],
        "constraints": ["Use Node.js built-in modules"],
        "estimated_complexity": "low"
    })

    msg_id = 'msg_' + uuid.uuid4().hex[:24]
    events = []
    events.append(('message_start', {
        'type': 'message_start',
        'message': {'id': msg_id, 'type': 'message', 'role': 'assistant', 'model': 'mock', 'content': [], 'stop_reason': None, 'stop_sequence': None, 'usage': {'input_tokens': 50, 'output_tokens': 0}}
    }))
    events.append(('content_block_start', {'type': 'content_block_start', 'index': 0, 'content_block': {'type': 'text', 'text': ''}}))

    for i in range(0, len(content), 30):
        events.append(('content_block_delta', {'type': 'content_block_delta', 'index': 0, 'delta': {'type': 'text_delta', 'text': content[i:i+30]}}))

    events.append(('content_block_stop', {'type': 'content_block_stop', 'index': 0}))
    events.append(('message_delta', {'type': 'message_delta', 'delta': {'stop_reason': 'end_turn', 'stop_sequence': None}, 'usage': {'output_tokens': 100}}))
    events.append(('message_stop', {'type': 'message_stop'}))

    sse_body = ''
    for event_type, event_data in events:
        sse_body += f'event: {event_type}\n'
        sse_body += f'data: {json.dumps(event_data)}\n\n'

    resp = (
        'HTTP/1.1 200 OK\r\n'
        'Content-Type: text/event-stream\r\n'
        'Cache-Control: no-cache\r\n'
        'Connection: keep-alive\r\n'
        f'Content-Length: {len(sse_body)}\r\n'
        '\r\n'
        f'{sse_body}'
    )
    conn.sendall(resp.encode())
    time.sleep(0.5)
    conn.close()


server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
server.bind(('0.0.0.0', 8080))
server.listen(5)
print('listening on 8080', flush=True)

conn, addr = server.accept()
handle(conn)
server.close()
