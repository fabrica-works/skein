"""Pure vectors and structural validation for the hand-run squares proof."""
from contextlib import redirect_stdout
import importlib.machinery
import importlib.util
import io
import json
from pathlib import Path
import unittest
from unittest.mock import patch
from runner import canonical, experiment
from .fixtures import D, fixture

PROOF = Path(__file__).resolve().parents[1] / 'proof'


def load(name, path):
    loader = importlib.machinery.SourceFileLoader(name, str(path))
    spec = importlib.util.spec_from_loader(name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


ledger = load('proof_ledger', PROOF / 'subjects/ledger/ledger')
answer = load('proof_answer', PROOF / 'squares/checks/answer.py')
PROGRAM = dict(verb='program', bytes=1)
ACCEPTED = dict(verb='answer', value=1, verdict='accepted')
WRONG = dict(verb='answer', value=2, verdict='wrong')


class Squares(unittest.TestCase):
    def test_seed_vectors(self):
        for seed, values, total in [
            ('0', [89, 30, 5, 49, 47, 85, 89, 88, 52, 95, 65, 89], 48873),
            ('18446744073709551615', [74, 97, 39, 5, 5, 97, 93, 71, 38, 7, 87, 18], 41697),
        ]:
            with self.subTest(seed=seed):
                self.assertEqual(ledger.numbers(seed), values)
                self.assertEqual(ledger.expected(values), total)

    def test_seed_refuses_noncanonical_or_out_of_range(self):
        for seed in ('', '01', '-1', '+1', '18446744073709551616', '1\n'):
            with self.subTest(seed=seed), self.assertRaises(ValueError):
                ledger.numbers(seed)

    def test_check_passes_nonempty_program_and_last_accepted_answer(self):
        self.assertEqual(answer.judge(dict(calls=[WRONG, PROGRAM, ACCEPTED])), (0, []))

    def test_check_failure_signatures(self):
        for calls, signatures in [
            ([dict(verb='program', bytes=0), ACCEPTED], ['program/missing']),
            ([PROGRAM], ['answer/missing', 'answer/wrong']),
            ([PROGRAM, ACCEPTED, WRONG], ['answer/wrong']),
            ([], ['program/missing', 'answer/missing', 'answer/wrong']),
        ]:
            with self.subTest(calls=calls):
                status, lines = answer.judge(dict(calls=calls))
                self.assertEqual(status, 1)
                self.assertEqual([line['fail'] for line in lines], signatures)
                self.assertTrue(all(type(line['summary']) is str and line['summary'] for line in lines))

    def test_unreadable_observations_are_undecided(self):
        expected = (2, [dict(undecided='observations unreadable')])
        self.assertEqual(answer.judge(None), expected)
        with patch.object(answer.Path, 'read_text', side_effect=OSError('unreadable')):
            output = io.StringIO()
            with redirect_stdout(output):
                status = answer.main(['observations.json'])
        self.assertEqual((status, [json.loads(output.getvalue())]), expected)

    def test_client_refuses_arguments_before_connecting(self):
        for args in [
            ['127.0.0.1:1234', 'task', 'extra'],
            ['127.0.0.1:1234', 'program', 'extra'],
            ['127.0.0.1:1234', 'answer', '1', 'extra'],
            ['127.0.0.1:1234', 'observations'],
            ['127.0.0.1:1234', 'unknown'],
            ['127.0.0.1:1234', 'answer', 'one'],
            ['127.0.0.1:1234', 'answer'],
            ['192.0.2.1:1234', 'task'],
            ['localhost:1234', 'task'],
            ['127.0.0.1:0', 'task'],
            ['127.0.0.1:65536', 'task'],
        ]:
            with self.subTest(args=args), self.assertRaises(ValueError):
                ledger.parse_client(args)

    def test_client_parses_only_fixed_endpoint_and_supported_verbs(self):
        for args, expected in [
            (['127.0.0.1:1', 'task'], (('127.0.0.1', 1), 'task', None)),
            (['127.0.0.1:65535', 'program'], (('127.0.0.1', 65535), 'program', None)),
            (['127.0.0.1:1234', 'answer', '-12'], (('127.0.0.1', 1234), 'answer', -12)),
        ]:
            with self.subTest(args=args):
                self.assertEqual(ledger.parse_client(args), expected)

    def test_landed_experiment_is_canonical_and_structurally_valid(self):
        directory = PROOF / 'squares'
        raw = (directory / 'experiment.json').read_bytes()
        exp = canonical.loads(raw)
        self.assertEqual(raw, canonical.record_bytes(exp))
        _, subject, settings, facts, _ = fixture()
        subject['artifacts'] = dict(ledger=dict(sha256=D, bytes=7, exec=True))
        settings['costs'] = dict(squares=settings['costs']['smoke'])
        settings['driver'] = dict(program=dict(path='/usr/bin/codex', sha256=D, version='0.162.1'),
                                  model='scripted', effort='low')
        facts.update(driver_host=True, artifacts_available=['ledger'],
                     known=dict(experiment='/immutable/experiment', **{'artifact.ledger': '/immutable/artifacts/ledger'}),
                     experiment_files={p.relative_to(directory).as_posix(): p.read_text()
                                       for p in directory.rglob('*') if p.is_file()},
                     executables={exp['processes'][0]['argv'][0]: dict(regular=True, exec=True, sha256=D)})
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])
