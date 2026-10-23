"""Finding histories over supplied judged records, times and ancestry facts."""
import json
from pathlib import Path
import unittest
from runner import canonical, findings
from runner.records import CLASSES, Refused, finding_refusals, fix_refusals
from .test_runtime_store import run_record
from .fixtures import D

T = '2026-10-20T12:34:56.001Z'
P = 'sha256:' + 'b' * 64
Q = 'sha256:' + 'c' * 64
SIGNATURE = 'panic/app/src/lib.rs:7:2'
OWNER = 'owner'


def run(number, seed='1', outcome='fail', attempt=1, *, key=D, signature=SIGNATURE, kind='panic'):
    value = run_record(number, seed, outcome, attempt)
    value['key'] = key
    if outcome == 'fail':
        value['failures'] = [dict(id='failure-1', signature=signature, kind=kind, summary='failed', evidence=[])]
    return value


class Lifecycle(unittest.TestCase):
    def setUp(self):
        self.records = {D: [run(1), run(2, '2'), run(3, '3'), run(4, attempt=2)],
                        Q: [run(5, str(i), 'pass', key=Q) for i in (1, 2, 3)]}
        # Each run identity is unique within a key.
        for i, value in enumerate(self.records[Q], 5):
            value['run'] = run_record(i, value['seed'])['run']
        self.history = [findings.event('candidate', T, 'runner', signature=SIGNATURE, kind='panic', summary='failed',
                                       **findings.reference(self.records[D][0], self.records[D][0]['failures'][0]))]
        self.facts = {}

    def state(self):
        return findings.fold(self.history, self.records, self.facts)

    def add(self, name, by=OWNER, **fields):
        value = findings.event(name, T, by, **fields)
        state = findings.fold(self.history + [value], self.records, self.facts)
        self.history.append(value)
        return state

    def confirmed(self, finding_class='experiment'):
        self.add('classed', **{'class': finding_class})
        self.add('filed', **findings.filing(self.state(), self.records))
        self.add('taken')

    def fixed(self, finding_class='experiment'):
        self.confirmed(finding_class)
        fix = ({'experiment': P} if finding_class == 'experiment' else
               {'runner_commit': 'a' * 40, 'tests': ['unit.finding']} if finding_class == 'lab' else
               {'repository': '/repository', 'commit': 'a' * 40, 'expectations': ['page'], 'product_commit': 'b' * 40} if finding_class == 'look' else
               {'repository': '/repository', 'commit': 'a' * 40, 'tests': ['world::replays'], 'product_commit': 'b' * 40})
        self.add('fixed', fix=fix)
        return fix

    def recheck(self, fix, **fields):
        self.facts[(canonical.digest(fix), Q)] = True
        return self.add('rechecked', key=Q, verdict=findings.snapshot(Q, self.records[Q]), **fields)

    def test_full_lifecycle_and_canonical_event_round_trip(self):
        fix = self.fixed()
        self.assertEqual(self.recheck(fix)['state'], 'rechecked')
        self.assertEqual(self.add('closed')['state'], 'closed')
        for value in self.history:
            self.assertEqual(canonical.loads(canonical.record_bytes(value)), value)
        derived = findings.index({'F-0001': self.history}, self.records, self.facts)['findings'][0]
        self.assertEqual(derived['state'], 'closed')
        self.assertEqual(derived['reproduction']['numerator'], 1)
        self.assertEqual(derived['reproduction']['denominator'], 1)

    def test_language_neutral_reproduction_vectors(self):
        vectors = json.loads((Path(__file__).parent / 'vectors/findings.json').read_text())
        for vector in vectors:
            records = {D: self.records[D][:3] + [run(i + 4, outcome=outcome, attempt=i + 2)
                                               for i, outcome in enumerate(vector['reruns'])]}
            state = findings.fold(self.history, records)
            rate = findings.reproduction(state, records)
            self.assertEqual(rate['numerator'], vector['numerator'])
            self.assertEqual(rate['denominator'], vector['denominator'])
            self.assertEqual(findings.filing(state, records)['flaky'], vector['flaky'])

    def test_words_lifecycle_vector_requires_product_fix_and_recheck(self):
        vectors = json.loads((Path(__file__).parent / 'vectors/findings.json').read_text())
        for vector in vectors:
            if vector.get('class') != 'words':
                continue
            states = [self.add('classed', **{'class': vector['class']})['state']]
            states.append(self.add('filed', **findings.filing(self.state(), self.records))['state'])
            states.append(self.add('taken')['state'])
            states.append(self.add('fixed', fix=vector['fix'])['state'])
            with self.assertRaises(Refused):
                self.add('closed')
            with self.assertRaisesRegex(Refused, 'carriage'):
                self.add('rechecked', key=Q, verdict=findings.snapshot(Q, self.records[Q]))
            states.append(self.recheck(vector['fix'])['state'])
            states.append(self.add('closed')['state'])
            self.assertEqual(states, vector['states'])

    def test_proposal_allows_filing_but_only_owner_confirms_and_fix_needs_confirmation(self):
        self.add('proposed', by='triager', **{'class': 'experiment', 'title': 'Proposal', 'reason': 'evidence'})
        self.assertIsNone(self.state()['class'])
        self.add('filed', **findings.filing(self.state(), self.records))
        self.add('taken')
        with self.assertRaisesRegex(Refused, 'confirmed class'):
            self.add('fixed', fix={'experiment': P})
        with self.assertRaisesRegex(Refused, 'owner action'):
            self.add('classed', by='triager', **{'class': 'experiment'})
        self.add('classed', **{'class': 'experiment'})
        self.add('fixed', fix={'experiment': P})

    def test_no_class_or_early_transition_is_invented(self):
        for name, fields in [('filed', findings.filing(self.state(), self.records)), ('taken', {}),
                             ('fixed', {'fix': {'experiment': P}}), ('closed', {})]:
            with self.subTest(name=name), self.assertRaises(Refused):
                self.add(name, **fields)
        self.assertIsNone(self.state()['proposed'])
        self.assertIsNone(self.state()['class'])
        with self.assertRaises(Refused):
            self.add('proposed', **{'class': 'world', 'title': 'title', 'reason': 'reason'})

    def test_two_failing_seeds_and_rerun_cannot_file_before_three_seeds(self):
        self.records[D] = [self.records[D][0], self.records[D][1], self.records[D][3]]
        self.add('classed', **{'class': 'world'})
        with self.assertRaisesRegex(Refused, 'exactly three'):
            findings.filing(self.state(), self.records)

    def test_first_seed_rerun_required_even_with_three_failures(self):
        self.records[D] = self.records[D][:3]
        with self.assertRaisesRegex(Refused, 'first failing seed'):
            findings.filing(self.state(), self.records)

    def test_nonreproducing_and_unjudged_reruns_keep_outcome_and_explicit_rate(self):
        self.records[D][3] = run(4, outcome='pass', attempt=2)
        self.records[D].extend([run(8, outcome='error', attempt=3), run(9, outcome='incomplete', attempt=4),
                                run(10, attempt=5)])
        self.add('classed', **{'class': 'experiment'})
        filed = findings.filing(self.state(), self.records)
        self.assertTrue(filed['flaky'])
        self.assertEqual([v['outcome'] for v in filed['reruns']], ['pass', 'error', 'incomplete', 'fail'])
        self.add('filed', **filed)
        rate = findings.reproduction(self.state(), self.records)
        self.assertEqual((rate['numerator'], rate['denominator']), (1, 4))
        self.assertEqual(rate['reruns'][1]['outcome'], 'error')

    def test_error_only_rerun_satisfies_attempt_requirement_without_claiming_pass(self):
        self.records[D][3] = run(4, outcome='error', attempt=2)
        filed = findings.filing(self.state(), self.records)
        self.assertEqual(filed['reruns'], [dict(run=self.records[D][3]['run'], outcome='error', reproduced=False)])
        self.assertFalse(filed['flaky'])
        self.assertEqual(findings.reproduction(self.state(), self.records)['denominator'], 1)

    def test_every_failing_seed_counts_but_attempts_before_first_failure_do_not(self):
        self.records[D] = [run(8, outcome='error'), run(1, attempt=2), run(2, '2'), run(3, '3'),
                           run(4, attempt=3), run(9, '2', 'incomplete', 2)]
        rate = findings.reproduction(self.state(), self.records)
        self.assertEqual((rate['numerator'], rate['denominator']), (1, 2))

    def test_exact_classes_and_each_fix_field_required(self):
        self.assertEqual(CLASSES, ('world', 'fake', 'below', 'tools', 'experiment', 'lab', 'look', 'words'))
        for finding_class in CLASSES:
            fix = ({'experiment': P} if finding_class == 'experiment' else
                   {'runner_commit': 'a' * 40, 'tests': ['own::test']} if finding_class == 'lab' else
                   {'repository': '/repo', 'commit': 'a' * 40, 'expectations': ['page'], 'product_commit': 'b' * 40} if finding_class == 'look' else
                   {'repository': '/repo', 'commit': 'a' * 40, 'tests': ['lowest::test'], 'product_commit': 'b' * 40})
            self.assertFalse(fix_refusals(fix, finding_class))
            for field in fix:
                with self.subTest(finding_class=finding_class, missing=field):
                    self.assertTrue(fix_refusals({k: v for k, v in fix.items() if k != field}, finding_class))
            self.assertTrue(fix_refusals({**fix, 'untrusted': True}, finding_class))
        for tests in ([], [''], ['same', 'same'], 'test'):
            self.assertTrue(fix_refusals({'runner_commit': 'a' * 40, 'tests': tests}, 'lab'))

    def test_look_fix_expectations_and_verified_carriage(self):
        fix = self.fixed('look')
        self.assertEqual(self.state()['class'], 'look')
        for expectations in ([], [''], ['same', 'same'], 'page', [{}]):
            self.assertTrue(fix_refusals({**fix, 'expectations': expectations}, 'look'))
        for field, item in (('repository', 'relative'), ('commit', 'short'), ('product_commit', None), ('tests', ['world'])):
            self.assertTrue(fix_refusals({**fix, field: item}, 'look'))
        with self.assertRaisesRegex(Refused, 'carriage'):
            self.add('rechecked', key=Q, verdict=findings.snapshot(Q, self.records[Q]))
        self.recheck(fix)
        self.add('closed')
        self.assertEqual(self.state()['state'], 'closed')

    def test_close_refuses_missing_false_or_unavailable_ancestry_and_same_key(self):
        fix = self.fixed('below')
        for proof in (None, False, 'true'):
            self.facts[(canonical.digest(fix), Q)] = proof
            with self.subTest(proof=proof), self.assertRaises(Refused):
                self.add('rechecked', key=Q, verdict=findings.snapshot(Q, self.records[Q]))
        self.facts[(canonical.digest(fix), D)] = True
        with self.assertRaises(Refused):
            self.add('rechecked', key=D, verdict=findings.snapshot(D, self.records[D]))
        self.assertEqual(self.recheck(fix)['state'], 'rechecked')

    def test_recheck_refuses_short_failed_flaky_and_judged_signature_evidence(self):
        fix = self.fixed()
        self.facts[(canonical.digest(fix), Q)] = True
        passing = list(self.records[Q])
        variants = [passing[:2], [*passing, run(12, outcome='fail', attempt=2, key=Q)],
                    [run(12, str(i), key=Q) for i in (1, 2, 3)]]
        for records in variants:
            self.records[Q] = records
            with self.subTest(records=records), self.assertRaises(Refused):
                self.add('rechecked', key=Q, verdict=findings.snapshot(Q, records))
        # Even a caller's pass outcome cannot hide a signature in a judged run.
        corrupt = [dict(r, failures=self.records[D][0]['failures']) for r in passing]
        self.records[Q] = corrupt
        with self.assertRaises(Refused):
            self.add('rechecked', key=Q, verdict=findings.snapshot(Q, corrupt))
        self.records[Q] = passing
        forged = dict(findings.snapshot(Q, passing), key=P)
        with self.assertRaises(Refused):
            self.add('rechecked', key=Q, verdict=forged)

    def test_historical_pass_stays_valid_when_later_recurrence_reopens(self):
        fix = self.fixed()
        self.recheck(fix)
        self.add('closed')
        self.facts[(canonical.digest(fix), D)] = False
        old = run(11, attempt=3)
        self.records[D].append(old)
        self.add('occurred', by='runner', **findings.reference(old, old['failures'][0]))
        self.assertEqual(self.state()['state'], 'closed')
        recurrence = run(12, attempt=2, key=Q)
        self.records[Q].append(recurrence)
        self.add('reopened', by='runner', **findings.reference(recurrence, recurrence['failures'][0]))
        self.assertEqual(self.state()['state'], 'open')

    def test_recurrence_matching_old_P_stays_closed_corrected_P_reopens(self):
        fix = self.fixed()
        self.recheck(fix)
        self.add('closed')
        self.facts[(canonical.digest(fix), D)] = False
        old, corrected = run(11, attempt=3), run(12, key=Q, attempt=2)
        self.records[D].append(old)
        self.records[Q].append(corrected)
        additions = findings.reconcile({'F-0001': self.history}, [old, corrected], self.records, T, self.facts)
        self.assertEqual([v['event'] for _, v in additions], ['occurred', 'reopened'])
        with self.assertRaisesRegex(Refused, 'must reopen'):
            self.add('occurred', by='runner', **findings.reference(corrected, corrected['failures'][0]))
        self.facts[(canonical.digest(fix), Q)] = False
        with self.assertRaisesRegex(Refused, 'does not carry'):
            self.add('reopened', by='runner', **findings.reference(corrected, corrected['failures'][0]))

    def test_lab_owner_judges_recheck_and_reopening_no_automatic_inference(self):
        fix = self.fixed('lab')
        self.facts[(canonical.digest(fix), Q)] = True
        with self.assertRaisesRegex(Refused, 'owner judgment'):
            self.add('rechecked', key=Q, verdict=findings.snapshot(Q, self.records[Q]))
        self.recheck(fix, judgment='runner commit and tests checked by owner')
        self.add('closed')
        recurrence = run(12, key=Q, attempt=2)
        self.records[Q].append(recurrence)
        additions = findings.reconcile({'F-0001': self.history}, [recurrence], self.records, T, self.facts)
        self.assertEqual(additions[0][1]['event'], 'occurred')
        self.history.extend(value for _identity, value in additions)
        occurrence_count = len(self.state()['occurrences'])
        with self.assertRaises(Refused):
            self.add('reopened', by='runner', judgment='guess', **findings.reference(recurrence, recurrence['failures'][0]))
        self.assertEqual(self.add('reopened', judgment='owner verified runner carries fix',
                                  **findings.reference(recurrence, recurrence['failures'][0]))['state'], 'open')
        self.assertEqual(len(self.state()['occurrences']), occurrence_count)

    def test_equality_only_and_use_awaits_triage(self):
        other = run(12, key=Q, signature=SIGNATURE + '/other')
        use = run(13, key=Q, signature='use/goal', kind='use')
        self.records[Q].extend([other, use])
        additions = findings.reconcile({}, [self.records[D][0], other, use], self.records, T)
        self.assertEqual([identity for identity, _ in additions], ['F-0001', 'F-0002'])
        histories = {identity: [v] for identity, v in additions}
        self.assertEqual(findings.reconcile(histories, [self.records[D][0], other, use], self.records, T), [])
        candidate = findings.event('candidate', T, 'runner', signature='use/goal', kind='use', summary='failed',
                                   **findings.reference(use, use['failures'][0]))
        with self.assertRaisesRegex(Refused, 'triager proposal'):
            findings.fold([candidate], self.records)
        candidate['by'] = 'triager'
        proposed = findings.event('proposed', T, 'triager', **{'class': 'tools', 'title': 'proposal', 'reason': 'evidence'})
        self.assertEqual(findings.fold([candidate, proposed], self.records)['proposed'], 'tools')

    def test_duplicate_occurrence_wrong_reference_and_corrupt_snapshots_refuse(self):
        first = self.history[0]
        with self.assertRaisesRegex(Refused, 'duplicate'):
            self.add('occurred', by='runner', **{k: first[k] for k in ('key', 'run', 'seed', 'failure')})
        for field, value in [('seed', '9'), ('failure', 'missing'), ('signature', 'another'), ('summary', 'different')]:
            with self.subTest(field=field), self.assertRaises(Refused):
                findings.fold([{**first, field: value}], self.records)
        self.confirmed()
        with self.assertRaises(Refused):
            findings.fold(self.history[1:], self.records)
        forged = [dict(v) for v in self.history]
        forged[2] = {**forged[2], 'reruns': []}
        with self.assertRaises(Refused):
            findings.fold(forged, self.records)

    def test_record_validation_calendar_unknown_members_and_no_owner_impersonation(self):
        valid = self.history[0]
        for changed in ({'format': 'lab-finding/9'}, {'event': 'unknown'}, {'by': ''},
                        {'t': '2026-02-30T00:00:00.000Z'}, {'t': '2026-10-20T24:00:00.000Z'},
                        {'extra': True}, {'judgment': 'not this event'}):
            self.assertTrue(finding_refusals({**valid, **changed}), changed)
        for finding_class in ('product', None, 3, []):
            self.assertTrue(finding_refusals(dict(format='lab-finding/1', event='classed', t=T, by=OWNER,
                                                 **{'class': finding_class})))
