"""Triage contracts over supplied runs and histories; no model, clock or child."""
import json
import os
from pathlib import Path
import tomllib
import unittest
from runner import canonical, codex, experiment, finding_store, findings, store, triage, triager, views
from runner.records import CLASSES, Refused, SAFE, cost_refusals, triage_refusals
from .fixtures import fixture, D
from .test_codex import inventory, message
from .test_findings import T, run
from . import test_finding_store


def inputs():
    first, second = run(1), run(2, attempt=2)
    second['failures'].extend([
        dict(id='new', signature='check/new', kind='check', summary='new failure', evidence=[]),
        dict(id='use', signature='use/untriaged', kind='use', summary='goal missed', evidence=[])])
    records = {D: [first, second]}
    history = {}
    for identity, value in findings.reconcile(history, records[D], records, T):
        history.setdefault(identity, []).append(value)
    return second, history, records


def answer(identity, match=None, signature=None, finding_class='tools'):
    return dict(id=identity, match=match, signature=signature, title='A proposal', reason='Retained evidence',
                **{'class': finding_class})


class Decisions(unittest.TestCase):
    def setUp(self):
        self.run, self.history, self.records = inputs()
        self.supplied = triage.input_value(self.run, self.history, self.records)
        self.answers = dict(failures=[answer('new'), answer('use', signature='use/goal')])

    def test_settings_admit_triager_and_name_each_bad_member(self):
        exp, subject, settings, facts, _ = fixture()
        configured = dict(program=dict(path='/codex', sha256=D, version='1'), model='m', effort='low',
                          catalog=dict(path='/catalog.json', sha256=D))
        settings['triager'] = configured
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])
        for field, bad in (('program', {}), ('model', ''), ('effort', None), ('catalog', {}), ('extra', True)):
            settings['triager'] = {**configured, field: bad}
            members = [r.member for r in experiment.validate(exp, subject, settings, facts)]
            self.assertTrue(any(m.startswith('settings.triager.' + field) for m in members), members)
        settings['triager'] = None
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])

    def test_triage_class_values_are_optional_positive_safe_integers(self):
        cost = fixture()[2]['costs']['smoke']
        self.assertEqual(cost_refusals(cost, 'cost'), [])
        for name in ('triage_seconds', 'triage_tokens'):
            self.assertEqual(cost_refusals({**cost, name: SAFE}, 'cost'), [])
            for value in (0, -1, SAFE + 1, True, '1', None):
                self.assertEqual([r.member for r in cost_refusals({**cost, name: value}, 'cost')], ['cost.' + name])

    def test_selection_is_use_and_candidates_created_by_this_run(self):
        self.assertEqual([f['id'] for f in self.supplied['failures']], ['new', 'use'])
        self.assertEqual(self.supplied['classes'], list(CLASSES))
        self.assertIsNone(self.supplied['report'])
        self.assertEqual(self.supplied['findings'], findings.index(self.history, self.records)['findings'])
        with self.assertRaisesRegex(Refused, 'judged failing'):
            triage.input_value(dict(self.run, outcome='error'), self.history, self.records)
        matched = dict(self.run, failures=self.run['failures'][:1])
        with self.assertRaisesRegex(Refused, 'no use failures or candidates'):
            triage.input_value(matched, self.history, self.records)

    def test_closed_schema_each_answer_once_and_contextual_constraints(self):
        self.assertEqual(triage.output_value(json.dumps(self.answers), self.supplied), self.answers)
        vectors = ['not json', '{}', dict(failures=[]), dict(failures=self.answers['failures'][:1]),
                   dict(failures=self.answers['failures'] * 2), dict(failures=[answer('extra')]),
                   {**self.answers, 'extra': 1}, dict(failures=[{**answer('new'), 'extra': 1}, self.answers['failures'][1]])]
        for text in vectors:
            with self.subTest(text=text), self.assertRaises(Refused):
                triage.output_value(text if type(text) is str else json.dumps(text), self.supplied)
        for field in self.answers['failures'][0]:
            changed = [{k: v for k, v in self.answers['failures'][0].items() if k != field}, self.answers['failures'][1]]
            with self.subTest(missing=field), self.assertRaises(Refused):
                triage.output_value(json.dumps(dict(failures=changed)), self.supplied)
        for changes in (dict(signature='not-use'), dict(signature='use/with space'), dict(signature='use/é'),
                        dict(signature='use/'), dict(match='F-9999'), {'class': 'unknown'}, dict(reason=' ')):
            rows = [self.answers['failures'][0], {**self.answers['failures'][1], **changes}]
            with self.subTest(changes=changes), self.assertRaises(Refused):
                triage.output_value(json.dumps(dict(failures=rows)), self.supplied)
        with self.assertRaises(Refused):
            triage.output_value('{"failures":[],"failures":[]}', self.supplied)

    def test_fold_proposes_on_candidate_and_creates_use_candidate_without_changing_run(self):
        before = canonical.dumps(self.records)
        additions = triage.additions(self.supplied, self.answers, self.history, self.records, T)
        self.assertEqual([(i, v['event']) for i, v in additions],
                         [('F-0002', 'proposed'), ('F-0003', 'candidate'), ('F-0003', 'proposed')])
        self.assertTrue(all(v['by'] == 'triager' for _, v in additions))
        for identity, value in additions:
            self.history.setdefault(identity, []).append(value)
        derived = findings.index(self.history, self.records)['findings']
        self.assertEqual(derived[-1]['signature'], 'use/goal')
        self.assertEqual(derived[-1]['proposed'], 'tools')
        self.assertIsNone(derived[-1]['class'])
        self.assertEqual(canonical.dumps(self.records), before)

    def test_use_match_occurs_nonuse_match_keeps_candidate_and_reason(self):
        output = dict(failures=[answer('new', match='F-0001'),
                               answer('use', match='F-0001', finding_class=None)])
        additions = triage.additions(self.supplied, output, self.history, self.records, T)
        self.assertEqual([(i, v['event']) for i, v in additions], [('F-0002', 'proposed'), ('F-0001', 'occurred')])
        self.assertIn('Suggested match: F-0001', additions[0][1]['reason'])
        self.assertNotIn('match', additions[0][1])
        for identity, value in additions:
            self.history[identity].append(value)
        self.assertEqual(len(findings.fold(self.history['F-0001'], self.records)['occurrences']), 3)
        forged = dict(additions[1][1], by='runner')
        with self.assertRaises(Refused):
            findings.fold(self.history['F-0001'][:-1] + [forged], self.records)

    def test_entire_batch_refuses_duplicate_proposed_signature_before_publication(self):
        output = dict(failures=[answer('new'), answer('use', signature='use/goal')])
        use = dict(self.run['failures'][-1], id='another-use')
        self.run['failures'].append(use)
        supplied = triage.input_value(self.run, self.history, self.records)
        output['failures'].append(answer('another-use', signature='use/goal'))
        with self.assertRaisesRegex(Refused, 'same signature'):
            triage.additions(supplied, output, self.history, self.records, T)

    def test_profile_has_only_evidence_read_work_write_minimal_and_host(self):
        profile = views.profile_toml('/codex', '/triage/evidence', '/triage/work', name='triage')
        value = tomllib.loads(profile)
        self.assertEqual(value['default_permissions'], 'triage')
        self.assertEqual(value['permissions'], dict(triage=dict(filesystem={
            ':minimal': 'read', '/codex': 'read', '/triage/evidence': 'read', '/triage/work': 'write'},
            network=dict(enabled=False))))
        config = tomllib.loads(codex.config_toml('', '', 'r', [], 'm', 'low', profile, mcp=False))
        self.assertNotIn('mcp_servers', config)
        self.assertEqual(config['model'], 'm')
        self.assertFalse(config['allow_login_shell'])

    def test_inventory_requires_no_servers_and_audit_no_mcp_or_other_raw_tools(self):
        values = inventory()
        values['thread/start']['activePermissionProfile']['id'] = 'triage'
        expected = codex.expected_inventory([], '/lab/codex/config.toml', profile='triage')
        self.assertEqual(codex.observe(values, expected)['mcp_servers_and_tools']['state'], 'mismatch')
        values['mcpServerStatus/list'] = [dict(data=[], nextCursor=None)]
        observed = codex.observe(values, expected)
        self.assertFalse(any(r['state'] == 'mismatch' for r in observed.values()))
        for m in (message('item/started', item=dict(type='mcpToolCall', server='lab')),
                  message('item/started', item=dict(type='mcpToolCall', server='codex', tool='list_mcp_resources')),
                  message('rawResponseItem/completed', item=dict(type='function_call', name='apply_patch', call_id='x'))):
            with self.assertRaises(Refused):
                codex.fold(codex.initial(), m, 0, triage=True)

    def test_tokens_count_completed_input_including_cached_plus_output_once(self):
        state = codex.initial()
        state = codex.fold(state, message('thread/tokenUsage/updated', turnId='t',
            tokenUsage=dict(total=dict(inputTokens=12, cachedInputTokens=5, outputTokens=3))), 1, triage=True)
        self.assertIsNone(triage.token_total(state))
        state = codex.fold(state, message('turn/completed', turn=dict(id='t', status='completed')), 2, triage=True)
        self.assertEqual(triage.token_total(state), 15)
        state['turns'][0]['tokens']['input'] = None
        self.assertIsNone(triage.token_total(state))

    def test_schema_fields_match_runner_contract(self):
        schema = json.loads((Path(__file__).resolve().parents[2] / 'schemas/triage.json').read_bytes())
        self.assertFalse(schema['additionalProperties'])
        self.assertEqual(schema['required'], ['failures'])
        row = schema['properties']['failures']['items']
        self.assertFalse(row['additionalProperties'])
        self.assertEqual(set(row['required']), set(self.answers['failures'][0]))
        self.assertEqual(row['properties']['class']['enum'], [*CLASSES, None])

    def test_triage_record_shape_and_unknown_usage(self):
        value = dict(format='lab-triage/1', run=self.run['run'], key=D, n=1, started=T, ended=T,
            outcome='error', invocation=None, expected_inventory=None, inventory=None, probe=None,
            input=None, output=None, bounds=dict(name='smoke', **fixture()[2]['costs']['smoke'],
            triage_seconds=1, triage_tokens=100), usage=dict(seconds=None, turns=0,
            tokens=dict(input=None, cached=None, output=None), tokens_overshoot=None),
            errors=[dict(what='triage', detail='host failed')], finding_events=[], brake=None)
        self.assertEqual(triage_refusals(value), [])
        self.assertEqual(canonical.loads(canonical.record_bytes(value)), value)
        for changes in (dict(format='lab-triage/2'), dict(n=0), dict(outcome='pass'), dict(extra=True),
                        dict(started='bad'), dict(usage={}), dict(input=dict(path='wrong', bytes=-1, sha256='x'))):
            with self.subTest(changes=changes):
                self.assertTrue(triage_refusals({**value, **changes}))


