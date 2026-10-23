"""Stdio MCP calls admitted by the runner and executed in private views (lab.md, 6.3)."""
from collections import deque
import json
import os
import re
from pathlib import Path
import selectors
import shutil
import signal
import socket
import subprocess
import sys
from . import store, views
from .records import Refused


def _wire(value):
    return json.dumps(value, ensure_ascii=False, separators=(',', ':')).encode() + b'\n'


def _kill(child):
    if child['process'].poll() is None:
        try:
            signal.pidfd_send_signal(child['pidfd'], signal.SIGKILL)
        except ProcessLookupError:
            pass
    child['process'].wait()


def _spawn(record, call, number, directory, bwrap, roots):
    paths = {key: directory / f'{number}.{key}' for key in ('stdin', 'stdout', 'stderr')}
    streams = {}
    try:
        for name, path in paths.items():
            fd = os.open(path, os.O_RDWR | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
            streams[name] = os.fdopen(fd, 'w+b')
        streams['stdin'].write((call['stdin'] or '').encode())
        streams['stdin'].seek(0)
        tool = next(t for t in record['tools'] if t['name'] == call['tool'])
        argv = views.tool_argv(bwrap, tool, record['work'], call['args'], roots)
        process = subprocess.Popen(argv, env={}, stdin=streams['stdin'], stdout=streams['stdout'],
                                   stderr=streams['stderr'], cwd=record['work'])
        try:
            pidfd = os.pidfd_open(process.pid)
        except BaseException:
            process.kill()
            process.wait()
            raise
        return dict(process=process, pidfd=pidfd, call=call, n=number, paths=paths)
    finally:
        for stream in streams.values():
            stream.close()


def serve(record, directory, *, incoming=None, outgoing=None):
    incoming = sys.stdin.buffer if incoming is None else incoming
    outgoing = sys.stdout.buffer if outgoing is None else outgoing
    bwrap = shutil.which('bwrap', path=os.defpath)
    if bwrap is None:
        raise RuntimeError('bubblewrap is required')
    roots = [path for path in views.SYSTEM_ROOTS if Path(path).exists()]
    directory = Path(directory)
    directory.mkdir(mode=0o700, exist_ok=True)
    pending, running, used = deque(), {}, set()
    buffers = {'mcp': b'', 'broker': b''}
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as broker, selectors.DefaultSelector() as selector:
        # sockaddr_un is bounded; the manifest retains the full absolute path.
        parent = store.checked_directory(Path(record['socket']).parent)
        try:
            broker.connect(f'/proc/self/fd/{parent}/' + Path(record['socket']).name)
        finally:
            os.close(parent)
        selector.register(incoming, selectors.EVENT_READ, 'mcp')
        selector.register(broker, selectors.EVENT_READ, 'broker')
        outbound = {'output': bytearray(), 'broker': bytearray()}
        after_broker = []
        targets = {'output': outgoing.fileno(), 'broker': broker.fileno()}
        blocking = {fd: os.get_blocking(fd) for fd in targets.values()}
        for fd in targets.values():
            os.set_blocking(fd, False)

        def enqueue(kind, value):
            outbound[kind].extend(_wire(value))
            events = selectors.EVENT_WRITE | (selectors.EVENT_READ if kind == 'broker' else 0)
            try:
                selector.modify(targets[kind], events, kind)
            except KeyError:
                selector.register(targets[kind], events, kind)

        try:
            while True:
                for key, events in selector.select():
                    kind = key.data
                    if kind in ('output', 'broker') and events & selectors.EVENT_WRITE:
                        try:
                            count = os.write(key.fd, outbound[kind])
                        except BlockingIOError:
                            count = 0
                        del outbound[kind][:count]
                        if not outbound[kind]:
                            if kind == 'broker':
                                selector.modify(broker, selectors.EVENT_READ, 'broker')
                                for response in after_broker:
                                    enqueue('output', response)
                                after_broker.clear()
                            else:
                                selector.unregister(key.fd)
                    if kind == 'output':
                        continue
                    if type(kind) is int:
                        child = running.pop(kind)
                        selector.unregister(child['pidfd'])
                        status = child['process'].wait()
                        os.close(child['pidfd'])
                        result = dict(status=status if status >= 0 else None,
                                      signal=signal.Signals(-status).name if status < 0 else None)
                        enqueue('broker', dict(end=kind, **result))
                        result.update({name: child['paths'][name].read_bytes().decode('utf-8', 'replace')
                                       for name in ('stdout', 'stderr')})
                        after_broker.append(views.call_result(child['call']['id'], result,
                                                              failed=status != 0))
                        continue
                    if not events & selectors.EVENT_READ:
                        continue
                    data = os.read(key.fd, 65536)
                    if not data:
                        return 0
                    buffers[kind] += data
                    while b'\n' in buffers[kind]:
                        line, buffers[kind] = buffers[kind].split(b'\n', 1)
                        if kind == 'mcp':
                            response, call = views.mcp_request(line, record['tools'])
                            if response is not None:
                                enqueue('output', response)
                            if call is not None:
                                pending.append(call)
                                enqueue('broker', views.broker_start(call))
                        else:
                            try:
                                reply = views.broker_reply(line)
                            except ValueError as error:
                                raise RuntimeError(str(error)) from error
                            if not pending:
                                raise RuntimeError('unsolicited broker admission')
                            call = pending.popleft()
                            if 'refuse' in reply:
                                enqueue('output', views.call_result(call['id'], dict(error=reply['refuse']), failed=True))
                            else:
                                number = reply['go']
                                if number in used:
                                    raise RuntimeError('broker reused an action number')
                                used.add(number)
                                child = _spawn(record, call, number, directory, bwrap, roots)
                                running[number] = child
                                selector.register(child['pidfd'], selectors.EVENT_READ, number)
        finally:
            for child in running.values():
                _kill(child)
                os.close(child['pidfd'])
            for fd, was_blocking in blocking.items():
                os.set_blocking(fd, was_blocking)


def main(root, run):
    if type(run) is not str or not re.fullmatch(r'[A-Za-z0-9_-]+', run):
        raise Refused('run', 'expected a run id')
    # The runner owns the writer lock. This reader neither sweeps nor reconciles it.
    with store.open_lab(root) as lab:
        found = list(lab.root.glob('keys/*/*/runs/' + run + '/driver/tools.json'))
        if len(found) != 1:
            raise Refused('run', 'expected exactly one driver/tools.json')
        path = found[0].relative_to(lab.root).as_posix()
        try:
            record = views.tools_record(lab.read(path), run)
        except ValueError as error:
            raise Refused('driver/tools.json', str(error)) from error
        return serve(record, found[0].parent / 'calls')
