"""Real private files and additive publication, with no process or clock."""
from contextlib import redirect_stderr, redirect_stdout
import io
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from runner import canonical, cli, evidence, finding_store, store
from runner.records import Refused
from .fixtures import fixture
from .test_findings import T, SIGNATURE, run


class Files(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.lab = store.open_lab(Path(self.temp.name) / 'lab')
        self.lock = self.lab.lock()
        self.lock.__enter__()
        self.subject = dict(format='lab-subject/1', commit='a' * 40, source=canonical.tree_digest([]), artifacts={})
        self.subject_id = canonical.digest(self.subject)
        self.lab.capture('subjects', self.subject_id,
                         [dict(path='subject.json', data=canonical.record_bytes(self.subject), exec=False)], empty_dirs=('source',))
        self.original, self.original_exp = self.key('original')
        self.corrected, self.corrected_exp = self.key('corrected')

    def tearDown(self):
        self.lock.__exit__(None, None, None)
        self.lab.close()
        for directory, _, _ in os.walk(self.temp.name):
            os.chmod(directory, 0o700)
        self.temp.cleanup()

    def key(self, label):
        exp = dict(format='lab-experiment/1', name='sample', cost='smoke', dirs=[], processes=[], actions=[],
                   driver=None, faults=[], planted=[], collect=[], checks=[])
        files = [dict(path='experiment.json', data=canonical.record_bytes(exp), exec=False),
                 dict(path='guide', data=label.encode(), exec=False)]
        exp_id = canonical.tree_digest(store.entries(files))
        self.lab.capture('experiments', exp_id, files)
        value = dict(format='lab-key/1', experiment=exp_id, subject=self.subject_id, lab=fixture()[4])
        identity = canonical.digest(value)
        self.lab.write_once('keys/sample/' + identity + '/key.json', canonical.record_bytes(value))
        return identity, exp_id

    def publish(self, number, seed='1', outcome='fail', attempt=1, *, key=None):
        key = self.original if key is None else key
        value = run(number, seed, outcome, attempt, key=key)
        evidence.publish(self.lab, 'keys/sample/' + key, value)
        return value

    def act(self, action, **fields):
        return finding_store.act(self.lab, 'F-0001', action, T, 'owner', **fields)

    def ready(self, finding_class='experiment'):
        for n in (1, 2, 3):
            self.publish(n, str(n))
        self.publish(4, attempt=2)
        self.act('class', finding_class=finding_class)
        self.act('file')
        self.act('take')

    def fixed(self):
        self.ready()
        self.act('fix', fix={'experiment': self.corrected_exp})
        for n in (1, 2, 3):
            self.publish(10 + n, str(n), 'pass', key=self.corrected)

    def events(self):
        return finding_store.histories(self.lab)['F-0001']

    def test_end_to_end_experiment_close_old_occurrence_and_corrected_recurrence(self):
        self.fixed()
        result = self.act('close', recheck=self.corrected)
        self.assertEqual(result['findings'][0]['state'], 'closed')
        self.publish(20, attempt=3)
        self.assertEqual(self.events()[-1]['event'], 'occurred')
        result = finding_store.sync(self.lab, T)
        self.assertEqual(result['findings'][0]['state'], 'closed')
        self.publish(21, attempt=2, key=self.corrected)
        self.assertEqual(self.events()[-1]['event'], 'reopened')
        self.assertEqual(finding_store.sync(self.lab, T)['findings'][0]['state'], 'open')
        before = self.lab.read('findings/F-0001.jsonl')
        finding_store.sync(self.lab, T)
        self.assertEqual(self.lab.read('findings/F-0001.jsonl'), before)

    def test_repeated_publication_and_sync_never_duplicate_occurrences(self):
        value = self.publish(1)
        path = 'keys/sample/' + self.original
        before = self.lab.read(path + '/evidence.jsonl')
        history = self.lab.read('findings/F-0001.jsonl')
        evidence.publish(self.lab, path, value)
        finding_store.sync(self.lab, T)
        self.assertEqual(before, self.lab.read(path + '/evidence.jsonl'))
        self.assertEqual(history, self.lab.read('findings/F-0001.jsonl'))
        self.publish(2, attempt=2)
        self.assertEqual([v['event'] for v in self.events()], ['candidate', 'occurred'])

    def test_publication_failure_retains_live_marker_and_reentry_reconciles_evidence(self):
        path = 'keys/sample/' + self.original
        value = run(1, key=self.original)
        live = path + '/runs/' + value['run'] + '/live.json'
        self.lab.write_once(live, canonical.record_bytes(dict(format='lab-live/1', unit='lab-owned.service')))
        append = self.lab.append_line
        def cut(path, record):
            if path.startswith('findings/'):
                raise OSError('publication cut before finding append')
            return append(path, record)
        with patch.object(self.lab, 'append_line', side_effect=cut), self.assertRaises(OSError):
            evidence.publish(self.lab, path, value)
        with self.assertRaisesRegex(Refused, 'owner recovery required'):
            evidence.require_recovered(self.lab)
        self.assertTrue(self.lab.read(live))
        evidence.publish(self.lab, path, value)
        self.assertEqual(len(self.events()), 1)
        self.assertEqual(len(evidence.read(self.lab, path)), 1)
        # This is explicit owner reconciliation; ordinary commands keep refusing
        # the live marker until its unit and cleanup have been verified.
        with self.assertRaisesRegex(Refused, 'owner recovery required'):
            evidence.require_recovered(self.lab)

    def test_index_failure_after_append_rebuilds_without_duplicate_event(self):
        replace = self.lab.replace_derived
        def cut(path, value):
            if path == 'findings/index.json':
                raise OSError('index publication cut')
            return replace(path, value)
        with patch.object(self.lab, 'replace_derived', side_effect=cut), self.assertRaises(OSError):
            self.publish(1)
        before = self.lab.read('findings/F-0001.jsonl')
        derived = finding_store.sync(self.lab, T)
        self.assertEqual(derived['findings'][0]['state'], 'candidate')
        self.assertEqual(before, self.lab.read('findings/F-0001.jsonl'))

    def test_close_cut_after_verified_recheck_can_finish_once(self):
        self.fixed()
        append = self.lab.append_line
        def cut(path, value):
            if value.get('event') == 'closed':
                raise OSError('close cut after recheck')
            return append(path, value)
        with patch.object(self.lab, 'append_line', side_effect=cut), self.assertRaises(OSError):
            self.act('close', recheck=self.corrected)
        self.assertEqual(self.events()[-1]['event'], 'rechecked')
        self.act('close', recheck=self.corrected)
        self.assertEqual([v['event'] for v in self.events()].count('rechecked'), 1)
        self.assertEqual(self.events()[-1]['event'], 'closed')

    def test_candidate_partial_append_is_refused_without_rewrite_or_new_id(self):
        self.publish(1)
        path = self.lab.root / 'findings/F-0001.jsonl'
        with path.open('ab') as stream:
            stream.write(b'{')
        before = path.read_bytes()
        with self.assertRaises(Refused):
            finding_store.sync(self.lab, T)
        self.assertEqual(path.read_bytes(), before)
        self.assertFalse((self.lab.root / 'findings/F-0002.jsonl').exists())

    def test_corrupt_index_is_ignored_and_source_corruption_refuses(self):
        self.publish(1)
        path = self.lab.root / 'findings/index.json'
        path.write_bytes(b'untrusted cached state')
        derived = finding_store.sync(self.lab, T)
        self.assertEqual(canonical.loads(path.read_bytes()), derived)
        original = self.lab.read('findings/F-0001.jsonl')
        event = self.events()[0]
        event['seed'] = '9'
        (self.lab.root / 'findings/F-0001.jsonl').write_bytes(canonical.record_bytes(event))
        with self.assertRaises(Refused):
            finding_store.sync(self.lab, T)
        self.assertNotEqual(original, self.lab.read('findings/F-0001.jsonl'))

    def test_missing_run_and_unpublished_run_are_recovery_refusals(self):
        value = self.publish(1)
        path = 'keys/sample/' + self.original + '/runs/' + value['run'] + '/run.json'
        os.unlink(self.lab.root / path)
        with self.assertRaises(FileNotFoundError):
            finding_store.sync(self.lab, T)
        self.lab.write_once(path, canonical.record_bytes(value))
        orphan = run(2, attempt=2, key=self.original)
        self.lab.write_once('keys/sample/' + self.original + '/runs/' + orphan['run'] + '/run.json', canonical.record_bytes(orphan))
        with self.assertRaisesRegex(Refused, 'owner recovery required'):
            evidence.require_recovered(self.lab)

    def test_close_ignores_cached_pass_and_refuses_two_seeds_old_key_and_wrong_experiment(self):
        self.ready()
        self.act('fix', fix={'experiment': self.corrected_exp})
        self.publish(11, '1', 'pass', key=self.corrected)
        self.publish(12, '2', 'pass', key=self.corrected)
        self.lab.replace_derived('keys/sample/' + self.corrected + '/verdict.json',
                                 dict(format='lab-verdict/1', verdict='pass'))
        for key in (self.corrected, self.original, 'sha256:' + 'f' * 64):
            with self.subTest(key=key), self.assertRaises((Refused, FileNotFoundError)):
                self.act('close', recheck=key)
        unrelated, _exp = self.key('unrelated')
        for n in (1, 2, 3):
            self.publish(20 + n, str(n), 'pass', key=unrelated)
        with self.assertRaisesRegex(Refused, 'does not carry'):
            self.act('close', recheck=unrelated)
        self.assertEqual(self.events()[-1]['event'], 'fixed')

    def test_close_refuses_tampered_key_and_experiment_or_missing_subject(self):
        self.fixed()
        key_path = self.lab.root / 'keys/sample' / self.corrected / 'key.json'
        before = key_path.read_bytes()
        os.chmod(key_path, 0o600)
        key_path.write_bytes(canonical.record_bytes(dict(format='lab-key/1')))
        with self.assertRaises(Refused):
            self.act('close', recheck=self.corrected)
        key_path.write_bytes(before)
        os.chmod(key_path, 0o400)
        exp_path = self.lab.root / 'store/experiments' / self.corrected_exp / 'guide'
        os.chmod(exp_path, 0o600)
        with self.assertRaises(Refused):
            self.act('close', recheck=self.corrected)
        os.chmod(exp_path, 0o400)
        subject_path = self.lab.root / 'store/subjects' / self.subject_id / 'subject.json'
        os.chmod(subject_path.parent, 0o700)
        os.unlink(subject_path)
        with self.assertRaises((Refused, FileNotFoundError)):
            self.act('close', recheck=self.corrected)

    def test_product_proof_unavailable_explicitly_refused_without_events(self):
        self.ready('below')
        self.act('fix', fix=dict(repository='/untrusted/repository', commit='a' * 40,
                                 tests=['lowest::story'], product_commit='b' * 40))
        for n in (1, 2, 3):
            self.publish(10 + n, str(n), 'pass', key=self.corrected)
        before = self.lab.read('findings/F-0001.jsonl')
        with self.assertRaisesRegex(Refused, 'Git ancestry.*provenance is not proof'):
            self.act('close', recheck=self.corrected)
        self.assertEqual(before, self.lab.read('findings/F-0001.jsonl'))

    def test_lab_proof_requires_runner_mapping_and_owner_judgment(self):
        self.ready('lab')
        self.act('fix', fix=dict(runner_commit='a' * 40, tests=['unit::finding']))
        for n in (1, 2, 3):
            self.publish(10 + n, str(n), 'pass', key=self.corrected)
        before = self.lab.read('findings/F-0001.jsonl')
        with self.assertRaisesRegex(Refused, 'key.lab.runner.*explicit owner judgment'):
            self.act('close', recheck=self.corrected)
        self.assertEqual(before, self.lab.read('findings/F-0001.jsonl'))

    def test_unchanged_experiment_is_not_a_fix(self):
        self.ready()
        with self.assertRaisesRegex(Refused, 'must differ'):
            self.act('fix', fix={'experiment': self.original_exp})
        self.assertEqual(self.events()[-1]['event'], 'taken')

    def test_owner_cli_paths_with_supplied_time(self):
        for n in (1, 2, 3):
            self.publish(n, str(n))
        self.publish(4, attempt=2)
        self.lock.__exit__(None, None, None)
        try:
            commands = [['class', 'experiment'], ['file'], ['take', '--by', 'person'],
                        ['fix', '--experiment', self.corrected_exp]]
            for command in commands:
                with redirect_stdout(io.StringIO()) as output, redirect_stderr(io.StringIO()) as error:
                    status = cli.main(['finding', 'F-0001', *command], lab_root=self.lab.root, finding_time=T)
                self.assertEqual(status, 0, error.getvalue())
                self.assertIn('F-0001', output.getvalue())
            with self.lab.lock():
                for n in (1, 2, 3):
                    self.publish(10 + n, str(n), 'pass', key=self.corrected)
            with redirect_stdout(io.StringIO()) as output, redirect_stderr(io.StringIO()) as error:
                status = cli.main(['finding', 'F-0001', 'close', '--recheck', self.corrected],
                                  lab_root=self.lab.root, finding_time=T)
            self.assertEqual(status, 0, error.getvalue())
            self.assertEqual(output.getvalue().strip(), 'F-0001 closed')
        finally:
            self.lock = self.lab.lock()
            self.lock.__enter__()

    def test_index_replace_is_exact_and_refuses_symlink(self):
        os.close(self.lab.directory('findings', create=True))
        self.lab.replace_derived('findings/index.json', dict(format='lab-finding-index/1', findings=[]))
        for target in ('elsewhere/index.json', 'findings/other.json', 'findings/F-0001.jsonl', 'index.json'):
            with self.subTest(target=target), self.assertRaises(Refused):
                self.lab.replace_derived(target, {})
        path = self.lab.root / 'findings/index.json'
        path.unlink()
        protected = self.lab.root / 'protected'
        protected.write_bytes(b'protected')
        path.symlink_to(protected)
        with self.assertRaises(Refused):
            finding_store.sync(self.lab, T)
        self.assertEqual(protected.read_bytes(), b'protected')

    def test_nonprivate_history_and_scratch_evidence_refuse(self):
        self.publish(1)
        path = self.lab.root / 'findings/F-0001.jsonl'
        self.assertEqual(path.stat().st_mode & 0o777, 0o600)
        path.chmod(0o644)
        with self.assertRaisesRegex(Refused, 'private'):
            finding_store.sync(self.lab, T)
        path.chmod(0o600)
        value = run(2, attempt=2, key=self.original)
        value['failures'][0]['evidence'] = ['scratch/app/log']
        with self.assertRaisesRegex(Refused, 'scratch'):
            evidence.publish(self.lab, 'keys/sample/' + self.original, value)
