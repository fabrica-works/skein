"""One-shot Codex account read, with no thread or turn and no account file reads."""
import argparse
import json
import os
import selectors
import subprocess
import tempfile
import time
from . import codex
from .brake import CAP, kill_group


class Host:
    def __init__(self, process, poller, deadline):
        self.process, self.poller, self.deadline = process, poller, deadline
        self.buffer = b''
        self.lines = []
        for stream in (process.stdout, process.stderr):
            os.set_blocking(stream.fileno(), False)
            poller.register(stream, selectors.EVENT_READ)

    def send(self, value):
        self.process.stdin.write((json.dumps(value, separators=(',', ':')) + '\n').encode())
        self.process.stdin.flush()

    def receive(self, identity):
        while True:
            while self.lines:
                response = json.loads(self.lines.pop(0))
                if type(response) is not dict:
                    raise ValueError('invalid host response')
                if response.get('id') == identity:
                    return response
            if self.process.stdout.closed:
                raise RuntimeError('host exited')
            self.read()

    def read(self):
        remaining = self.deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError('host timed out')
        events = self.poller.select(remaining)
        if not events:
            raise TimeoutError('host timed out')
        for key, _events in events:
            data = os.read(key.fd, CAP)
            if not data:
                self.poller.unregister(key.fileobj)
                key.fileobj.close()
            elif key.fileobj is self.process.stdout:
                self.buffer += data
                if len(self.buffer) > CAP:
                    raise ValueError('host response too large')
                while b'\n' in self.buffer:
                    line, self.buffer = self.buffer.split(b'\n', 1)
                    self.lines.append(line)


def read_account(path, home, five_hour, week, timeout):
    """Run initialize, initialized, account/rateLimits/read, then close and await exit."""
    with tempfile.TemporaryDirectory(prefix='lab-brake-') as empty_home:
        env = dict(CODEX_HOME=home, HOME=empty_home, PATH='/usr/bin:/bin', LANG='C.UTF-8')
        with subprocess.Popen([path, 'app-server', '--stdio', '--strict-config'], env=env,
                              cwd=empty_home, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE, start_new_session=True) as process:
            try:
                with selectors.DefaultSelector() as poller:
                    host = Host(process, poller, time.monotonic() + timeout)
                    host.send(dict(id=1, method='initialize', params=dict(
                        clientInfo=dict(name='lab-brake', version='1'), capabilities=dict(experimentalApi=True))))
                    initialized = host.receive(1)
                    if 'error' in initialized:
                        return codex.brake_fold(initialized, five_hour, week)
                    host.send(dict(method='initialized'))
                    host.send(dict(id=2, method='account/rateLimits/read', params={}))
                    response = host.receive(2)
                    process.stdin.close()
                    while poller.get_map():
                        host.read()
                    try:
                        status = process.wait(timeout=max(0, host.deadline - time.monotonic()))
                    except subprocess.TimeoutExpired as error:
                        raise TimeoutError('host timed out') from error
                    if status != 0:
                        return 1, 'refused: host exit ' + str(status)
                    return codex.brake_fold(response, five_hour, week)
            finally:
                kill_group(process)
                process.wait()


def percent(value):
    number = int(value)
    if not 0 <= number <= 100:
        raise argparse.ArgumentTypeError('expected percent 0..100')
    return number


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--codex', required=True)
    parser.add_argument('--home', required=True)
    parser.add_argument('--five-hour', type=percent, required=True)
    parser.add_argument('--week', type=percent, required=True)
    parser.add_argument('--timeout', type=int, default=30)
    args = parser.parse_args(argv)
    if not os.path.isabs(args.codex) or not os.path.isabs(args.home) or args.timeout <= 0:
        parser.error('absolute codex/home paths and a positive timeout are required')
    previous = os.umask(0o077)
    try:
        try:
            status, line = read_account(args.codex, args.home, args.five_hour, args.week, args.timeout)
        except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
            status, line = 1, 'refused: ' + str(error)
        print(line)
        return status
    finally:
        os.umask(previous)
