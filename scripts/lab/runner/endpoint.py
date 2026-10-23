"""Loopback Responses endpoint; script decisions live in runner.script."""
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
from pathlib import Path
import signal
from . import script, store


USAGE = dict(input_tokens=100, input_tokens_details=dict(cached_tokens=0), output_tokens=10,
             output_tokens_details=dict(reasoning_tokens=0), total_tokens=110)


def main(script_path, out_dir):
    os.umask(0o077)
    value = script.parse(store.read_regular(script_path)[0])
    output = Path(out_dir)
    for name in ('requests', 'responses'):
        (output / name).mkdir(mode=0o700, parents=True, exist_ok=True)
    turn, captured, serial = 0, {}, 0

    def write(path, data):
        fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        with os.fdopen(fd, 'wb') as stream:
            stream.write(data)

    class Handler(BaseHTTPRequestHandler):
        def handle_request(self):
            nonlocal turn, captured, serial
            number = serial
            serial += 1
            body = self.rfile.read(int(self.headers.get('Content-Length', '0')))
            write(output / 'requests' / f'{number}.json', body)
            valid = self.command == 'POST' and self.path == '/v1/responses'
            write(output / 'requests' / f'{number}.http.json',
                  (json.dumps(dict(method=self.command, path=self.path, status=200 if valid else 404)) + '\n').encode())
            if not valid:
                write(output / 'responses' / f'{number}.sse', b'')
                self.send_error(404)
                return
            items, captured = script.respond(value, turn, json.loads(body), captured)
            data = script.stream(items, 'resp_' + str(turn), USAGE)
            turn += 1
            write(output / 'responses' / f'{number}.sse', data)
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def __getattr__(self, name):
            if name.startswith('do_'):
                return self.handle_request
            raise AttributeError(name)

        def log_message(self, *_args):
            pass

    def terminate(_signal, _frame):
        raise SystemExit(0)

    signal.signal(signal.SIGTERM, terminate)
    with HTTPServer(('127.0.0.1', 0), Handler) as server:
        print('listening 127.0.0.1:' + str(server.server_address[1]), flush=True)
        server.serve_forever()
    return 0
