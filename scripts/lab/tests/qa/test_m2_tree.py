"""Design-only QA: lab.md 4.3, 4.7 and 5.1 through the shipped CLI."""
import gzip
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[4]
LAB = ROOT / 'scripts/lab/lab'
PARTICIPANT = ROOT / 'scripts/lab/tests/support/participant.py'


def canonical(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(',', ':')).encode() + b'\n'


def archive(members, form=tarfile.USTAR_FORMAT):
    out = io.BytesIO()
    with tarfile.open(fileobj=out, mode='w', format=form) as packed:
        for name, kind, data, mode in members:
            item = tarfile.TarInfo(name)
            item.type, item.mode = kind, mode
            item.linkname = 'invented-target' if kind in (tarfile.SYMTYPE, tarfile.LNKTYPE) else ''
            item.size = len(data) if kind == tarfile.REGTYPE else 0
            packed.addfile(item, io.BytesIO(data) if item.size else None)
    return out.getvalue()


class RunnerCase(unittest.TestCase):
    def setUp(self):
        os.umask(0o077)
        # On disk, above the lab's floor (/tmp may be a small tmpfs), and short
        # enough for the runner's sockets beneath the invented home.
        self.temp = tempfile.TemporaryDirectory(prefix='m2-qa-', dir='/var/tmp')
        self.base = Path(self.temp.name)
        self.addCleanup(self.clean)
        self.home = self.base / 'home'
        self.store = self.home / 'src/rust/tmp/lab'
        self.store.mkdir(parents=True, mode=0o700)
        self.env = dict(os.environ, HOME=str(self.home))
        costs = dict(ready_seconds=4, command_seconds=2, stop_seconds=2,
                     driver_seconds=2, driver_actions=2, driver_input_tokens=2)
        (self.store / 'settings.json').write_bytes(canonical(dict(
            format='lab-settings/1', costs={'qa': costs}, driver=None, triager=None)))
        self.repo = self.base / 'source'
        self.repo.mkdir()
        (self.repo / 'invented.txt').write_text('invented subject\n')
        self.command(['git', 'init', '-q', self.repo])
        self.command(['git', '-C', self.repo, 'add', 'invented.txt'])
        self.command(['git', '-C', self.repo, '-c', 'user.name=QA', '-c',
                      'user.email=qa@example.invalid', 'commit', '-qm', 'Invented subject'])
        self.commit = self.command(['git', '-C', self.repo, 'rev-parse', 'HEAD']).stdout.strip().decode()
        self.experiment = self.base / 'experiment'
        self.experiment.mkdir()
        shutil.copyfile(PARTICIPANT, self.experiment / 'participant.py')
        self.exp = dict(format='lab-experiment/1', name='m2-qa', cost='qa', dirs=[],
                        processes=[], actions=[], driver=None, faults=[], planted=[], collect=[], checks=[])

    def clean(self):
        for parent, dirs, files in os.walk(self.base):
            os.chmod(parent, 0o700)
        self.temp.cleanup()

    def command(self, argv, timeout=90):
        return subprocess.run([str(x) for x in argv], env=self.env, cwd=self.base,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout)

    def subject(self, data=None):
        args = [LAB, 'subject', '--repository', self.repo, '--commit', self.commit]
        path = self.base / 'bundle.tar'
        path.write_bytes(archive([]) if data is None else data)
        args += ['--artifact', 'bundle=' + str(path)]
        result = self.command(args)
        self.assertEqual(result.returncode, 0, result.stderr.decode(errors='replace'))
        return result.stdout.strip().decode()

    def write(self):
        (self.experiment / 'experiment.json').write_bytes(canonical(self.exp))

    def validate(self, subject):
        self.write()
        return self.command([LAB, 'validate', self.experiment, '--subject', subject])

    def run_lab(self, subject):
        self.write()
        result = self.command([LAB, 'run', self.experiment, '--subject', subject, '--seed', '7'])
        records = list(self.store.glob('keys/*/*/runs/*/run.json'))
        if not records:
            diagnostic = result.stderr.decode(errors='replace')
            if any(word in diagnostic.lower() for word in ('systemd', 'bubblewrap', 'bwrap', 'namespace', 'cgroup', 'user manager', 'disk')):
                self.skipTest('Native containment/admission unavailable: ' + diagnostic.strip())
        self.assertEqual(len(records), 1, (result.returncode, result.stdout, result.stderr))
        path = records[0]
        return json.loads(path.read_bytes()), path.parent

    def service(self, files=None, mode='server', args=None):
        return dict(name='service', argv=['/usr/bin/python3', '-I', '-B', '-S',
                    '{experiment}/participant.py', mode, *(args or ['-', 'none', '{run.id}'])],
                    env={}, files=files or [], memory_bytes=None,
                    ready=[dict(stream='stdout', prefix='ready ', capture='address')])


