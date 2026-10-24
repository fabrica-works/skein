"""A plain native run ignores stale browser settings."""
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import unittest
from runner import canonical, store
from runner.host import CapabilityError, await_removed, manager, namespace_probe

PYTHON = os.path.realpath(sys.executable)
PACKAGE = Path(__file__).resolve().parents[2]
COST = dict(ready_seconds=60, command_seconds=120, stop_seconds=10,
            driver_seconds=900, driver_actions=80, driver_input_tokens=2500000)


class BrowserIgnored(unittest.TestCase):
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
        scratch = Path.home() / 'src/rust/tmp/lab'
        scratch.mkdir(mode=0o700, parents=True, exist_ok=True)
        if scratch.stat().st_mode & 0o077:
            self.fail('native scratch tmp/lab is not private')
        self.temp = tempfile.TemporaryDirectory(prefix='browser-ignored-', dir=scratch)
        self.root = Path(self.temp.name)
        self.lab_root = self.root / 'lab'
        missing = self.root / 'missing-browser'
        browser = {'program': str(missing / 'program'),
                   'chromium': {'path': str(missing / 'chromium'), 'sha256': 'sha256:' + 'a' * 64, 'version': '1'},
                   'fonts': {'path': str(missing / 'fonts'), 'tree': 'sha256:' + 'b' * 64}}
        with store.open_lab(self.lab_root) as lab:
            lab.write_once('settings.json', canonical.record_bytes(dict(format='lab-settings/1', costs={'smoke': COST},
                                                                        driver=None, triager=None, browser=browser)))
            subject = dict(format='lab-subject/1', commit='a' * 40, source=canonical.tree_digest([]), artifacts={})
            self.subject = canonical.digest(subject)
            lab.capture('subjects', self.subject,
                        [dict(path='subject.json', data=canonical.record_bytes(subject), exec=False)], empty_dirs=('source',))
        self.directory = self.root / 'experiment'
        self.directory.mkdir(mode=0o700)
        (self.directory / 'ready.py').write_text("import signal, sys\nsignal.signal(signal.SIGTERM, lambda s, f: sys.exit(0))\nprint('ready', flush=True)\nsignal.pause()\n")
        exp = dict(format='lab-experiment/1', name='browser-ignored', cost='smoke', dirs=[],
                   processes=[dict(name='ready', argv=[PYTHON, '-I', '-B', '-S', '{experiment}/ready.py'], env={},
                                   ready=[dict(stream='stdout', prefix='ready', capture=None)], memory_bytes=None)],
                   actions=[], driver=None, faults=[], planted=[], collect=[], checks=[])
        (self.directory / 'experiment.json').write_bytes(canonical.record_bytes(exp))
        self.children = []

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
        for live in self.lab_root.glob('keys/browser-ignored/*/runs/*/live.json'):
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
        for directory, _, _ in os.walk(self.root):
            os.chmod(directory, 0o700)
        self.temp.cleanup()

    def cli(self, command, *extra):
        bootstrap = 'import sys; sys.path.insert(0,sys.argv.pop(1)); from runner.cli import main; sys.exit(main(sys.argv[2:],lab_root=sys.argv[1]))'
        args = [PYTHON, '-I', '-B', '-S', '-c', bootstrap, str(PACKAGE), str(self.lab_root), command,
                str(self.directory), '--subject', self.subject, *extra]
        process = subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.children.append(process)
        stdout, stderr = process.communicate(timeout=COST['command_seconds'])
        if process.returncode == 2 and any(part in stderr for part in (b'containment', b'bubblewrap')):
            self.skipTest('native provisioning refused: ' + stderr.decode('utf-8', 'replace'))
        if process.returncode == 4 and b'containment/provision: native provision refused:' in stderr:
            self.skipTest('native provisioning refused: ' + stderr.decode('utf-8', 'replace'))
        return process.returncode, stdout.decode().strip(), stderr.decode('utf-8', 'replace')

    def test_plain_validate_and_run_ignore_missing_browser_paths_and_wrong_digests(self):
        status, identity, errors = self.cli('validate')
        self.assertEqual(status, 0, errors)
        key_path = 'keys/browser-ignored/' + identity
        with store.open_lab(self.lab_root) as lab:
            key = canonical.loads(lab.read(key_path + '/key.json'))
        self.assertNotIn('browser', key['lab'])
        status, run_id, errors = self.cli('run', '--seed', '1')
        self.assertEqual(status, 0, errors)
        base = self.lab_root / key_path / 'runs' / run_id
        record = canonical.loads((base / 'run.json').read_bytes())
        self.assertEqual(record['outcome'], 'pass')
        self.assertEqual(list(record['processes']), ['ready'])
        self.assertFalse((base / 'live.json').exists())
        events = [canonical.loads(line + b'\n') for line in (base / 'events.jsonl').read_bytes().splitlines()]
        group = next(e['cgroup'] for e in events if e['event'] == 'containment')
        self.assertFalse(Path(group).exists())
