from contextlib import redirect_stdout, redirect_stderr
import io
import json
import os
from pathlib import Path
import stat
import tempfile
import unittest
from unittest.mock import patch
from runner import canonical, cli, store
from runner.records import Refused
from .fixtures import D, fixture


class Filesystem(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.lab = store.open_lab(self.root / 'lab')

    def tearDown(self):
        self.lab.close()
        # Immutable trees must become removable when the temporary world ends.
        for directory, _, _ in os.walk(self.root):
            os.chmod(directory, 0o700)
        self.temp.cleanup()

    def test_private_root_and_no_symlink_components(self):
        public = self.root / 'public'
        public.mkdir(mode=0o755)
        public.chmod(0o755)
        with self.assertRaises(Refused):
            store.open_lab(public)
        link = self.root / 'link'
        link.symlink_to(self.root / 'lab', target_is_directory=True)
        with self.assertRaises(OSError):
            store.open_lab(link)
        (self.root / 'lab' / 'bad').symlink_to(self.root, target_is_directory=True)
        with self.assertRaises(OSError):
            self.lab.write_once('bad/escape', b'bad')
        self.assertFalse((self.root / 'escape').exists())

    def test_lock_is_exclusive_and_no_symlink_lock(self):
        with self.lab.lock():
            with self.assertRaises(Refused):
                with self.lab.lock():
                    self.fail('second writer admitted')
        (self.root / 'lab' / 'lock').unlink()
        target = self.root / 'target'
        target.write_text('unchanged')
        (self.root / 'lab' / 'lock').symlink_to(target)
        with self.assertRaises(OSError):
            with self.lab.lock():
                self.fail('symlink lock admitted')
        self.assertEqual(target.read_text(), 'unchanged')

    def test_walk_refuses_links_and_special_files(self):
        source = self.root / 'inputs'
        source.mkdir()
        (source / 'file').write_bytes(b'x')
        (source / 'link').symlink_to(source / 'file')
        with self.assertRaises(Refused):
            store.walk(source)
        (source / 'link').unlink()
        os.mkfifo(source / 'fifo')
        with self.assertRaises(Refused):
            store.walk(source)
        os.unlink(source / 'fifo')
        os.link(source / 'file', source / 'hardlink')
        with self.assertRaises(Refused):
            store.walk(source)

    def test_write_once_collision_and_append(self):
        with self.lab.lock():
            self.lab.write_once('inputs/file', b'one')
            self.lab.write_once('inputs/file', b'one')
            with self.assertRaises(Refused):
                self.lab.write_once('inputs/file', b'two')
            with self.assertRaises(Refused):
                self.lab.write_once('../escape', b'bad')
            for value in [1, 2]:
                self.lab.append_line('lines.jsonl', {'format': 'lab-tree/1', 'value': value})
        self.assertEqual(self.lab.read('inputs/file'), b'one')
        self.assertEqual(stat.S_IMODE((self.root / 'lab/inputs/file').stat().st_mode), 0o400)
        self.assertEqual([json.loads(line)['value'] for line in self.lab.read('lines.jsonl').splitlines()], [1, 2])
        target = self.root / 'target'
        target.write_bytes(b'safe')
        (self.root / 'lab/link').symlink_to(target)
        for effect in [lambda: self.lab.write_once('link', b'bad'),
                       lambda: self.lab.append_line('link', {'format': 'lab-tree/1'})]:
            with self.assertRaises((OSError, Refused)):
                effect()
        self.assertEqual(target.read_bytes(), b'safe')

    def test_atomic_capture_preserves_exec_and_verifies_collision(self):
        files = [{'path': 'bin/app', 'data': b'executable', 'exec': True},
                 {'path': 'config', 'data': b'config', 'exec': False}]
        identity = canonical.tree_digest(store.entries(files))
        with self.lab.lock():
            target = self.lab.capture('experiments', identity, files)
            self.lab.capture('experiments', identity, list(reversed(files)))
            with self.assertRaises(Refused):
                self.lab.capture('experiments', identity, [{**files[0], 'data': b'changed'}, files[1]])
        self.assertEqual(stat.S_IMODE(target.stat().st_mode), 0o500)
        self.assertEqual(stat.S_IMODE((target / 'bin/app').stat().st_mode), 0o500)
        self.assertEqual(stat.S_IMODE((target / 'config').stat().st_mode), 0o400)
        self.assertEqual(store.walk(target), store.entries(files))
        self.assertFalse(any(p.name.startswith('.input-') for p in target.parent.iterdir()))

    def test_subject_roundtrip_integrity_and_provenance(self):
        artifact = self.root / 'app'
        artifact.write_bytes(b'program')
        artifact.chmod(0o700)
        source = [{'path': 'code', 'data': b'source', 'exec': False}]
        with patch.object(store, 'git_source', return_value=('a' * 40, source)), self.lab.lock():
            identity = store.capture_subject(self.lab, '/repo', 'HEAD', [], [('app', str(artifact))])
            again = store.capture_subject(self.lab, '/other', 'HEAD', ['irrelevant'], [('app', str(artifact))])
        self.assertEqual(identity, again)
        subject, path = cli._subject(self.lab, identity)
        self.assertEqual(subject['about']['repository'], '/repo')
        self.assertTrue(subject['artifacts']['app']['exec'])
        self.assertEqual((path / 'source/code').read_bytes(), b'source')
        (path / 'artifacts/app').chmod(0o700)
        (path / 'artifacts/app').write_bytes(b'changed')
        (path / 'artifacts/app').chmod(0o500)
        with self.assertRaises(Refused):
            cli._subject(self.lab, identity)

    def test_empty_source_is_materialized(self):
        artifact = self.root / 'app'
        artifact.write_bytes(b'app')
        with patch.object(store, 'git_source', return_value=('a' * 40, [])), self.lab.lock():
            identity = store.capture_subject(self.lab, '/repo', 'HEAD', [], [('app', str(artifact))])
            again = store.capture_subject(self.lab, '/repo', 'HEAD', [], [('app', str(artifact))])
        self.assertEqual(identity, again)
        _, path = cli._subject(self.lab, identity)
        self.assertTrue((path / 'source').is_dir())
        self.assertEqual(stat.S_IMODE((path / 'source').stat().st_mode), 0o500)

    def test_cli_subject_validate_identity_and_multiple_refusals(self):
        exp, _, settings, _, _ = fixture()
        directory = self.root / 'experiment'
        directory.mkdir()
        (directory / 'app.conf').write_text('endpoint={proc.backend.ready.address}')
        (directory / 'check').write_bytes(b'#!/bin/sh\nexit 0\n')
        (directory / 'check').chmod(0o700)
        # The system executable story uses a local file, with two declared helpers.
        helper = self.root / 'helper'
        helper.write_bytes(b'helper')
        helper.chmod(0o700)
        other_helper = self.root / 'other-helper'
        other_helper.write_bytes(b'other helper')
        other_helper.chmod(0o700)
        # No machine executables or source trees are read in this temporary world.
        for patcher in [patch.object(cli.sys, 'executable', str(helper)),
                        patch.object(cli.shutil, 'which', return_value=None),
                        patch.object(cli, '_runner_digest', return_value=D)]:
            patcher.start()
            self.addCleanup(patcher.stop)
        exp['actions'][0]['argv'][0] = str(helper)
        exp['checks'].append({'name': 'helper-check', 'argv': [str(other_helper)], 'env': {}})
        (directory / 'experiment.json').write_bytes(canonical.record_bytes(exp))
        self.lab.write_once('settings.json', canonical.record_bytes(settings))
        artifact = self.root / 'app'
        artifact.write_bytes(b'app')
        artifact.chmod(0o700)
        output, errors = io.StringIO(), io.StringIO()
        with patch.object(store, 'git_source', return_value=('a' * 40, [{'path': 'code', 'data': b'code', 'exec': False}])), redirect_stdout(output), redirect_stderr(errors):
            code = cli.main(['subject', '--repository', '/repo', '--commit', 'HEAD', '--artifact', 'app=' + str(artifact)], lab_root=self.root / 'lab')
        self.assertEqual(code, 0, errors.getvalue())
        subject_id = output.getvalue().strip()
        facts = {name: True for name in ('private', 'lock_available', 'disk_ok', 'containment', 'bubblewrap')}
        output, errors = io.StringIO(), io.StringIO()
        with patch.object(cli, '_machine_facts', return_value=facts), redirect_stdout(output), redirect_stderr(errors):
            code = cli.main(['validate', str(directory), '--subject', subject_id], lab_root=self.root / 'lab')
        self.assertEqual(code, 0, errors.getvalue())
        identity = output.getvalue().strip()
        value = canonical.loads(self.lab.read(f'keys/hello/{identity}/key.json'))
        programs = value['lab']['programs']
        self.assertIn(str(helper), [p['path'] for p in programs])
        self.assertIn(str(other_helper), [p['path'] for p in programs])
        other_helper.write_bytes(b'changed')
        with patch.object(cli, '_machine_facts', return_value=facts), self.lab.lock():
            changed, refusals = cli.validate_command(self.lab, directory, subject_id)
        self.assertEqual(refusals, [])
        self.assertNotEqual(identity, changed)
        exp.update(name='BAD', surprise=True)
        exp['processes'][0]['files'] = [{'path': '../escape', 'from': 'missing'}]
        (directory / 'experiment.json').write_bytes(canonical.record_bytes(exp))
        output, errors = io.StringIO(), io.StringIO()
        with patch.object(cli, '_machine_facts', return_value=facts), redirect_stdout(output), redirect_stderr(errors):
            code = cli.main(['validate', str(directory), '--subject', subject_id], lab_root=self.root / 'lab')
        self.assertEqual(code, 2)
        self.assertEqual(output.getvalue(), '')
        self.assertIn('experiment.name', errors.getvalue())
        self.assertIn('experiment.surprise', errors.getvalue())
        self.assertIn('processes.0.files.0.path', errors.getvalue())
        self.assertIn('processes.0.files.0.from', errors.getvalue())
        with redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as error:
            cli.main(['run'], lab_root=self.root / 'lab')
        self.assertEqual(error.exception.code, 2)

    def test_cli_supplies_artifact_tree_bytes_and_refuses_bad_archives_without_run(self):
        from .test_tree import GOOD, archive
        exp, _, settings, facts, _ = fixture()
        directory = self.root / 'experiment'
        directory.mkdir()
        helper = self.root / 'helper'
        helper.write_bytes(b'helper')
        helper.chmod(0o700)
        exp['processes'] = [dict(name='server', argv=[str(helper)], env={},
                                  ready=[dict(stream='stdout', prefix='ready ', capture=None)], memory_bytes=None,
                                  files=[dict(path='web', tree='bundle')])]
        exp.update(actions=[], collect=[], checks=[])
        (directory / 'experiment.json').write_bytes(canonical.record_bytes(exp))
        self.lab.write_once('settings.json', canonical.record_bytes(settings))
        artifact = self.root / 'bundle'
        with patch.object(store, 'git_source', return_value=('a' * 40, [])), \
             patch.object(cli, '_machine_facts', return_value=facts), \
             patch.object(cli, '_runner_digest', return_value=D), \
             patch.object(cli.sys, 'executable', str(helper)), \
             patch.object(cli.shutil, 'which', return_value=None):
            for data, expected in ((archive(GOOD), 0), (b'invalid archive', 2)):
                artifact.write_bytes(data)
                with self.lab.lock():
                    subject = store.capture_subject(self.lab, '/invented-repository', 'HEAD', [], [('bundle', artifact)])
                output, errors = io.StringIO(), io.StringIO()
                with redirect_stdout(output), redirect_stderr(errors):
                    code = cli.main(['validate', str(directory), '--subject', subject], lab_root=self.lab.root)
                self.assertEqual(code, expected, errors.getvalue())
                if code == 2:
                    self.assertIn('processes.0.files.0.tree', errors.getvalue())
                    self.assertEqual(output.getvalue(), '')
        self.assertEqual(list(self.lab.root.glob('keys/*/*/runs')), [])
