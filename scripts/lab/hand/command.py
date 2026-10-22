"""The hand's one-call connection and streams (lab-browser.md, 8.2, 8.8)."""
import socket
import sys
import time
from look import canon
from runner import canonical
from runner.records import Refused
from . import words

LIMIT = 16 * 1024 * 1024


def exchange(endpoint, call):
    deadline = time.monotonic() + (call['seconds'] + 30 if call['call'] == 'wait' else 60)
    family = socket.AF_INET6 if endpoint[0] == '::1' else socket.AF_INET
    with socket.socket(family, socket.SOCK_STREAM) as connection:
        connection.settimeout(max(0.001, deadline - time.monotonic()))
        connection.connect(endpoint)
        connection.settimeout(max(0.001, deadline - time.monotonic()))
        connection.sendall(canonical.dumps(call) + b'\n')
        data = bytearray()
        while len(data) <= LIMIT:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise words.Unreadable('deadline passed')
            connection.settimeout(remaining)
            chunk = connection.recv(min(65536, LIMIT + 1 - len(data)))
            if not chunk:
                raise words.Unreadable('connection closed')
            data.extend(chunk)
            end = data.find(b'\n')
            if end >= 0:
                if end + 1 > LIMIT:
                    break
                return canon.loads(bytes(data[:end]), None)
        raise words.Unreadable('answer too long')


def main(argv=None, stdin=None, stdout=None, stderr=None, *, sign_in=False):
    argv = list(sys.argv[1:] if argv is None else argv)
    stdin = sys.stdin if stdin is None else stdin
    stdout = sys.stdout if stdout is None else stdout
    stderr = sys.stderr if stderr is None else stderr
    source = getattr(stdin, 'buffer', stdin)
    try:
        if not argv:
            raise words.Usage('missing ADDRESS')
        endpoint = words.address(argv.pop(0))
        if sign_in:
            if argv:
                raise words.Usage('sign-in takes only ADDRESS')
            link = source.readline()
            if isinstance(link, bytes):
                link = link.decode('utf-8', 'strict')
            link.encode('utf-8', 'strict')
            if link.endswith('\n'):
                link = link[:-1]
            if not link or '\n' in link or source.read(1):
                raise words.Usage('sign-in needs exactly one nonempty input line')
            call = dict(call='sign-in', link=link)
        else:
            call = words.call(argv)
            if call['call'] == 'type':
                value = source.read()
                if isinstance(value, bytes):
                    value = value.decode('utf-8', 'strict')
                value.encode('utf-8', 'strict')
                call['text'] = value[:-1] if value.endswith('\n') else value
    except (words.Usage, UnicodeError, ValueError) as error:
        # No argument or input text belongs in a sign-in diagnostic.
        stderr.write('usage: sign-in ADDRESS; one nonempty link line on stdin\n' if sign_in else
                     'usage: browse ADDRESS VERB [OPERAND...] [--in ROLE NAME] [--exact]\n' + str(error) + '\n')
        return 2
    try:
        status, lines = words.text(call, exchange(endpoint, call))
    except (OSError, words.Unreadable, Refused, UnicodeError, ValueError, RecursionError):
        # A fixed line: an error's text could carry a sign-in's link.
        stderr.write('the browser is unreachable, or its answer unreadable\n')
        return 3
    for line in lines:
        stdout.write(line + '\n')
    return status
