"""Current bounds select evidence without erasing failed ownership releases."""
import json

from .test_m2_tree import LAB
from .test_real_inputs import RealCase


class VerdictQA(RealCase):
    def test_raised_bound_preserves_key_and_requires_explicit_attempt(self):
        # lab.md 8: "failures are all bounds ... now above ... no longer judged";
        # "a seed whose runs are none of them judged ... runs nothing".
        self.meter('3')
        subject = self.subject()
        record, run, result = self.execute(subject)
        self.assertEqual(result.returncode, 1, record)
        original = (run / 'run.json').read_bytes()
        evidence = run.parents[1] / 'evidence.jsonl'
        earlier_evidence = evidence.read_bytes()
        self.settings['costs']['qa']['subject_spend'] = 4
        self.save_settings()
        judged = self.command([LAB, 'judge', self.experiment, '--subject', subject])
        self.assertEqual(judged.returncode, 3, judged.stderr)
        self.assertIn(b'not enough evidence', judged.stdout)
        self.assertEqual(evidence.read_bytes(), earlier_evidence)
        verdict = json.loads((run.parents[1] / 'verdict.json').read_bytes())
        self.assertIn(record['run'], json.dumps(verdict['seeds']))
        self.assertEqual((run / 'run.json').read_bytes(), original)
        rerun, newer, result = self.execute(subject)
        self.assertEqual(result.returncode, 0, rerun)
        self.assertEqual(rerun['key'], record['key'])
        self.assertEqual(rerun['attempt'], 2)
        self.assertNotEqual(newer, run)
        self.assertEqual(rerun['bounds']['subject_spend'], 4)
        self.assertEqual(record['bounds']['subject_spend'], 2)
        self.assertTrue(evidence.read_bytes().startswith(earlier_evidence))
        final = self.command([LAB, 'judge', self.experiment, '--subject', subject], timeout=25)
        self.assertEqual(final.returncode, 0, final.stderr)
        self.assertEqual(final.stdout.strip(), b'pass')

    def test_equal_bound_keeps_failures_judged(self):
        # lab.md 8: supersession needs a class value "now above" the recorded one.
        self.meter('3')
        subject = self.subject()
        record, _, result = self.execute(subject)
        self.assertEqual(result.returncode, 1, record)
        judged = self.command([LAB, 'judge', self.experiment, '--subject', subject], timeout=25)
        self.assertEqual(judged.returncode, 1, judged.stderr)
        self.assertEqual(judged.stdout.strip(), b'fail')
        self.assertEqual(len(list(self.store.glob('keys/*/*/runs/*/run.json'))), 3)

    def test_words_class_records_text_fix_evidence(self):
        # lab.md 9.3: "For words, as for world: the test that holds the text,
        # the repository and commit of its fix, and ... commit that carries it".
        self.exp['actions'] = [self.command_spec('bad', '/usr/bin/false')]
        subject = self.subject()
        for seed in [7, 8, 9, 7]:
            record, _, result = self.execute(subject, seed)
            self.assertEqual(result.returncode, 1, record)
        finding = next((self.store / 'findings').glob('F-*.jsonl'))
        identifier = finding.stem
        self.checked([LAB, 'finding', identifier, 'class', 'words', '--by', 'qa-owner'])
        self.checked([LAB, 'finding', identifier, 'file', '--by', 'qa-owner'])
        self.checked([LAB, 'finding', identifier, 'take', '--by', 'qa-owner'])
        self.checked([LAB, 'finding', identifier, 'fix', '--repository', self.repo,
            '--commit', self.commit, '--test', 'text/refusal', '--product-commit', self.commit,
            '--by', 'qa-owner'])
        events = [json.loads(line) for line in finding.read_text().splitlines()]
        self.assertEqual(events[-1]['event'], 'fixed')
        self.assertIn('text/refusal', json.dumps(events[-1]))
        self.assertIn(self.commit, json.dumps(events[-1]))
        # A product-class close needs independently verified carriage, per README.
        refused = self.command([LAB, 'finding', identifier, 'close', '--recheck', record['key'],
                                '--by', 'qa-owner'])
        self.assertEqual(refused.returncode, 2, refused.stderr)
