from copy import deepcopy
import unittest
from runner import experiment
from .fixtures import D, fixture
from .test_tree import GOOD, archive, bad_archives


def inputs():
    exp, subject, settings, facts, _ = fixture()
    exp['actions'][0]['await'] = dict(process='app', stream='stdout', prefix='fresh ', capture='word')
    exp['actions'].append(dict(name='next', argv=['/usr/bin/helper', '{action.seed.word}'], env={},
                               stdin='{action.seed.word}\n'))
    return exp, subject, settings, facts


class AwaitExperiment(unittest.TestCase):
    def test_await_and_later_action_collection_check_templates(self):
        exp, subject, settings, facts = inputs()
        for group in ('collect', 'checks'):
            exp[group][0]['env']['VALUE'] = '{action.seed.word}'
            exp[group][0]['stdin'] = '{action.seed.word}\n'
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])

    def test_every_await_refusal_names_member_and_aggregates(self):
        exp, subject, settings, facts = inputs()
        rule = exp['actions'][0]['await']
        for field, values in (('process', ('missing', None, [])), ('stream', ('stdin', None, [])),
                              ('prefix', (None, [], 'bad\n', 'bad\r', 'bad\0')),
                              ('capture', (None, [], '', 'Bad', 'one.two'))):
            for value in values:
                changed = deepcopy(exp)
                changed['actions'][0]['await'][field] = value
                with self.subTest(field=field, value=value):
                    members = [r.member for r in experiment.validate(changed, subject, settings, facts)]
                    self.assertIn('actions.0.await.' + field, members)
        for field in rule:
            changed = deepcopy(exp)
            del changed['actions'][0]['await'][field]
            self.assertIn('actions.0.await.' + field,
                          [r.member for r in experiment.validate(changed, subject, settings, facts)])
        rule.update(process='missing', stream='stdin', prefix='\n', capture='Bad', unknown=True)
        members = [r.member for r in experiment.validate(exp, subject, settings, facts)]
        for field in ('process', 'stream', 'prefix', 'capture', 'unknown'):
            self.assertIn('actions.0.await.' + field, members)
        exp['actions'][0]['await'] = None
        self.assertIn('actions.0.await', [r.member for r in experiment.validate(exp, subject, settings, facts)])

    def test_only_seed_actions_accept_await(self):
        for group in ('collect', 'checks', 'processes'):
            exp, subject, settings, facts = inputs()
            exp[group][0]['await'] = exp['actions'][0]['await']
            with self.subTest(group=group):
                self.assertIn(group + '.0.await', [r.member for r in experiment.validate(exp, subject, settings, facts)])

    def test_action_capture_scope_and_executable_identity(self):
        for location in ('before', 'self', 'process', 'file', 'executable', 'unknown-action', 'unknown-capture'):
            exp, subject, settings, facts = inputs()
            expected = 'actions.1.stdin'
            if location == 'before':
                exp['actions'].reverse()
                expected = 'actions.0.stdin'
            elif location == 'self':
                exp['actions'][0]['stdin'] = '{action.seed.word}'
                expected = 'actions.0.stdin'
            elif location == 'process':
                exp['processes'][0]['env']['VALUE'] = '{action.seed.word}'
                expected = 'processes.0.env.VALUE'
            elif location == 'file':
                facts['experiment_files']['app.conf'] = '{action.seed.word}'
                expected = 'processes.2.files.0.from'
            elif location == 'executable':
                exp['actions'][1]['argv'][0] = '{action.seed.word}'
                expected = 'actions.1.argv.0'
            elif location == 'unknown-action':
                exp['actions'][1]['stdin'] = '{action.missing.word}'
            else:
                exp['actions'][1]['stdin'] = '{action.seed.missing}'
            with self.subTest(location=location):
                self.assertIn(expected, [r.member for r in experiment.validate(exp, subject, settings, facts)])

    def test_tool_capture_and_await_are_refused(self):
        exp, subject, settings, facts = inputs()
        host = dict(program=dict(path='/usr/bin/codex', sha256=D, version='1'), model='test', effort='medium')
        settings.update(driver=host, triager=None)
        facts.update(driver_host=True)
        facts['experiment_files']['charter.md'] = 'Invented test only.'
        tool = dict(provision=dict(args=[], status=0), name='client', argv=['{artifact.app}'], env={}, reads=[], writes=[])
        exp['driver'] = dict(charter='charter.md', tools=[tool])
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])
        tool.update(env={'VALUE': '{action.seed.word}'}, **{'await': exp['actions'][0]['await']})
        members = [r.member for r in experiment.validate(exp, subject, settings, facts)]
        self.assertIn('driver.tools.0.env.VALUE', members)
        self.assertIn('driver.tools.0.await', members)


class TreeExperiment(unittest.TestCase):
    def inputs(self):
        exp, subject, settings, facts, _ = fixture()
        exp['processes'][2]['files'].append(dict(path='bundle', tree='app'))
        facts['artifact_bytes'] = {'app': archive(GOOD)}
        return exp, subject, settings, facts

    def test_tree_accepted_and_its_contents_are_never_templates(self):
        self.assertEqual(experiment.validate(*self.inputs()), [])

    def test_unknown_unavailable_or_missing_artifact_bytes_refused_by_member(self):
        for value in ('unknown', None, []):
            exp, subject, settings, facts = self.inputs()
            exp['processes'][2]['files'][1]['tree'] = value
            self.assertIn('processes.2.files.1.tree', [r.member for r in experiment.validate(exp, subject, settings, facts)])
        for field in ('artifact_bytes', 'artifacts_available'):
            exp, subject, settings, facts = self.inputs()
            del facts[field]
            self.assertIn('processes.2.files.1.tree', [r.member for r in experiment.validate(exp, subject, settings, facts)])

    def test_each_archive_refusal_is_named_at_tree_member(self):
        for why, data in bad_archives():
            exp, subject, settings, facts = self.inputs()
            facts['artifact_bytes']['app'] = data
            with self.subTest(why=why):
                self.assertIn('processes.2.files.1.tree', [r.member for r in experiment.validate(exp, subject, settings, facts)])

    def test_file_requires_exactly_one_source_and_refuses_unknown_members(self):
        for entry in (dict(path='both', tree='app', **{'from': 'app.conf'}), dict(path='neither')):
            exp, subject, settings, facts = self.inputs()
            exp['processes'][2]['files'][1] = entry
            members = [r.member for r in experiment.validate(exp, subject, settings, facts)]
            self.assertIn('processes.2.files.1.from', members)
            self.assertIn('processes.2.files.1.tree', members)
        exp, subject, settings, facts = self.inputs()
        exp['processes'][2]['files'][1]['unknown'] = True
        self.assertIn('processes.2.files.1.unknown', [r.member for r in experiment.validate(exp, subject, settings, facts)])

    def test_overlaps_refused_across_both_source_kinds(self):
        for first in ('from', 'tree'):
            for second in ('from', 'tree'):
                for other in ('bundle', 'bundle/child', 'parent'):
                    exp, subject, settings, facts = self.inputs()
                    path = 'parent/bundle' if other == 'parent' else 'bundle'
                    source = lambda kind: {kind: 'app.conf' if kind == 'from' else 'app'}
                    exp['processes'][2]['files'] = [dict(path=path, **source(first)), dict(path=other, **source(second))]
                    with self.subTest(first=first, second=second, other=other):
                        self.assertIn('processes.2.files.1.path', [r.member for r in experiment.validate(exp, subject, settings, facts)])
