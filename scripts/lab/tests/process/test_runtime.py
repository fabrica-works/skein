"""Native user units, spawns and cgroups; missing containment fails the stories."""
import json
import io
import os
from pathlib import Path
import select
import shutil
import signal
import socket
import subprocess
import sys
import tarfile
import tempfile
import unittest
from runner import canonical, evidence, finding_store, store
from runner.host import CapabilityError, await_removed, manager, namespace_probe

PYTHON = os.path.realpath(sys.executable)
PACKAGE = Path(__file__).resolve().parents[2]
SUPPORT = PACKAGE / 'tests/support/participant.py'
# Approved smoke operational caps, supplied as experiment settings.
COST = dict(ready_seconds=60, command_seconds=120, stop_seconds=10,
            driver_seconds=900, driver_actions=80, driver_input_tokens=2500000)


class Runtime(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.systemctl = shutil.which('systemctl')
        bwrap = shutil.which('bwrap')
        if not cls.systemctl or not bwrap or not shutil.which('systemd-run') or not hasattr(os, 'pidfd_open'):
            raise AssertionError('native process tier requires systemd user manager, bubblewrap and pidfds')
        try:
            manager(cls.systemctl, ['show', '--property=Version', '--value'], COST['command_seconds'])
            namespace_probe(bwrap, PYTHON, COST['command_seconds'])
        except (OSError, CapabilityError, subprocess.SubprocessError) as error:
            raise AssertionError('native capability refused: ' + str(error)) from error

    def setUp(self):
        # Native scratch is beneath tmp/lab, always private.
        scratch = Path.home() / 'src/rust/tmp/lab'
        scratch.mkdir(mode=0o700, parents=True, exist_ok=True)
        if scratch.stat().st_mode & 0o077:
            self.fail('native scratch tmp/lab is not private')
        self.temp = tempfile.TemporaryDirectory(prefix='runtime-', dir=scratch)
        self.root = Path(self.temp.name)
        self.lab_root = self.root / 'lab'
        with store.open_lab(self.lab_root) as lab:
            lab.write_once('settings.json', canonical.record_bytes(dict(format='lab-settings/1', costs={'smoke': COST}, driver=None, triager=None)))
            subject = dict(format='lab-subject/1', commit='a' * 40, source=canonical.tree_digest([]), artifacts={})
            self.subject = canonical.digest(subject)
            lab.capture('subjects', self.subject, [dict(path='subject.json', data=canonical.record_bytes(subject), exec=False)], empty_dirs=('source',))
        self.directory = self.root / 'experiment'
        self.directory.mkdir(mode=0o700)
        (self.directory / 'participant.py').write_bytes(SUPPORT.read_bytes())
        self.children = []
        self.canaries = []

    def tearDown(self):
        for process in self.children:
            if process.poll() is None:
                process.send_signal(signal.SIGTERM)
                try:
                    process.wait(timeout=COST['stop_seconds'])
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            for stream in (process.stdout, process.stderr):
                if stream is not None:
                    stream.close()
        # A timed-out outer CLI can leave its worker holding the writer lock.
        # Only this test's private run markers identify units for cleanup.
        for live in self.lab_root.glob('keys/native/*/runs/*/live.json'):
            unit = canonical.loads(live.read_bytes())['unit']
            self.assertEqual(unit, 'lab-' + live.parent.name + '.service')
            try:
                group = manager(self.systemctl, ['show', '--property=ControlGroup', '--value', unit], COST['stop_seconds'])
                if group:
                    manager(self.systemctl, ['kill', '--kill-whom=all', '--signal=SIGKILL', unit], COST['stop_seconds'])
                    await_removed('/sys/fs/cgroup' + group, COST['stop_seconds'])
            except CapabilityError as error:
                if 'not loaded' not in str(error) and 'not found' not in str(error):
                    raise
        for fd in self.canaries:
            try:
                signal.pidfd_send_signal(fd, signal.SIGKILL)
            except ProcessLookupError:
                pass
            os.close(fd)
        for directory, _, _ in os.walk(self.root):
            os.chmod(directory, 0o700)
        self.temp.cleanup()

    def command(self, mode, *args):
        return [PYTHON, '-I', '-B', '-S', '{experiment}/participant.py', mode, *args]

    def experiment(self, *, check='pass', detach='none', notify='-', dependent=False, unexpected=False, panic=False):
        server = dict(name='server', argv=self.command('panic') if panic else self.command('server', notify, detach, '{run.id}'), env={},
                      ready=[dict(stream='stdout', prefix='pid ', capture='pid'), dict(stream='stdout', prefix='ready ', capture='address')], memory_bytes=None)
        processes = [server]
        if dependent:
            (self.directory / 'dependent.conf').write_text('{proc.server.ready.address}')
            processes.append(dict(name='dependent', after=['server'],
                                  files=[dict(path='endpoint', **{'from': 'dependent.conf'})],
                                  argv=self.command('dependent', '{proc.dependent.dir}/endpoint'), env={},
                                  ready=[dict(stream='stdout', prefix='ready ', capture=None)], memory_bytes=None))
        actions = []
        if unexpected:
            actions = [dict(name='exit', argv=self.command('exit-service', '{proc.server.ready.address}', '{proc.server.ready.pid}'), env={}),
                       dict(name='later', argv=self.command('touch', '{dir.client}/later'), env={})]
        value = dict(format='lab-experiment/1', name='native', cost='smoke', dirs=['client'], processes=processes,
                     actions=actions, driver=None, faults=[], planted=[],
                     collect=[dict(name='obs', argv=self.command('collect', '{collect.dir}'), env={}, scan=['log'])],
                     checks=[dict(name='end', argv=self.command('check', check, '{collected.obs}', '{run.seed}'), env={})])
        (self.directory / 'experiment.json').write_bytes(canonical.record_bytes(value))

    def cli(self, command, *, seed=None, wait=True):
        bootstrap = 'import sys; sys.path.insert(0,sys.argv.pop(1)); from runner.cli import main; sys.exit(main(sys.argv[2:],lab_root=sys.argv[1]))'
        args = [PYTHON, '-I', '-B', '-S', '-c', bootstrap, str(PACKAGE), str(self.lab_root), command, str(self.directory), '--subject', self.subject]
        if seed is not None:
            args.extend(['--seed', seed])
        process = subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.children.append(process)
        if not wait:
            return process
        stdout, stderr = process.communicate(timeout=COST['command_seconds'])
        if process.returncode == 2 and any(part in stderr for part in (b'containment', b'bubblewrap')):
            self.fail('native provisioning refused: ' + stderr.decode('utf-8', 'replace'))
        if process.returncode == 4 and b'containment/provision: native provision refused:' in stderr:
            self.fail('native provisioning refused: ' + stderr.decode('utf-8', 'replace'))
        return process.returncode, stdout.decode().strip(), stderr.decode('utf-8', 'replace')

    def records(self):
        keys = list((self.lab_root / 'keys/native').iterdir())
        self.assertEqual(len(keys), 1)
        key_path = 'keys/native/' + keys[0].name
        with store.open_lab(self.lab_root) as lab:
            entries = evidence.read(lab, key_path)
            records = [canonical.loads(lab.read(key_path + '/runs/' + e['run'] + '/run.json')) for e in entries]
        return key_path, entries, records

    def assert_empty(self, record):
        run_dir = self.lab_root / 'keys/native' / record['key'] / 'runs' / record['run']
        self.assertFalse((run_dir / 'live.json').exists())
        events = [canonical.loads(line + b'\n') for line in (run_dir / 'events.jsonl').read_bytes().splitlines()]
        containment = next(e for e in events if e['event'] == 'containment')
        self.assertFalse(Path(containment['cgroup']).exists(), containment)
        for command in record['commands']:
            if 'exit' in command:
                observed = next(e for e in events if e['event'] == 'exit' and e['group'] == command['group']
                                and e['name'] == command['name'])
                self.assertEqual(observed['ms'], command['exit']['ms'])
        for proc in record['processes'].values():
            self.assertIsNotNone(proc['exit'])
            self.assertIsNotNone(proc['memory']['peak_bytes'])
            self.assertIsNotNone(proc['memory']['tree_peak_bytes'])
            self.assertIn(proc['memory']['source'], ('VmHWM', 'wait4'))

    def stopped_experiment(self, failed_copy=False):
        path = self.directory / 'experiment.json'
        exp = canonical.loads(path.read_bytes())
        exp['processes'][0]['argv'].append('{proc.server.dir}/terminal')
        argv = (self.command('partial-copy', '{collect.dir}') if failed_copy else
                ['/bin/cp', '-R', '--', '{proc.server.dir}/.', '{collect.dir}/.'])
        exp['collect'].append(dict(name='state', cluster='stopped', argv=argv, env={}, scan=[]))
        exp['checks'] += [dict(name='state', cluster='stopped',
                               argv=self.command('check-terminal', '{collected.state}'), env={}),
                          dict(name='independent', cluster='stopped',
                               argv=self.command('touch', '{dir.client}/independent'), env={})]
        path.write_bytes(canonical.record_bytes(exp))

    def test_stopped_copy_observes_sigterm_terminal_state(self):
        self.experiment()
        self.stopped_experiment()
        status, _run, stderr = self.cli('run', seed='1')
        self.assertEqual(status, 0, stderr)
        record = self.records()[2][0]
        self.assertEqual([c['name'] for c in record['commands']], ['obs', 'end', 'state', 'state', 'independent'])
        self.assertTrue(all(c.get('cluster') == 'stopped' for c in record['commands'][2:]))
        root = self.lab_root / 'keys/native' / record['key'] / 'runs' / record['run']
        self.assertEqual((root / 'collected/state/terminal').read_text(), 'terminal\n')
        events = [canonical.loads(line + b'\n') for line in (root / 'events.jsonl').read_bytes().splitlines()]
        stopped = next(e['ms'] for e in events if e['event'] == 'phase' and e['phase'] == 'stopped-collect')
        emptied = next(e['ms'] for e in events if e['event'] == 'tree_empty' and e['group'] == 'processes')
        self.assertGreaterEqual(stopped, emptied)
        self.assert_empty(record)

    def test_partial_failed_stopped_copy_never_starts_dependent_check(self):
        self.experiment()
        self.stopped_experiment(failed_copy=True)
        status, _run, stderr = self.cli('run', seed='1')
        self.assertEqual(status, 3, stderr)
        record = self.records()[2][0]
        copy = next(c for c in record['commands'] if c['group'] == 'collect' and c['name'] == 'state')
        check = next(c for c in record['commands'] if c['group'] == 'checks' and c['name'] == 'state')
        self.assertEqual(copy['exit']['status'], 7)
        self.assertEqual(check, dict(group='checks', name='state', cluster='stopped',
                                     skipped='collection input did not complete successfully'))
        self.assertTrue(any(u['check'] == 'checks/state' for u in record['undecided']))
        root = self.lab_root / 'keys/native' / record['key'] / 'runs' / record['run']
        self.assertEqual((root / 'collected/state/terminal').read_text(), 'partial\n')
        self.assertTrue((root / 'dirs/client/independent').is_file())
        self.assert_empty(record)

    def test_interrupt_copies_nothing_and_skips_stopped_checks(self):
        process, _connection, _note, fds = self.notified('none', self.stopped_experiment)
        process.send_signal(signal.SIGINT)
        _stdout, stderr = process.communicate(timeout=COST['command_seconds'])
        self.assertEqual(process.returncode, 130, stderr)
        self.assert_exited(fds)
        record = self.records()[2][0]
        stopped = [c for c in record['commands'] if c.get('cluster') == 'stopped']
        self.assertEqual(len(stopped), 3)
        self.assertTrue(all('skipped' in c and 'exit' not in c for c in stopped))
        root = self.lab_root / 'keys/native' / record['key'] / 'runs' / record['run']
        self.assertEqual(list((root / 'collected/state').iterdir()), [])
        self.assert_empty(record)

    def test_live_check_machinery_error_skips_stopped_copy(self):
        self.experiment(check='malformed')
        self.stopped_experiment()
        status, _run, stderr = self.cli('run', seed='1')
        self.assertEqual(status, 4, stderr)
        record = self.records()[2][0]
        stopped = [c for c in record['commands'] if c.get('cluster') == 'stopped']
        self.assertEqual(len(stopped), 3)
        self.assertTrue(all('skipped' in c for c in stopped))
        root = self.lab_root / 'keys/native' / record['key'] / 'runs' / record['run']
        self.assertEqual(list((root / 'collected/state').iterdir()), [])
        self.assert_empty(record)

    def test_exec_ack_error_skips_stopped_copy(self):
        self.experiment()
        program = self.directory / 'bad-executable'
        program.write_text('not an executable format\n')
        program.chmod(0o700)
        path = self.directory / 'experiment.json'
        exp = canonical.loads(path.read_bytes())
        exp['processes'][0]['argv'] = ['{experiment}/bad-executable']
        path.write_bytes(canonical.record_bytes(exp))
        self.stopped_experiment()
        status, _run, stderr = self.cli('run', seed='1')
        self.assertEqual(status, 4, stderr)
        record = self.records()[2][0]
        self.assertTrue(any(e['what'] == 'spawn/server' for e in record['errors']))
        stopped = [c for c in record['commands'] if c.get('cluster') == 'stopped']
        self.assertEqual(len(stopped), 3)
        self.assertTrue(all('skipped' in c for c in stopped))
        self.assert_empty(record)

    def listener(self, name):
        listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        listener.bind(str(self.root / name))
        listener.listen(1)
        listener.settimeout(COST['ready_seconds'])
        self.addCleanup(listener.close)
        return listener

    def notification(self, listener, process):
        ready, _, _ = select.select([listener, process.stdout, process.stderr], [], [], COST['ready_seconds'])
        self.assertTrue(listener in ready, 'stand-in must notify before the runner exits')
        connection, _ = listener.accept()
        self.addCleanup(connection.close)
        connection.settimeout(COST['ready_seconds'])
        data = b''
        while not data.endswith(b'\n'):
            chunk = connection.recv(4096)
            self.assertTrue(bool(chunk), 'stand-in must complete its public notification')
            data += chunk
        return connection, json.loads(data)

    def observe_public_piece(self, path, marker=b'piece\n'):
        # Inotify wakes on retained public stderr. Two acknowledged markers
        # force another stdout read before the source releases the second piece.
        import ctypes
        import time
        libc = ctypes.CDLL(None, use_errno=True)
        fd = libc.inotify_init1(os.O_CLOEXEC | os.O_NONBLOCK)
        self.assertGreaterEqual(fd, 0, 'inotify is required for native stream observations')
        try:
            watch = libc.inotify_add_watch(fd, ctypes.c_char_p(os.fsencode(path)), 0x2 | 0x8)
            self.assertGreaterEqual(watch, 0, 'public evidence stream must be watchable')
            deadline = time.monotonic() + COST['ready_seconds']
            while marker not in path.read_bytes():
                ready, _, _ = select.select([fd], [], [], max(0, deadline - time.monotonic()))
                self.assertTrue(bool(ready), 'runner must observe the public piece within ready_seconds')
                os.read(fd, 65536)
        finally:
            os.close(fd)

    def bootstrap_story(self, source_mode='normal', recipient_mode='success', interrupt=False):
        source_listener = self.listener('source-notify')
        recipient_listener = self.listener('recipient-notify') if recipient_mode == 'hold' else None
        self.experiment()
        path = self.directory / 'experiment.json'
        exp = canonical.loads(path.read_bytes())
        server = exp['processes'][0]
        server['argv'] = self.command('private-source', str(self.root / 'source-notify'), '{run.id}', source_mode)
        server['bootstrap'] = dict(stream='stdout', action='seed')
        for rule in server['ready']:
            rule['stream'] = 'stderr'
        exp['actions'] = [dict(name='seed', argv=self.command('private-echo', recipient_mode, '{dir.client}/received',
                                                            str(self.root / 'recipient-notify'), '{run.id}'), env={})]
        path.write_bytes(canonical.record_bytes(exp))
        self.stopped_experiment()
        process = self.cli('run', seed='1', wait=False)
        source, note = self.notification(source_listener, process)
        for pid in (note['main'], note['first']):
            self.canaries.append(os.pidfd_open(pid))
        keys = list((self.lab_root / 'keys/native').iterdir())
        self.assertEqual(len(keys), 1)
        root = keys[0] / 'runs' / note['run']
        self.observe_public_piece(root / 'proc/server/stderr')
        source.sendall(b'P')
        self.observe_public_piece(root / 'proc/server/stderr', b'piece-two\n')
        source.sendall(b'G')
        if recipient_listener is not None:
            _recipient, recipient_note = self.notification(recipient_listener, process)
            self.canaries.append(os.pidfd_open(recipient_note['first']))
            if interrupt:
                process.send_signal(signal.SIGINT)
            else:
                source.sendall(b'H')
        stdout, stderr = process.communicate(timeout=COST['command_seconds'])
        # Only booleans and public paths enter assertions, even on a leak.
        canary = b'canary-' + note['run'].encode('ascii') + b'-private'
        first_piece = (canary + b'\n')[:-10]
        for private in (canary, first_piece):
            self.assertTrue(private not in stdout and private not in stderr, 'runner output must contain no private bytes')
            for retained in self.lab_root.rglob('*'):
                if retained.is_file():
                    self.assertTrue(private not in retained.read_bytes(), 'private bytes retained at ' + str(retained.relative_to(self.lab_root)))
        record = self.records()[2][0]
        channel = record['processes']['server']['bootstrap']
        self.assertEqual(channel['stream'], 'stdout')
        self.assertEqual(channel['action'], 'seed')
        self.assertIsNone(record['processes']['server']['output']['stdout'])
        self.assertFalse((root / 'proc/server/stdout').exists())
        events = [canonical.loads(line + b'\n') for line in (root / 'events.jsonl').read_bytes().splitlines()]
        for event in events:
            if event['event'] in ('bootstrap_received', 'bootstrap_delivered'):
                self.assertEqual(set(event), {'format', 'event', 'ms', 'process', 'action'})
                self.assertEqual(event['process'], 'server')
                self.assertEqual(event['action'], 'seed')
                field = 'received_ms' if event['event'] == 'bootstrap_received' else 'delivered_ms'
                self.assertEqual(channel[field], event['ms'])
        commands = [c for c in record['commands'] if c['group'] == 'actions']
        if source_mode in ('partial', 'extra'):
            self.assertEqual(commands, [])
            self.assertIsNone(channel['received_ms'])
            self.assertIsNone(channel['delivered_ms'])
        else:
            self.assertIsNotNone(channel['received_ms'])
            self.assertIsNotNone(channel['delivered_ms'])
            self.assertGreaterEqual(channel['delivered_ms'], channel['received_ms'])
            self.assertEqual(len(commands), 1)
            self.assertEqual(commands[0]['bootstrap'], dict(process='server'))
            self.assertIsNone(commands[0]['stdin'])
            self.assertEqual(commands[0]['output'], dict(stdout=None, stderr=None))
            for stream in ('stdin', 'stdout', 'stderr'):
                self.assertFalse((root / ('commands/actions-seed.' + stream)).exists())
            if recipient_mode != 'closed':
                self.assertEqual((root / 'dirs/client/received').read_text(), 'exact\n')
        self.assertFalse(any(f['kind'] == 'panic' for f in record['failures']))
        self.assert_empty(record)
        return process.returncode, record

    def test_split_private_input_echoed_on_both_streams_is_never_retained(self):
        status, record = self.bootstrap_story()
        self.assertEqual(status, 0)
        self.assertEqual(record['outcome'], 'pass')

    def test_private_recipient_nonzero_keeps_action_signature_without_evidence(self):
        status, record = self.bootstrap_story(recipient_mode='nonzero')
        self.assertEqual(status, 1)
        failure = next(f for f in record['failures'] if f['signature'] == 'action/seed/exit:7')
        self.assertEqual(failure['evidence'], [])

    def test_private_recipient_abort_keeps_signal_signature_without_evidence(self):
        status, record = self.bootstrap_story(recipient_mode='abort')
        self.assertEqual(status, 1)
        failure = next(f for f in record['failures'] if f['signature'] == 'action/seed/signal:SIGABRT')
        self.assertEqual(failure['evidence'], [])

    def test_private_recipient_closing_stdin_is_an_action_failure(self):
        status, record = self.bootstrap_story(recipient_mode='closed')
        self.assertEqual(status, 1)
        self.assertTrue(any(f['signature'] == 'action/seed/exit:7' for f in record['failures']))
        self.assertEqual(record['errors'], [])

    def test_extra_private_bytes_cut_the_echoing_recipient_and_still_copy_stopped(self):
        status, record = self.bootstrap_story(source_mode='cut', recipient_mode='hold')
        self.assertEqual(status, 1)
        failure = next(f for f in record['failures'] if f['signature'] == 'bootstrap/server/extra')
        self.assertEqual(failure['kind'], 'bootstrap')
        self.assertEqual(failure['evidence'], [])
        recipient = next(c for c in record['commands'] if c['group'] == 'actions')
        self.assertTrue(recipient['exit']['stopped'])
        self.assertTrue(any(c['group'] == 'checks' and c.get('cluster') == 'stopped' and 'exit' in c
                            for c in record['commands']))

    def test_partial_private_input_cuts_before_recipient_spawn(self):
        status, record = self.bootstrap_story(source_mode='partial')
        self.assertEqual(status, 1)
        self.assertTrue(any(f['signature'] == 'bootstrap/server/partial' for f in record['failures']))

    def test_line_and_extra_private_bytes_never_deliver_the_line(self):
        status, record = self.bootstrap_story(source_mode='extra')
        self.assertEqual(status, 1)
        self.assertTrue(any(f['signature'] == 'bootstrap/server/extra' for f in record['failures']))

    def test_interrupt_drops_private_input_and_cancels_recipient_without_copying(self):
        status, record = self.bootstrap_story(recipient_mode='hold', interrupt=True)
        self.assertEqual(status, 130)
        stopped = [c for c in record['commands'] if c.get('cluster') == 'stopped']
        self.assertTrue(all('skipped' in c for c in stopped))
        self.assertTrue(any(e['what'] == 'interrupted' for e in record['errors']))

    def test_ready_lines_late_dependent_file_and_pass_evidence(self):
        self.experiment(dependent=True)
        status, _run, stderr = self.cli('run', seed='18446744073709551615')
        self.assertEqual(status, 0, stderr)
        key_path, entries, records = self.records()
        record = records[0]
        self.assertIsNone(record['driver'])
        self.assertIsNone(record['usage']['driver'])
        self.assertEqual(record['seed'], '18446744073709551615')
        self.assertEqual(record['schedule']['start_order'], ['server', 'dependent'])
        rendered = self.lab_root / key_path / 'runs' / record['run'] / 'scratch/dependent/endpoint'
        self.assertEqual(rendered.read_text(), record['processes']['server']['captures']['address'])
        self.assertTrue(record['processes']['server']['exit']['stopped'])
        self.assertEqual(entries[0]['outcome'], 'pass')
        self.assert_empty(record)

    def test_action_awaits_fresh_process_line_and_next_action_receives_capture_on_stdin(self):
        self.experiment()
        path = self.directory / 'experiment.json'
        exp = canonical.loads(path.read_bytes())
        exp['actions'] = [dict(name='refresh', argv=self.command('request-fresh', '{proc.server.ready.address}'), env={},
                               **{'await': dict(process='server', stream='stdout', prefix='fresh ', capture='word')}),
                          dict(name='use', argv=self.command('capture-stdin'), env={}, stdin='{action.refresh.word}\n')]
        path.write_bytes(canonical.record_bytes(exp))
        status, _run, stderr = self.cli('run', seed='1')
        self.assertEqual(status, 0, stderr)
        key_path, _entries, records = self.records()
        record = records[0]
        refresh, use = record['commands'][:2]
        self.assertEqual(refresh['awaited']['captures'], {'word': 'invented-word'})
        self.assertNotIn('awaited', use)
        self.assertEqual(use['exit']['status'], 0)
        root = self.lab_root / key_path / 'runs' / record['run']
        self.assertEqual((root / use['stdin']['path']).read_text(), 'invented-word\n')
        events = [canonical.loads(line + b'\n') for line in (root / 'events.jsonl').read_bytes().splitlines()]
        awaited = next(e for e in events if e['event'] == 'awaited')
        self.assertEqual({k: awaited[k] for k in ('ms', 'captures')}, refresh['awaited'])
        self.assertEqual(awaited['name'], 'refresh')
        self.assert_empty(record)

    def test_artifact_tree_is_read_only_before_process_readiness(self):
        contents = io.BytesIO()
        with tarfile.open(fileobj=contents, mode='w', format=tarfile.USTAR_FORMAT) as output:
            for name, kind, mode, data in [('nested', tarfile.DIRTYPE, 0o755, b''),
                                           ('nested/index', tarfile.REGTYPE, 0o644, b'{unrendered}\n'),
                                           ('run', tarfile.REGTYPE, 0o755, b'#!/bin/sh\n')]:
                info = tarfile.TarInfo(name)
                info.type, info.mode, info.size = kind, mode, len(data)
                output.addfile(info, io.BytesIO(data))
        data = contents.getvalue()
        subject = dict(format='lab-subject/1', commit='a' * 40, source=canonical.tree_digest([]),
                       artifacts={'bundle': dict(sha256=store.file_digest(data), bytes=len(data), exec=False)})
        self.subject = canonical.digest(subject)
        with store.open_lab(self.lab_root) as lab:
            lab.capture('subjects', self.subject,
                        [dict(path='subject.json', data=canonical.record_bytes(subject), exec=False),
                         dict(path='artifacts/bundle', data=data, exec=False)], empty_dirs=('source',))
        self.experiment()
        path = self.directory / 'experiment.json'
        exp = canonical.loads(path.read_bytes())
        exp['processes'] = [dict(name='reader', argv=self.command('tree-reader', '{proc.reader.dir}/web'), env={},
                                  files=[dict(path='web', tree='bundle')], memory_bytes=None,
                                  ready=[dict(stream='stdout', prefix='ready ', capture='tree')])]
        path.write_bytes(canonical.record_bytes(exp))
        status, _run, stderr = self.cli('run', seed='1')
        self.assertEqual(status, 0, stderr)
        record = self.records()[2][0]
        self.assertEqual(record['processes']['reader']['captures'], {'tree': 'read-only-tree'})
        self.assert_empty(record)

    def test_unexpected_stop_cuts_seed_actions_and_still_checks(self):
        self.experiment(unexpected=True)
        status, _run, stderr = self.cli('run', seed='1')
        self.assertEqual(status, 1, stderr)
        _path, _entries, records = self.records()
        record = records[0]
        self.assertIn('stop/server/exit:17', [f['signature'] for f in record['failures']])
        self.assertFalse(any(c['name'] == 'later' for c in record['commands']))
        self.assertTrue(any(c['group'] == 'checks' for c in record['commands']))
        self.assert_empty(record)

    def test_malformed_check_is_error_and_explicit_retry_is_additive(self):
        self.experiment(check='malformed')
        for seed, attempt in (('1', 1), ('1', 2)):
            status, _run, stderr = self.cli('run', seed=seed)
            self.assertEqual(status, 4, stderr)
            _path, entries, records = self.records()
            self.assertEqual(entries[-1]['attempt'], attempt)
            self.assertTrue(any(e['what'] == 'check/end' for e in records[-1]['errors']))
            self.assert_empty(records[-1])
        status, output, stderr = self.cli('judge')
        self.assertEqual(status, 3, stderr)
        self.assertEqual(output, 'not enough evidence')
        self.assertEqual(len(self.records()[1]), 2)

    def test_three_seeds_judge_and_fourth_seed_refusal(self):
        self.experiment()
        status, output, stderr = self.cli('judge')
        self.assertEqual(status, 0, stderr)
        self.assertEqual(output, 'pass')
        key_path, entries, records = self.records()
        self.assertEqual(len({e['seed'] for e in entries}), 3)
        for record in records:
            self.assert_empty(record)
        before = (self.lab_root / key_path / 'evidence.jsonl').read_bytes()
        status, output, stderr = self.cli('judge')
        self.assertEqual((status, output), (0, 'pass'), stderr)
        self.assertEqual((self.lab_root / key_path / 'evidence.jsonl').read_bytes(), before)
        fourth = next(str(n) for n in range(4) if str(n) not in {e['seed'] for e in entries})
        status, _out, stderr = self.cli('run', seed=fourth)
        self.assertEqual(status, 2, stderr)
        self.assertIn('fourth', stderr)
        self.assertEqual((self.lab_root / key_path / 'evidence.jsonl').read_bytes(), before)

    def test_panic_replaces_stop_signature(self):
        self.experiment(panic=True)
        status, _run, stderr = self.cli('run', seed='1')
        self.assertEqual(status, 1, stderr)
        record = self.records()[2][0]
        signatures = [f['signature'] for f in record['failures']]
        self.assertIn('panic/server/standin.rs:7:2', signatures)
        self.assertFalse(any(s.startswith('stop/server/') for s in signatures))
        self.assert_empty(record)

    def test_retained_judged_failures_through_owner_experiment_recheck(self):
        self.experiment(check='fail')
        original_bytes = (self.directory / 'experiment.json').read_bytes()
        for seed in ('1', '2', '3', '1'):
            status, _output, stderr = self.cli('run', seed=seed)
            self.assertEqual(status, 1, stderr)
        original_path, _lines, records = self.records()
        for record in records:
            self.assert_empty(record)
        t = records[-1]['ended']
        with store.open_lab(self.lab_root) as lab, lab.lock():
            derived = finding_store.sync(lab, t)
            finding = next(f for f in derived['findings'] if f['signature'] == 'check/end/state/private-canary')
            identity = finding['id']
            finding_store.act(lab, identity, 'class', t, 'owner', finding_class='experiment')
            finding_store.act(lab, identity, 'file', t, 'owner')
            finding_store.act(lab, identity, 'take', t, 'owner')
        self.experiment(check='pass')
        status, output, stderr = self.cli('judge')
        self.assertEqual((status, output), (0, 'pass'), stderr)
        with store.open_lab(self.lab_root) as lab, lab.lock():
            records_by_key, paths = finding_store.sources(lab)
            corrected = next(k for k in paths if paths[k] != original_path)
            key = canonical.loads(lab.read(paths[corrected] + '/key.json'))
            t = records_by_key[corrected][-1]['ended']
            for record in records_by_key[corrected]:
                self.assert_empty(record)
            finding_store.act(lab, identity, 'fix', t, 'owner', fix={'experiment': key['experiment']})
            derived = finding_store.act(lab, identity, 'close', t, 'owner', recheck=corrected)
            self.assertEqual(next(f['state'] for f in derived['findings'] if f['id'] == identity), 'closed')
        # The retained faulty experiment still fails and attaches while closed.
        (self.directory / 'experiment.json').write_bytes(original_bytes)
        status, _output, stderr = self.cli('run', seed='1')
        self.assertEqual(status, 1, stderr)
        with store.open_lab(self.lab_root) as lab, lab.lock():
            values = finding_store.histories(lab)[identity]
            self.assertEqual(values[-1]['event'], 'occurred')
            derived = finding_store.sync(lab, values[-1]['t'])
            self.assertEqual(next(f['state'] for f in derived['findings'] if f['id'] == identity), 'closed')

    def notified(self, detach, configure=None):
        endpoint = str(self.root / 'notify')
        listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        listener.bind(endpoint)
        listener.listen(1)
        listener.settimeout(COST['ready_seconds'])
        self.addCleanup(listener.close)
        self.experiment(detach=detach, notify=endpoint)
        if configure is not None:
            configure()
        process = self.cli('run', seed='1', wait=False)
        ready, _, _ = select.select([listener, process.stdout, process.stderr], [], [], COST['ready_seconds'])
        if listener not in ready:
            stdout, stderr = process.communicate(timeout=COST['command_seconds'])
            if ((process.returncode == 2 and b'containment' in stderr)
                    or (process.returncode == 4 and b'containment/provision: native provision refused:' in stderr)):
                self.fail('native provisioning refused: ' + stderr.decode('utf-8', 'replace'))
            self.fail('canary did not notify: ' + repr((stdout, stderr)))
        connection, _ = listener.accept()
        connection.settimeout(COST['ready_seconds'])
        self.addCleanup(connection.close)
        data = b''
        while not data.endswith(b'\n'):
            chunk = connection.recv(4096)
            self.assertTrue(chunk, 'private canary closed before its notification')
            data += chunk
        notification = json.loads(data)
        fds = [os.pidfd_open(pid) for pid in [notification['main'], notification['first'], *notification['descendants']]]
        self.canaries.extend(fds)
        return process, connection, notification, fds

    def assert_exited(self, fds):
        pending = set(fds)
        # A single shared readiness deadline, not one per descendant.
        import time
        end = time.monotonic() + COST['stop_seconds']
        while pending:
            ready, _, _ = select.select(list(pending), [], [], max(0, end - time.monotonic()))
            if not ready:
                self.fail('owned canary survived unit teardown')
            pending.difference_update(ready)

    def detached_cleanup(self, detach):
        process, connection, _note, fds = self.notified(detach)
        connection.sendall(b'G')
        _stdout, stderr = process.communicate(timeout=COST['command_seconds'])
        self.assertEqual(process.returncode, 1, stderr)
        self.assert_exited(fds)
        record = self.records()[2][0]
        self.assertIn('survivor/server', [f['signature'] for f in record['failures']])
        self.assert_empty(record)

    def test_setsid_survivors_are_killed(self):
        self.detached_cleanup('setsid')

    def test_doublefork_survivors_are_killed(self):
        self.detached_cleanup('doublefork')

    def test_outer_sigint_relays_to_unit_and_records_interruption(self):
        process, _connection, _note, fds = self.notified('doublefork')
        process.send_signal(signal.SIGINT)
        _stdout, stderr = process.communicate(timeout=COST['command_seconds'])
        self.assertEqual(process.returncode, 130, stderr)
        self.assert_exited(fds)
        record = self.records()[2][0]
        self.assertTrue(any(e['what'] == 'interrupted' for e in record['errors']))
        self.assert_empty(record)

    def test_runner_main_sigkill_leaves_no_detached_tree_and_retains_recovery_marker(self):
        process, _connection, note, fds = self.notified('doublefork')
        signal.pidfd_send_signal(fds[0], signal.SIGKILL)
        _stdout, stderr = process.communicate(timeout=COST['command_seconds'])
        self.assertEqual(process.returncode, 4, stderr)
        self.assert_exited(fds)
        runs = list((self.lab_root / 'keys/native').glob('*/runs/' + note['run']))
        self.assertEqual(len(runs), 1)
        self.assertTrue((runs[0] / 'live.json').is_file())
        self.assertFalse((runs[0] / 'run.json').exists())
        events = [canonical.loads(line + b'\n') for line in (runs[0] / 'events.jsonl').read_bytes().splitlines()]
        group = next(e['cgroup'] for e in events if e['event'] == 'containment')
        self.assertFalse(Path(group).exists())
        with store.open_lab(self.lab_root) as lab:
            with self.assertRaisesRegex(Exception, 'owner recovery required'):
                evidence.require_recovered(lab)

    def test_worker_keeps_writer_lock_after_outer_sigkill(self):
        process, _connection, note, fds = self.notified('none')
        process.kill()
        process.wait(timeout=COST['command_seconds'])
        with store.open_lab(self.lab_root) as lab:
            with self.assertRaisesRegex(Exception, 'another lab writer.*' + note['run']):
                with lab.lock():
                    self.fail('worker lost the transferred writer lock')
        signal.pidfd_send_signal(fds[0], signal.SIGINT)
        self.assert_exited(fds)
