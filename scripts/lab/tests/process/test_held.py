"""Native held-file release, loss, owner settlement and interrupted handoff."""
import os
from pathlib import Path
import select
import signal
import socket
import stat
import subprocess
import time
import unittest
from runner import canonical, held, store
from . import test_runtime

PACKAGE, PYTHON, COST = test_runtime.PACKAGE, test_runtime.PYTHON, test_runtime.COST
SERVICE = Path(__file__).with_name('held_service.py')


class HeldRuntime(unittest.TestCase):
    setUpClass = classmethod(test_runtime.Runtime.setUpClass.__func__)
    setUp = test_runtime.Runtime.setUp
    tearDown = test_runtime.Runtime.tearDown
    records = test_runtime.Runtime.records
    assert_empty = test_runtime.Runtime.assert_empty

    def cli(self, *args, wait=True):
        bootstrap = ('import sys; sys.path.insert(0,sys.argv.pop(1)); from runner.cli import main; '
                     'sys.exit(main(sys.argv[2:],lab_root=sys.argv[1]))')
        process = subprocess.Popen([PYTHON, '-I', '-B', '-S', '-c', bootstrap,
                                    str(PACKAGE), str(self.lab_root), *args],
                                   stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.children.append(process)
        if not wait:
            return process
        stdout, stderr = process.communicate(timeout=COST['command_seconds'])
        return process.returncode, stdout, stderr

    def command(self, mode, *args):
        return [PYTHON, '-I', '-B', '-S', '{experiment}/held_service.py', mode, *args]

    def configure(self, *, lose=False, notification='-', seed_wait='exit'):
        (self.directory / 'held_service.py').write_bytes(SERVICE.read_bytes())
        action = dict(name='move-in', argv=self.command('seed', '{held.token}', '{proc.server.dir}/token',
                                                       notification, seed_wait), env={})
        if seed_wait == 'await':
            action['await'] = dict(process='server', stream='stdout', prefix='unused ', capture='word')
        release = (self.command('lose', '{proc.server.dir}/token') if lose else
                   self.command('return', '{proc.server.dir}/token', '{held.token}'))
        exp = dict(format='lab-experiment/1', name='native', cost='smoke', dirs=[],
                   processes=[dict(name='server', argv=self.command('serve'), env={}, memory_bytes=None,
                                   ready=[dict(stream='stdout', prefix='ready', capture=None)])],
                   actions=[action], release=[dict(name='move-out', argv=release, env={})],
                   held=['token'], driver=None, faults=[], planted=[], collect=[], checks=[])
        (self.directory / 'experiment.json').write_bytes(canonical.record_bytes(exp))

    def put(self, data):
        source = self.root / 'stand-in'
        source.write_bytes(data)
        status, output, stderr = self.cli('held', 'put', 'token', str(source))
        self.assertEqual((status, output), (0, b'token kept\n'), stderr)

    def run_cli(self, wait=True):
        return self.cli('run', str(self.directory), '--subject', self.subject, '--seed', '1', wait=wait)

    def root_for(self, record):
        return self.lab_root / 'keys/native' / record['key'] / 'runs' / record['run']

    def assert_given(self, record, data, output):
        path = self.lab_root / 'held/token'
        self.assertEqual(path.read_bytes(), data)
        self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
        self.assertFalse((self.lab_root / 'held/token.taken').exists())
        self.assertEqual([h['name'] for h in record['held']], ['token'])
        self.assertIsNotNone(record['held'][0]['given_ms'])
        self.assertGreaterEqual(record['held'][0]['given_ms'], record['held'][0]['taken_ms'])
        root = self.root_for(record)
        for path in root.rglob('*'):
            if path.is_file():
                self.assertNotIn(data, path.read_bytes(), str(path))
        self.assertNotIn(data, output)
        events = [canonical.loads(line + b'\n') for line in (root / 'events.jsonl').read_bytes().splitlines()]
        for event, field in (('held_taken', 'taken_ms'), ('held_given', 'given_ms')):
            self.assertEqual(next(e for e in events if e['event'] == event),
                             dict(format='lab-event/1', event=event, name='token', ms=record['held'][0][field]))
        release = next(c for c in record['commands'] if c['group'] == 'release')
        self.assertEqual(release['exit']['status'], 0)
        self.assert_empty(record)

    def test_held_roundtrip_leaves_no_bytes_in_run_or_output(self):
        start = time.monotonic()
        data = os.urandom(48)
        self.configure()
        self.put(data)
        status, stdout, stderr = self.run_cli()
        self.assertEqual(status, 0, stderr)
        record = self.records()[2][0]
        self.assertEqual(record['outcome'], 'pass')
        self.assert_given(record, data, stdout + stderr)
        print(f'held roundtrip: {time.monotonic() - start:.3f}s')

    def test_held_roundtrip_with_a_meter_reading(self):
        start = time.monotonic()
        data = os.urandom(48)
        self.configure()
        exp_path = self.directory / 'experiment.json'
        exp = canonical.loads(exp_path.read_bytes())
        exp['meter'] = dict(name='spend', argv=['/bin/echo', '3'], env={})
        exp_path.write_bytes(canonical.record_bytes(exp))
        settings_path = self.lab_root / 'settings.json'
        settings_path.chmod(0o600)
        settings = canonical.loads(settings_path.read_bytes())
        settings['costs']['smoke'].update(subject_spend=10, meter_seconds=1)
        settings_path.write_bytes(canonical.record_bytes(settings))
        self.put(data)
        status, stdout, stderr = self.run_cli()
        self.assertEqual(status, 0, stderr)
        record = self.records()[2][0]
        self.assertEqual(record['outcome'], 'pass')
        self.assertEqual(record['usage']['subject_spend'], 3)
        self.assert_given(record, data, stdout + stderr)
        root = self.root_for(record)
        events = [canonical.loads(line + b'\n')['event'] for line in (root / 'events.jsonl').read_bytes().splitlines()]
        self.assertLess(events.index('held_taken'), events.index('meter'))
        self.assertLess(events.index('meter'), events.index('held_given'))
        print(f'held roundtrip with a meter: {time.monotonic() - start:.3f}s')

    def test_held_loss_refuses_next_run_until_owner_put(self):
        start = time.monotonic()
        data = os.urandom(48)
        self.configure(lose=True)
        self.put(data)
        status, stdout, stderr = self.run_cli()
        self.assertEqual(status, 1, stderr)
        record = self.records()[2][0]
        self.assertEqual(record['outcome'], 'fail')
        self.assertIn('release/held:token', [f['signature'] for f in record['failures']])
        self.assertIsNone(record['held'][0]['given_ms'])
        self.assertTrue((self.root_for(record) / 'scratch/server').is_dir())
        with store.open_lab(self.lab_root) as lab:
            taken = held.taken_record(lab, 'token')
        self.assertEqual(taken['run'], record['run'])
        status, listing, stderr = self.cli('held')
        self.assertEqual((status, listing), (0, f"token taken {record['run']} {taken['taken']}\n".encode()), stderr)
        status, output, stderr = self.run_cli()
        self.assertEqual((status, output), (2, b''))
        self.assertIn(('held.token: taken by run ' + record['run']).encode(), stderr)
        self.assertEqual(len(self.records()[2]), 1)
        self.put(data)
        self.configure()
        status, stdout, stderr = self.run_cli()
        self.assertEqual(status, 0, stderr)
        # The corrected release is a new experiment and key.
        saved = [canonical.loads(p.read_bytes()) for p in self.lab_root.glob('keys/native/*/runs/*/run.json')]
        self.assertEqual(sorted(r['outcome'] for r in saved), ['fail', 'pass'])
        self.assert_given(next(r for r in saved if r['outcome'] == 'pass'), data, stdout + stderr)
        print(f'held loss and settlement: {time.monotonic() - start:.3f}s')

    def interrupt_story(self, seed_wait):
        start = time.monotonic()
        data = os.urandom(48)
        listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        endpoint = str(self.root / 'notify')
        listener.bind(endpoint)
        listener.listen(1)
        self.addCleanup(listener.close)
        self.configure(notification=endpoint, seed_wait=seed_wait)
        self.put(data)
        process = self.run_cli(wait=False)
        ready, _, _ = select.select([listener, process.stdout, process.stderr], [], [], COST['ready_seconds'])
        if listener not in ready:
            stdout, stderr = process.communicate(timeout=COST['command_seconds'])
            self.fail('held seed did not notify: ' + repr((stdout, stderr)))
        connection, _ = listener.accept()
        with connection:
            connection.settimeout(COST['ready_seconds'])
            with connection.makefile('rb') as stream:
                self.assertEqual(stream.readline(64), b'moved\n')
        process.send_signal(signal.SIGTERM)
        stdout, stderr = process.communicate(timeout=COST['command_seconds'])
        self.assertEqual(process.returncode, 143, stderr)
        record = self.records()[2][0]
        self.assertEqual(record['outcome'], 'error')
        self.assertTrue(any(e['what'] == 'interrupted' for e in record['errors']))
        self.assert_given(record, data, stdout + stderr)
        print(f'held interrupted {seed_wait}: {time.monotonic() - start:.3f}s')

    def test_held_interrupted_seed_await_still_returns_file(self):
        self.interrupt_story('await')

    def test_held_interrupted_running_seed_still_returns_file(self):
        self.interrupt_story('hold')

    def test_killed_unit_is_swept_and_held_file_stays_taken(self):
        from runner.host import manager
        listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        listener.bind(str(self.root / 'notify'))
        listener.listen(1)
        self.addCleanup(listener.close)
        self.configure(notification=str(self.root / 'notify'), seed_wait='hold')
        self.put(b'throwaway-held-file')
        process = self.run_cli(wait=False)
        self.assertTrue(select.select([listener], [], [], COST['ready_seconds'])[0])
        connection, _ = listener.accept()
        connection.close()
        live = next(self.lab_root.glob('keys/native/*/runs/*/live.json'))
        marker = canonical.loads(live.read_bytes())
        manager(self.systemctl, ['kill', '--signal=SIGKILL', marker['unit']], COST['command_seconds'])
        process.communicate(timeout=COST['command_seconds'])
        self.assertEqual(process.returncode, 4)
        # A different held put is a writing command that can proceed after the sweep.
        source = self.root / 'other'
        source.write_bytes(b'other throwaway')
        status, _out, stderr = self.cli('held', 'put', 'other', str(source))
        self.assertEqual(status, 0, stderr)
        record = canonical.loads((live.parent / 'run.json').read_bytes())
        self.assertEqual(record['outcome'], 'error')
        self.assertEqual(record['held'][0]['given_ms'], None)
        self.assertFalse(live.exists())
        status, listing, stderr = self.cli('held')
        self.assertEqual(status, 0, stderr)
        self.assertIn(('token taken ' + record['run']).encode(), listing)
