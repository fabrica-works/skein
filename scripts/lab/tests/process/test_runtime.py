"""Actual user units, spawns and cgroups. Capability denials are explicit skips."""
import json
import os
from pathlib import Path
import select
import shutil
import signal
import socket
import subprocess
import sys
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
            raise unittest.SkipTest('native process tier requires systemd user manager, bubblewrap and pidfds')
        try:
            manager(cls.systemctl, ['show', '--property=Version', '--value'], COST['command_seconds'])
            namespace_probe(bwrap, PYTHON, COST['command_seconds'])
        except (OSError, CapabilityError, subprocess.SubprocessError) as error:
            raise unittest.SkipTest('native capability refused: ' + str(error)) from error

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
            self.skipTest('native provisioning refused: ' + stderr.decode('utf-8', 'replace'))
        if process.returncode == 4 and b'containment/provision: native provision refused:' in stderr:
            self.skipTest('native provisioning refused: ' + stderr.decode('utf-8', 'replace'))
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

    def notified(self, detach):
        endpoint = str(self.root / 'notify')
        listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        listener.bind(endpoint)
        listener.listen(1)
        listener.settimeout(COST['ready_seconds'])
        self.addCleanup(listener.close)
        self.experiment(detach=detach, notify=endpoint)
        process = self.cli('run', seed='1', wait=False)
        ready, _, _ = select.select([listener, process.stdout, process.stderr], [], [], COST['ready_seconds'])
        if listener not in ready:
            stdout, stderr = process.communicate(timeout=COST['command_seconds'])
            if ((process.returncode == 2 and b'containment' in stderr)
                    or (process.returncode == 4 and b'containment/provision: native provision refused:' in stderr)):
                self.skipTest('native provisioning refused: ' + stderr.decode('utf-8', 'replace'))
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
