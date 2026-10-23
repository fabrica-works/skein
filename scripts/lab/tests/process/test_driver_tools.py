"""Real views, stdio MCP and an inert Codex sandbox; no model or credentials."""
import json
import os
from pathlib import Path
import select
import shutil
import socket
import stat
import subprocess
import sys
import tempfile
import unittest
from runner import canonical, store, views
from runner.probe import BoundaryError, boundary_probe

PACKAGE = Path(__file__).resolve().parents[2]
PYTHON = os.path.realpath(sys.executable)
SUPPORT = PACKAGE / 'tests/support/tool_client.py'
CAP = 3


def receive_line(stream):
    data = b''
    while not data.endswith(b'\n'):
        ready, _, _ = select.select([stream], [], [], CAP)
        if not ready:
            raise AssertionError('wire did not progress')
        part = os.read(stream.fileno(), 1)
        if not part:
            raise AssertionError('unexpected EOF: ' + repr(data))
        data += part
    return json.loads(data)


class ToolProcess(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='tools-', dir=PACKAGE.parents[1])
        self.root = Path(self.temp.name)
        self.addCleanup(self.temp.cleanup)
        self.work = self.root / 'work'
        self.work.mkdir()
        self.read = self.root / 'read'
        self.read.mkdir()
        (self.read / 'input').write_text('readable')
        self.write = self.root / 'write'
        self.write.mkdir()
        self.outside = self.root / 'outside'
        self.outside.write_text('private-canary')
        self.endpoint = self.write / 'endpoint'
        self.listener = socket.socket(socket.AF_UNIX)
        self.listener.bind(str(self.endpoint))
        self.listener.listen(4)
        self.listener.settimeout(CAP)
        self.addCleanup(self.listener.close)
        self.lab = self.root / 'lab'
        self.driver = self.lab / 'keys/story/key/runs/R/driver'
        self.tool = dict(name='test', argv=[PYTHON, '-I', '-B', '-S', str(SUPPORT)], env={'ONLY': 'declared', 'LC_ALL': 'C'},
                         writes=[str(self.write)], reads=[str(self.read), str(SUPPORT)])

    def start(self):
        self.record = dict(format='lab-driver-tools/1', run='R', work=str(self.work), source=None,
                           socket=str(self.root / 'broker.sock'), tools=[self.tool])
        with store.open_lab(self.lab) as lab:
            lab.write_once('keys/story/key/runs/R/driver/tools.json', canonical.record_bytes(self.record))
        listener = socket.socket(socket.AF_UNIX)
        self.addCleanup(listener.close)
        listener.bind(self.record['socket'])
        listener.listen(1)
        listener.settimeout(CAP)
        code = 'import sys; sys.path.insert(0,sys.argv[1]); from runner.cli import main; sys.exit(main(["tool","R"],lab_root=sys.argv[2]))'
        previous = os.umask(0o077)
        try:
            self.process = subprocess.Popen([PYTHON, '-I', '-B', '-S', '-c', code, str(PACKAGE), str(self.lab)],
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                env={'PATH': os.defpath, 'INHERITED_SECRET': 'must-not-pass'})
        finally:
            os.umask(previous)
        self.addCleanup(self.stop)
        self.broker, _ = listener.accept()
        self.addCleanup(self.broker.close)
        self.send('initialize', {})
        self.assertEqual(receive_line(self.process.stdout)['result']['capabilities'], {'tools': {}})
        self.send('tools/list', {})
        self.assertEqual([t['name'] for t in receive_line(self.process.stdout)['result']['tools']], ['test'])

    def stop(self):
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait(timeout=CAP)
        for stream in (self.process.stdin, self.process.stdout, self.process.stderr):
            stream.close()

    def send(self, method, params):
        self.process.stdin.write(json.dumps(dict(jsonrpc='2.0', id=1, method=method, params=params)).encode() + b'\n')
        self.process.stdin.flush()

    def call(self, args, stdin=None):
        self.send('tools/call', dict(name='test', arguments=dict(args=args, stdin=stdin)))
        self.assertEqual(receive_line(self.broker), dict(start=dict(tool='test', args=args, stdin=stdin is not None)))

    def go(self, number=1):
        self.broker.sendall(json.dumps({'go': number}).encode() + b'\n')

    def accepted(self):
        connection, _ = self.listener.accept()
        connection.settimeout(CAP)
        self.addCleanup(connection.close)
        return connection

    def test_call_view_records_and_complete_environment(self):
        self.start()
        args = ['view', str(self.outside), str(self.read / 'input'), str(self.write), str(self.work),
                str(self.endpoint), os.readlink('/proc/self/ns/pid')]
        self.call(args, 'stdin text\n\0')
        self.assertFalse((self.driver / 'calls/1.stdin').exists())
        self.go()
        connection = self.accepted()
        self.assertEqual(connection.recv(32), b'connected')
        self.assertEqual(receive_line(self.broker), dict(end=1, status=0, signal=None))
        response = receive_line(self.process.stdout)['result']
        self.assertFalse(response['isError'], response)
        result = json.loads(response['content'][0]['text'])
        observed = json.loads(result['stdout'])
        self.assertEqual(observed, dict(read='readable', stdin='stdin text\n\0', env={'ONLY': 'declared', 'LC_ALL': 'C'},
            tmp=[], namespace=True, outside=False, read_write=False, work_write=False))
        self.assertEqual((self.write / 'allowed').read_text(), 'allowed')
        self.assertEqual((self.driver / 'calls/1.stdin').read_text(), 'stdin text\n\0')
        self.assertEqual((self.driver / 'calls/1.stdout').read_text(), result['stdout'])
        self.assertEqual((self.driver / 'calls/1.stderr').read_text(), result['stderr'])
        for path in (self.driver / 'calls').iterdir():
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
        self.assertFalse((self.driver / 'calls.jsonl').exists())

    def test_refusal_starts_nothing_and_keeps_serving(self):
        self.start()
        self.call(['touch', str(self.write / 'forbidden')])
        self.broker.sendall(b'{"refuse":"inventory mismatch"}\n')
        result = receive_line(self.process.stdout)['result']
        self.assertTrue(result['isError'])
        self.assertIn('inventory mismatch', result['content'][0]['text'])
        self.assertFalse((self.write / 'forbidden').exists())
        self.assertEqual(list((self.driver / 'calls').iterdir()), [])
        self.send('other', {})
        self.assertEqual(receive_line(self.process.stdout)['error']['code'], -32601)

    def test_out_of_order_calls_and_broker_close_under_backpressure(self):
        self.start()
        self.call(['wait', str(self.endpoint), str(self.write)])
        self.go()
        first = [self.accepted(), self.accepted()]
        self.assertEqual({c.recv(32) for c in first}, {b'parent', b'child'})
        self.call(['touch', str(self.write / 'second')])
        self.go(2)
        self.assertEqual(receive_line(self.broker), dict(end=2, status=0, signal=None))
        self.assertFalse(receive_line(self.process.stdout)['result']['isError'])
        self.assertEqual((self.write / 'second').read_text(), 'ran')
        self.call(['large'])
        self.go(3)
        self.assertEqual(receive_line(self.broker), dict(end=3, status=0, signal=None))
        # Leave the large MCP result unread while another view starts.
        self.call(['wait', str(self.endpoint), str(self.write)])
        self.go(4)
        last = [self.accepted(), self.accepted()]
        self.assertEqual({c.recv(32) for c in last}, {b'parent', b'child'})
        self.broker.close()
        self.assertEqual(self.process.wait(timeout=CAP), 0)
        for connection in first + last:
            self.assertEqual(connection.recv(1), b'')