class TreeQA(RunnerCase):
    def test_tree_preserves_bytes_and_readonly_modes(self):
        data = archive([('nested', tarfile.DIRTYPE, b'', 0o777),
                        ('nested/index', tarfile.REGTYPE, b'{unrendered}\n', 0o666),
                        ('run', tarfile.REGTYPE, b'invented executable', 0o777)])
        self.exp['processes'] = [self.service([dict(path='bundle', tree='bundle')],
            'tree-reader', ['{proc.service.dir}/bundle'])]
        record, _ = self.run_lab(self.subject(data))
        self.assertEqual(record['outcome'], 'pass', record)

    def test_invalid_archive_members_are_refused_before_run(self):
        directory = ('nested', tarfile.DIRTYPE, b'', 0o755)
        ordinary = ('nested/file', tarfile.REGTYPE, b'invented', 0o644)
        cases = [
            ('absolute', [('/escape', tarfile.REGTYPE, b'x', 0o644)]),
            ('parent', [('../escape', tarfile.REGTYPE, b'x', 0o644)]),
            ('dot', [('./file', tarfile.REGTYPE, b'x', 0o644)]),
            ('interior-parent', [('nested/../file', tarfile.REGTYPE, b'x', 0o644)]),
            ('duplicate', [directory, ordinary, ordinary]),
            ('missing-parent', [ordinary]),
            ('late-parent', [ordinary, directory]),
            ('symlink', [('link', tarfile.SYMTYPE, b'', 0o777)]),
            ('hardlink', [('link', tarfile.LNKTYPE, b'', 0o644)]),
            ('fifo', [('pipe', tarfile.FIFOTYPE, b'', 0o644)]),
        ]
        self.exp['processes'] = [self.service([dict(path='bundle', tree='bundle')])]
        for name, members in cases:
            with self.subTest(name=name):
                result = self.validate(self.subject(archive(members)))
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn(b'tree', result.stderr.lower(), result.stderr)
                self.assertFalse(list((self.store / 'runs').glob('*/run.json')))

    def test_non_ustar_formats_are_refused(self):
        members = [('file', tarfile.REGTYPE, b'invented', 0o644)]
        self.exp['processes'] = [self.service([dict(path='bundle', tree='bundle')])]
        for form in (tarfile.GNU_FORMAT, tarfile.PAX_FORMAT):
            with self.subTest(form=form):
                data = archive(members, form)
                # Force an extended header in the PAX case, rather than USTAR-compatible bytes.
                if form == tarfile.PAX_FORMAT:
                    data = archive([('x' * 110, tarfile.REGTYPE, b'invented', 0o644)], form)
                result = self.validate(self.subject(data))
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn(b'tree', result.stderr.lower(), result.stderr)

    def test_tree_and_text_destinations_cannot_overlap(self):
        (self.experiment / 'text').write_text('invented\n')
        subject = self.subject(archive([]))
        for tree, text in [('bundle', 'bundle/file'), ('bundle/nested', 'bundle'), ('bundle', 'bundle')]:
            with self.subTest(tree=tree, text=text):
                self.exp['processes'] = [self.service([
                    dict(path=tree, tree='bundle'), dict(path=text, **{'from': 'text'})])]
                result = self.validate(subject)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertRegex(result.stderr.decode(), r'(?i)(overlap|ancestor|beneath|above|duplicate)')

    def test_compressed_archive_is_refused(self):
        self.exp['processes'] = [self.service([dict(path='bundle', tree='bundle')])]
        result = self.validate(self.subject(gzip.compress(archive([]), mtime=0)))
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn(b'tree', result.stderr.lower(), result.stderr)

    def test_unknown_tree_artifact_is_refused(self):
        self.exp['processes'] = [self.service([dict(path='bundle', tree='absent')])]
        result = self.validate(self.subject())
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn(b'processes.0.files.0.tree', result.stderr, result.stderr)
