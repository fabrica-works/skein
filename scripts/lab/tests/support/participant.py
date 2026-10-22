"""A port-zero service and small commands used only by native process stories."""
import json
import os
from pathlib import Path
import select
import signal
import socket
import stat
import sys


def descendants(double_fork):
    reader, writer = os.pipe()
    child = os.fork()
    if child == 0:
        os.close(reader)
        os.setsid()
        grandchild = os.fork()
        if grandchild == 0:
            os.write(writer, str(os.getpid()).encode('ascii'))
            os.close(writer)
            while True:
                signal.pause()
        os.close(writer)
        if double_fork:
            os._exit(0)
        while True:
            signal.pause()
    os.close(writer)
    grandchild = int(os.read(reader, 100))
    os.close(reader)
    if double_fork:
        os.waitpid(child, 0)
    return [grandchild] if double_fork else [child, grandchild]


def server(args):
    notify, detach, run_id = args
    owned = descendants(detach == 'doublefork') if detach in ('setsid', 'doublefork') else []
    signal.signal(signal.SIGTERM, lambda _sig, _frame: sys.exit(0))
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', 0))
        listener.listen()
        address = '%s:%d' % listener.getsockname()
        if notify != '-':
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
                connection.connect(notify)
                connection.sendall(json.dumps(dict(main=os.getppid(), first=os.getpid(), descendants=owned, run=run_id)).encode('ascii') + b'\n')
                if connection.recv(1) != b'G':
                    return 19
        print('pid ' + str(os.getpid()), flush=True)
        print('ready ' + address, flush=True)
        while True:
            connection, _ = listener.accept()
            with connection:
                command = connection.recv(1024)
                if command == b'exit':
                    return 17
                if command == b'fresh':
                    print('fresh invented-word', flush=True)
                connection.sendall(b'private-canary')


def main():
    mode, *args = sys.argv[1:]
    if mode == 'server':
        return server(args)
    if mode == 'dependent':
        address = Path(args[0]).read_text()
        host, port = address.split(':')
        with socket.create_connection((host, int(port))) as connection:
            connection.sendall(b'ping')
            if connection.recv(1024) != b'private-canary':
                return 20
        signal.signal(signal.SIGTERM, lambda _sig, _frame: sys.exit(0))
        print('ready dependent', flush=True)
        while True:
            signal.pause()
    if mode == 'request-fresh':
        host, port = args[0].split(':')
        with socket.create_connection((host, int(port))) as connection:
            connection.sendall(b'fresh')
            return 0 if connection.recv(1024) == b'private-canary' else 20
    if mode == 'capture-stdin':
        return 0 if sys.stdin.read() == 'invented-word\n' else 21
    if mode == 'tree-reader':
        root = Path(args[0])
        for path, expected in ((root, 0o500), (root / 'nested', 0o500),
                               (root / 'nested/index', 0o400), (root / 'run', 0o500)):
            if stat.S_IMODE(path.stat().st_mode) != expected:
                return 22
        if (root / 'nested/index').read_bytes() != b'{unrendered}\n':
            return 23
        signal.signal(signal.SIGTERM, lambda _sig, _frame: sys.exit(0))
        print('ready read-only-tree', flush=True)
        while True:
            signal.pause()
    if mode == 'exit-service':
        address, pid = args
        pidfd = os.pidfd_open(int(pid))
        try:
            host, port = address.split(':')
            with socket.create_connection((host, int(port))) as connection:
                connection.sendall(b'exit')
                while connection.recv(1024):
                    pass
            select.select([pidfd], [], [])
        finally:
            os.close(pidfd)
        return 0
    if mode == 'touch':
        Path(args[0]).write_text('action ran')
        return 0
    if mode == 'collect':
        destination = Path(args[0])
        (destination / 'observation.json').write_text('{"private-canary":true}\n')
        (destination / 'log').write_text('collected private canary\n')
        return 0
    if mode == 'check':
        kind, collected, seed = args
        if not Path(collected, 'observation.json').is_file():
            print(json.dumps({'undecided': 'observation absent'}))
            return 2
        if kind == 'malformed':
            print('this is not a check record')
            return 0
        if kind == 'undecided':
            print(json.dumps({'undecided': 'private story cannot judge'}))
            return 2
        if kind == 'fail' or kind == 'parity' and int(seed) % 2:
            print(json.dumps({'fail': 'state/private-canary', 'summary': 'private failure'}))
            return 1
        return 0
    if mode == 'panic':
        print("thread 'main' panicked at standin.rs:7:2:", file=sys.stderr, flush=True)
        print('private planted panic', file=sys.stderr, flush=True)
        return 101
    raise ValueError('unknown private stand-in mode')


if __name__ == '__main__':
    sys.exit(main())