class Files(unittest.TestCase):
    setUp = test_finding_store.Files.setUp
    tearDown = test_finding_store.Files.tearDown
    key = test_finding_store.Files.key

    def test_retained_copy_does_not_walk_scratch_or_private_homes(self):
        base, target = 'fixture/run', 'fixture/triage/1/evidence'
        for name in ('run.json', 'events.jsonl', 'proc/p/stdout', 'commands/check.stdout', 'collected/c/obs',
                     'driver/events.jsonl', 'driver/provision/requests/1.json'):
            self.lab.write_once(base + '/' + name, b'evidence')
        for name in ('scratch/p', 'dirs/client', 'driver/home', 'driver/rehearsal/home', 'driver/provision/home'):
            fd = self.lab.directory(base + '/' + name, create=True)
            os.close(fd)
            (self.lab.root / base / name / 'forbidden').symlink_to('/unreadable')
        triager.retained(self.lab, base, target)
        paths = {v['path'] for v in store.snapshot(self.lab.root / target)}
        self.assertEqual(paths, {'run.json', 'events.jsonl', 'proc/p/stdout', 'commands/check.stdout', 'collected/c/obs',
                                'driver/events.jsonl', 'driver/provision/requests/1.json'})
        self.assertEqual((self.lab.root / target).stat().st_mode & 0o777, 0o500)

    def test_store_validates_whole_triage_before_any_append(self):
        record = run(1, key=self.original)
        from runner import evidence
        evidence.publish(self.lab, 'keys/sample/' + self.original, record)
        before = self.lab.read('findings/F-0001.jsonl')
        good = findings.event('proposed', T, 'triager', title='title', reason='reason', **{'class': 'tools'})
        invalid = dict(good, **{'class': None})
        with self.assertRaises(Refused):
            finding_store.append_triage(self.lab, [('F-0001', good), ('F-0001', invalid)])
        self.assertEqual(self.lab.read('findings/F-0001.jsonl'), before)
        finding_store.append_triage(self.lab, [('F-0001', good)])
        self.assertEqual(finding_store.sync(self.lab, T)['findings'][0]['proposed'], 'tools')

    def test_completed_turn_late_usage_enforces_token_bound(self):
        worker = triager.Triage(self.lab, dict(base='triage-test', bounds=dict(triage_tokens=15)))
        fd = self.lab.create_file('triage-test/events.jsonl')
        worker.files['events.jsonl'] = fd
        try:
            worker.received(message('turn/completed', turn=dict(id='t', status='completed')))
            worker.received(message('thread/tokenUsage/updated', turnId='t',
                tokenUsage=dict(total=dict(inputTokens=12, cachedInputTokens=5, outputTokens=3))))
            self.assertIsNone(worker.outcome)
            worker.received(message('thread/tokenUsage/updated', turnId='t',
                tokenUsage=dict(total=dict(inputTokens=12, cachedInputTokens=5, outputTokens=4))))
            self.assertEqual(worker.outcome, 'bound')
            self.assertEqual(worker.overshoot, 1)
            self.assertEqual(worker.errors[0]['what'], 'bound/triage_tokens')
        finally:
            os.close(fd)
