"""One contained scripted endpoint, sharing the triager's native poll loop."""
import os
from pathlib import Path
import select
import signal
import sys
import time
from . import canonical, transport


class Endpoint:
    def __init__(self, lab, base, host, value, seconds):
        self.lab, self.base, self.host, self.seconds = lab, base, host, seconds
        self.address, self.pending = None, b''
        self.ended = False
        self.logs = {}
        lab.write_once(base + '/script.json', canonical.record_bytes(value))
        root = lab.root / base
        argv = [os.path.realpath(sys.executable), '-I', '-B', '-S', str(Path(__file__).with_name('launch.py')),
                str(lab.root), 'endpoint', '-', str(root / 'script.json'), str(root)]
        for name in ('stdout', 'stderr'):
            self.logs[name] = lab.create_file(base + '/' + name)
        stdin = os.open(os.devnull, os.O_RDONLY | os.O_CLOEXEC)
        try:
            self.child = host.spawn('scripted-endpoint', argv, dict(PATH=os.defpath, LANG='C.UTF-8'), stdin, cwd=str(root))
        finally:
            os.close(stdin)

    def poll(self, *, stopping=False):
        child = self.child
        if 'scripted-endpoint' not in self.host.children:
            return
        if child['exec_ack'] is not None and select.select([child['exec_ack']], [], [], 0)[0]:
            data = os.read(child['exec_ack'], 65536)
            if data:
                raise RuntimeError('scripted endpoint spawn: ' + data.decode('utf-8', 'replace'))
            os.close(child['exec_ack'])
            child['exec_ack'] = None
        for name, fd in list(child['streams'].items()):
            try:
                data = os.read(fd, 65536)
            except BlockingIOError:
                continue
            if not data:
                os.close(fd)
                del child['streams'][name]
                continue
            view = memoryview(data)
            while view:
                size = os.write(self.logs[name], view)
                if size == 0:
                    raise OSError('scripted endpoint log made no progress')
                view = view[size:]
            if name == 'stdout':
                self.pending += data
                while b'\n' in self.pending:
                    line, self.pending = self.pending.split(b'\n', 1)
                    if self.address is not None or not line.startswith(b'listening 127.0.0.1:'):
                        raise RuntimeError('invalid scripted endpoint readiness')
                    self.address = line.decode().removeprefix('listening ')
        if not child['reaped'] and self.host.exited('scripted-endpoint'):
            self.host.reap('scripted-endpoint')
            self.host.kill_tree('scripted-endpoint')
            self.ended = True
        if child['reaped'] and not child['streams'] and not self.host.counts('scripted-endpoint')['populated']:
            self.host.remove('scripted-endpoint')
        if self.ended and not stopping:
            raise RuntimeError('scripted endpoint ended before triage')

    def ready(self, wake):
        deadline = time.monotonic() + self.seconds
        while self.address is None:
            self.poll()
            left = deadline - time.monotonic()
            if left <= 0:
                raise RuntimeError('scripted endpoint exceeded command_seconds')
            if self.address is None:
                transport.driver_poll(self.host, int(left * 1000), wake, [])
                self.wake(wake)
        return self.address

    @staticmethod
    def wake(fd):
        try:
            os.read(fd, 4096)
        except BlockingIOError:
            pass

    def stop(self, wake, seconds):
        self.host.signal_first('scripted-endpoint', signal.SIGTERM)
        deadline, killed = time.monotonic() + seconds, False
        while 'scripted-endpoint' in self.host.children:
            self.poll(stopping=True)
            if 'scripted-endpoint' not in self.host.children:
                break
            left = deadline - time.monotonic()
            if left <= 0:
                if killed:
                    raise RuntimeError('scripted endpoint did not drain within stop_seconds')
                self.host.kill_tree('scripted-endpoint')
                deadline, killed = time.monotonic() + seconds, True
                continue
            transport.driver_poll(self.host, int(left * 1000), wake, [])
            self.wake(wake)

    def close(self):
        for fd in self.logs.values():
            os.close(fd)
        self.logs.clear()
