#!/usr/bin/env python3
"""Connection-held advisory landing turn (build-infrastructure.md, 8)."""
import fcntl
import json
import math
import os
from pathlib import Path
import re
import secrets
import select
import signal
import socket
import stat
import subprocess
import sys
import time

os.umask(0o077)


def identity(token, repo, holder):
    if not re.fullmatch('[0-9a-f]{32}', token) or any(
            not re.fullmatch('[A-Za-z0-9][A-Za-z0-9_.-]{0,63}', value)
            for value in (repo, holder)):
        raise ValueError('invalid-identity')
    return dict(token=token, repository=repo, holder=holder)


def private_file(path, flags):
    fd = os.open(path, flags | os.O_CREAT | os.O_NOFOLLOW | os.O_NONBLOCK, 0o600)
    if not stat.S_ISREG(os.fstat(fd).st_mode):
        os.close(fd)
        raise ValueError('invalid-file')
    os.fchmod(fd, 0o600)
    return fd


def event(tmp, who, name, start, granted=None, reason=None):
    record = dict(who, timestamp=time.time(), event=name,
                  wait_s=round((granted or time.monotonic()) - start, 6))
    if granted is not None:
        record['runtime_s'] = round(time.monotonic() - granted, 6)
    if reason:
        record['reason'] = reason
    line = json.dumps(record, sort_keys=True) + '\n'
    print('land-turn: ' + line.rstrip(), file=sys.stderr, flush=True)
    try:
        directory = tmp / 'landing-turn'
        directory.mkdir(mode=0o700, parents=True, exist_ok=True)
        with os.fdopen(private_file(directory / 'events.jsonl', os.O_WRONLY | os.O_APPEND), 'w') as out:
            out.write(line)
            out.flush()
    except (OSError, ValueError):
        print('land-turn: private journal unavailable', file=sys.stderr, flush=True)


def disconnected(delay):
    return bool(select.select([sys.stdin], [], [], delay)[0])


def holder(directory, who, tmp):
    directory.mkdir(mode=0o700, parents=True, exist_ok=True)
    # The lock file is retained: unlinking it could split the flock identity.
    with os.fdopen(private_file(directory / (who['repository'] + '.lock'), os.O_RDWR), 'r+') as lock:
        start = time.monotonic()
        while not disconnected(0):
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except BlockingIOError:
                if disconnected(.05):
                    return
        else:
            return
        current = directory / (who['repository'] + '.current.json')
        granted = time.monotonic()
        try:
            grant = dict(who, wait_s=round(time.monotonic() - start, 6))
            with os.fdopen(private_file(current, os.O_WRONLY | os.O_TRUNC), 'w') as out:
                json.dump(grant, out)
            print(json.dumps(grant), flush=True)
            # Any connection input ends this holder; EOF needs no heartbeat.
            disconnected(None)
        finally:
            current.unlink(missing_ok=True)
            event(tmp, who, 'release', start, granted)


def close_transport(transport):
    if transport is None:
        return
    transport.stdin.close()
    transport.stdout.close()
    try:
        transport.wait(timeout=1)
    except subprocess.TimeoutExpired:
        transport.kill()
        transport.wait()


def client(tmp, repo, command):
    hostname = re.sub('[^A-Za-z0-9_.-]', '-', socket.gethostname()).lstrip('._-')[:64] or 'local'
    who = identity(secrets.token_hex(16), repo, hostname)
    start, granted, transport, child = time.monotonic(), None, None, None
    failed = False

    def fail(reason):
        nonlocal failed
        if not failed:
            failed = True
            event(tmp, who, 'fail-open', start, granted, reason)

    def interrupted(signum, frame):
        if child is not None and child.poll() is None:
            child.send_signal(signum)
        raise SystemExit(128 + signum)

    for signum in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP):
        signal.signal(signum, interrupted)
    event(tmp, who, 'request', start)
    try:
        fixture = os.environ.get('LAND_TURN_FIXTURE_DIR')
        try:
            if fixture is None:
                fail('transport-unapproved')
            elif (os.environ.get('HEAVY_RECOVERY_FIXTURE') != '1' or
                  not Path(fixture).resolve().is_relative_to(tmp.resolve()) or
                  Path(fixture).resolve() == tmp.resolve()):
                fail('invalid-fixture')
            else:
                transport = subprocess.Popen(
                    [sys.executable, '-I', '-B', str(Path(__file__).resolve()), 'holder', fixture,
                     who['token'], who['repository'], who['holder'], str(tmp)],
                    stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                    close_fds=True)
                pending = bytearray()
                while granted is None and not failed:
                    chunk = os.read(transport.stdout.fileno(), 513)
                    if not chunk:
                        fail('transport-eof')
                        break
                    pending.extend(chunk)
                    if len(pending) > 512:
                        fail('protocol-invalid')
                    elif b'\n' in pending:
                        grant = json.loads(pending)
                        wait = grant.pop('wait_s')
                        if (grant != who or type(wait) not in (int, float) or
                                not math.isfinite(wait) or wait < 0):
                            raise ValueError('protocol-invalid')
                        granted = time.monotonic()
                        event(tmp, who, 'grant', start, granted)
        except (OSError, ValueError, KeyError, TypeError, AttributeError, OverflowError):
            fail('transport-or-protocol-failure')
        if failed:
            close_transport(transport)
            transport = None
        environment = dict(os.environ, LAND_TURN_ACTIVE='1')
        # No transport descriptor reaches land, heavy or the after-landing tier.
        child = subprocess.Popen(command, env=environment, stdin=subprocess.DEVNULL, close_fds=True)
        while child.poll() is None:
            if transport is not None and select.select([transport.stdout], [], [], .05)[0]:
                chunk = os.read(transport.stdout.fileno(), 513)
                fail('transport-eof' if not chunk else 'protocol-invalid')
                close_transport(transport)
                transport = None
            else:
                time.sleep(.05)
        return child.returncode if child.returncode >= 0 else 128 - child.returncode
    finally:
        close_transport(transport)


def main():
    if sys.argv[1] == 'holder' and len(sys.argv) == 7:
        holder(Path(sys.argv[2]), identity(*sys.argv[3:6]), Path(sys.argv[6]))
        return 0
    if sys.argv[1] == 'client' and len(sys.argv) >= 5:
        return client(Path(sys.argv[2]), sys.argv[3], sys.argv[4:])
    raise ValueError('invalid-invocation')


if __name__ == '__main__':
    try:
        sys.exit(main())
    except (OSError, ValueError, IndexError):
        print('land-turn: invalid invocation or unavailable local state', file=sys.stderr, flush=True)
        sys.exit(2)
