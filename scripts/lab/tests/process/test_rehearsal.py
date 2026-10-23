"""The whole drive on the Responses endpoint is kept outside judged evidence."""
from contextlib import redirect_stderr, redirect_stdout
import io
import json
import time
import unittest
from runner import canonical, cli, store
from . import test_proof_squares as proof


class Rehearsal(unittest.TestCase):
    setUpClass = classmethod(proof.Squares.setUpClass.__func__)
    setUp = proof.Squares.setUp
    tearDown = proof.Squares.tearDown

    def run(self, result=None):
        started = time.monotonic()
        try:
            return super().run(result)
        finally:
            print(f'{self.id()}: {time.monotonic() - started:.3f} s', flush=True)

    def invoke(self, mode, *extra):
        script = proof.PROOF / 'squares-rehearsal' / (mode + '.json')
        stdout, stderr = io.StringIO(), io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            status = cli.main(['run', str(self.directory), '--subject', self.subject, '--seed', '1',
                               '--rehearse', str(script), *extra], lab_root=self.lab_root)
        return status, stdout.getvalue().strip(), stderr.getvalue()

    def story(self, mode, expected):
        # A file where the paid home would be proves that the rehearsal never opens it.
        paid_home = self.root / 'paid-home'
        paid_home.write_bytes(b'private-home-sentinel')
        self.settings['codex_home'] = str(paid_home)
        self.settings['brake'] = dict(argv=['/usr/bin/false'], env={})
        self.settings['costs']['squares']['driver_input_tokens'] = 1000
        settings = self.lab_root / 'settings.json'
        settings.chmod(0o600)
        settings.write_bytes(canonical.record_bytes(self.settings))
        (self.lab_root / 'pause').write_text('paused')
        status, run_id, stderr = self.invoke(mode)
        self.assertEqual(status, expected, stderr)
        root = self.lab_root / 'rehearsals/squares' / run_id
        record = canonical.loads((root / 'run.json').read_bytes())
        self.assertEqual(record['rehearsal'], dict(script=store.file_digest(
            (proof.PROOF / 'squares-rehearsal' / (mode + '.json')).read_bytes())))
        self.assertEqual(canonical.digest(canonical.loads((root / 'key.json').read_bytes())), record['key'])
        key = self.lab_root / 'keys/squares' / record['key']
        self.assertFalse((key / 'runs').exists())
        self.assertFalse((key / 'evidence.jsonl').exists())
        self.assertFalse((key / 'verdict.json').exists())
        self.assertEqual(list((self.lab_root / 'findings').glob('F-*')), [])
        self.assertEqual(list((self.lab_root / 'campaigns').glob('*')), [])
        self.assertEqual(paid_home.read_bytes(), b'private-home-sentinel')
        self.assertEqual(record['errors'], [])
        self.assertFalse((root / 'live.json').exists())
        self.assertEqual(record['usage']['driver']['actions'], 4)
        self.assertEqual(record['usage']['driver']['tokens'], dict(input=500, cached=0, output=50))
        self.assertEqual(record['driver']['invocation']['env']['CODEX_HOME'], str(root / 'driver/rehearsal/home'))
        observations = json.loads((root / 'collected/observations/observations.json').read_bytes())
        command = next(a for a in record['driver']['actions'] if a['kind'] == 'commandExecution')
        self.assertEqual(command['end']['aggregatedOutput'], str(observations['expected']) + '\n')
        for directory in ('driver/provision', 'driver/rehearsal'):
            self.assertTrue(list((root / directory / 'requests').glob('[0-9]*.json')))
            for path in (root / directory / 'requests').glob('*.json'):
                self.assertEqual(path.stat().st_mode & 0o777, 0o600)
        return record

    def test_correct_rehearsal_passes_while_paused_and_keeps_no_evidence(self):
        self.assertEqual(self.story('correct', 0)['outcome'], 'pass')

    def test_wrong_rehearsal_fails_the_answer_check_and_keeps_no_evidence(self):
        record = self.story('wrong', 1)
        self.assertIn('check/answer/answer/wrong', [f['signature'] for f in record['failures']])

    def test_campaign_and_malformed_script_are_refused_before_a_run(self):
        with redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as raised:
            self.invoke('correct', '--campaign', 'invented')
        self.assertEqual(raised.exception.code, 2)
        malformed = self.root / 'malformed.json'
        malformed.write_text('{"format":"lab-script/1","turns":[{}]}\n')
        stderr = io.StringIO()
        with redirect_stderr(stderr), redirect_stdout(io.StringIO()):
            status = cli.main(['run', str(self.directory), '--subject', self.subject,
                               '--rehearse', str(malformed)], lab_root=self.lab_root)
        self.assertEqual(status, 2, stderr.getvalue())
        self.assertFalse((self.lab_root / 'rehearsals').exists())
        self.assertFalse(list((self.lab_root / 'keys').glob('*/*/runs/*')))
