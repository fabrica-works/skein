import json
from pathlib import Path
import unittest
from runner import draws, judge
from runner.records import Refused


class Draws(unittest.TestCase):
    def test_fixed_vectors(self):
        vectors = json.loads((Path(__file__).parent / 'vectors/draws.json').read_text())
        for vector in vectors['draws']:
            self.assertEqual(draws.draw(vector['seed'], vector['label']), vector['draw'])
        for vector in vectors['orders']:
            self.assertEqual(draws.start_order(vector['seed'], vector['after']), vector['order'])
        for vector in vectors['faults']:
            self.assertEqual(draws.fault_n(vector['seed'], vector['index'], vector['low'], vector['high']), vector['n'])

    def test_refuses_invalid_graphs_and_ranges(self):
        for after in [{'a': ['b']}, {'a': ['a']}, {'a': ['b'], 'b': ['a']}, {'a': [False]}]:
            with self.assertRaises(Refused):
                draws.start_order('1', after)
        for bounds in [(0, 1), (2, 1), (True, 2)]:
            with self.assertRaises(Refused):
                draws.fault_n('1', 0, *bounds)
        self.assertEqual(draws.fault_n('18446744073709551615', 1, 2, 2), 2)


class Judge(unittest.TestCase):
    def test_verdict_vectors(self):
        vectors = json.loads((Path(__file__).parent / 'vectors/verdicts.json').read_text())
        for vector in vectors:
            with self.subTest(vector=vector):
                superseded = frozenset(r['run'] for r in vector['evidence']
                    if 'current' in vector and r['outcome'] == 'fail'
                    and judge.superseded(r['signatures'], vector['held_to'].get(r['run'], {}), vector['current']))
                self.assertEqual(judge.verdict(vector['evidence'], superseded), vector['verdict'])
                if 'superseded' in vector:
                    self.assertEqual(superseded, frozenset(vector['superseded']))
                    self.assertEqual(judge.next_seed(vector['evidence'], superseded), vector['next_seed'])
                    record = judge.verdict_record('key', vector['evidence'], superseded)
                    self.assertEqual(record['verdict'], vector['verdict'])
                    self.assertEqual(record['seeds'][0]['not_judged'], vector['superseded'])
                    self.assertEqual(record['seeds'][0]['judged'], [r['run'] for r in vector['evidence']
                        if r['seed'] == '1' and r['run'] not in superseded])
                    self.assertEqual(judge.verdict(vector['evidence']), 'flaky')

    def test_superseded_requires_every_signature_to_name_a_strictly_raised_class_bound(self):
        held = dict(ready_seconds=10, command_seconds=20)
        current = dict(ready_seconds=11, command_seconds=21)
        signatures = ['bound/ready_seconds', 'bound/command_seconds/check']
        self.assertTrue(judge.superseded(signatures, held, current))
        for invalid in ([], ['stop/server/exit:1'], ['bound/unknown'], ['bound/ready_seconds/'],
                        ['bound/ready_seconds/check/extra'], ['bound/ready_seconds', 'stop/server/exit:1']):
            self.assertFalse(judge.superseded(invalid, held, current))
        for values in ({}, dict(ready_seconds=11), dict(ready_seconds=11, command_seconds=20),
                       dict(ready_seconds=11, command_seconds=19)):
            self.assertFalse(judge.superseded(signatures, held, values))

    def test_seed_admission_and_additive_attempts(self):
        evidence = [{'seed': '1', 'outcome': 'error'}, {'seed': '1', 'outcome': 'pass'},
                    {'seed': '2', 'outcome': 'pass'}, {'seed': '3', 'outcome': 'pass'}]
        self.assertEqual(judge.verdict(evidence), 'pass')
        self.assertTrue(judge.seed_admitted(evidence, '1'))
        self.assertFalse(judge.seed_admitted(evidence, '4'))
        evidence.append({'seed': '1', 'outcome': 'fail'})
        self.assertEqual(judge.verdict(evidence), 'flaky')
        self.assertEqual(len(evidence), 5)
        with self.assertRaises(Refused):
            judge.verdict(evidence + [{'seed': '4', 'outcome': 'pass'}])
        for seed in ['01', 1, '-1']:
            with self.assertRaises(Refused):
                judge.seed_admitted(evidence, seed)

    def test_outcome_precedence_and_driver_context(self):
        self.assertEqual(judge.outcome(['failure'], ['error'], ['unknown'], None), 'fail')
        self.assertEqual(judge.outcome([], ['error'], ['unknown'], None), 'error')
        self.assertEqual(judge.outcome([], [], ['unknown'], {'done': True}), 'incomplete')
        self.assertEqual(judge.outcome([], [], [], None), 'incomplete')
        report = {'done': False, 'retried': [], 'unseen': [], 'surprises': []}
        self.assertEqual(judge.outcome([], [], [], report), 'fail')
        self.assertEqual(judge.outcome([], ['error'], [], report), 'fail')
        report['done'] = True
        self.assertEqual(judge.outcome([], [], [], report), 'pass')
        report['unseen'] = [{'step': 'step', 'looked_for': 'something'}]
        self.assertEqual(judge.outcome([], [], [], report), 'fail')
        self.assertEqual(judge.outcome([], [], [], {'done': True}), 'incomplete')
        self.assertEqual(judge.outcome([], [], [], None, driver_required=False), 'pass')