class ProbeProcess(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='probe-', dir=PACKAGE.parents[1])
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source, self.work, self.home = [self.root / name for name in ('source', 'work', 'home')]
        for path in (self.source, self.work, self.home):
            path.mkdir()
        self.canary = self.root / 'canary'
        self.canary.write_text('probe-canary')
        self.codex = shutil.which('codex')
        self.assertIsNotNone(self.codex, 'native probe requires codex')

    def probe(self, *, profile=None):
        return boundary_probe(self.codex, self.root, source=str(self.source), work=str(self.work),
                              canary=str(self.canary), home=str(self.home), seconds=CAP, profile=profile)

    def test_real_codex_profile_passes_without_model(self):
        results = self.probe()
        self.assertTrue(all(r['ok'] for r in results.values()), results)
        self.assertEqual(set(results), {'canary_read', 'home_list', 'source_write', 'network',
                                       'pid_namespace', 'canary_process', 'work_write'})
        self.assertEqual({p.name for p in (self.root / 'probe/codex').iterdir()}, {'config.toml', 'tmp'})
        self.assertEqual(stat.S_IMODE((self.root / 'probe/codex/config.toml').stat().st_mode), 0o600)

    def test_weakened_home_profile_fails_before_provisioning(self):
        profile = views.profile_toml(os.path.realpath(self.codex), str(self.source), str(self.work))
        profile = profile.replace('[permissions.lab.network]', json.dumps(str(self.home)) + ' = "read"\n[permissions.lab.network]')
        with self.assertRaises(BoundaryError) as caught:
            self.probe(profile=profile)
        self.assertFalse(caught.exception.results['home_list']['ok'])
        self.assertTrue(caught.exception.results['work_write']['ok'])
