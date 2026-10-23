"""The optional bootstrap channel reserves a whole stream and one recipient."""
from copy import deepcopy
import unittest
from runner import experiment
from .fixtures import fixture


def inputs():
    exp, subject, settings, facts, _ = fixture()
    exp['processes'][0]['bootstrap'] = dict(stream='stdout', action='seed')
    exp['processes'][0]['ready'][0]['stream'] = 'stderr'
    del exp['actions'][0]['stdin']
    return exp, subject, settings, facts


class BootstrapValidation(unittest.TestCase):
    def test_channel_validates_without_mutating_inputs_or_adding_templates(self):
        exp, subject, settings, facts = inputs()
        original = deepcopy(exp)
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])
        self.assertEqual(exp, original)
        for name in ('bootstrap.backend', 'proc.backend.bootstrap', 'proc.backend.ready.secret', 'action.seed.secret'):
            changed = deepcopy(exp)
            changed['actions'][0]['env']['VALUE'] = '{' + name + '}'
            self.assertIn('actions.0.env.VALUE', [r.member for r in experiment.validate(changed, subject, settings, facts)])

    def test_shape_types_unknown_members_and_missing_members_all_refused(self):
        for value in (None, [], False, 0, 'stdout'):
            exp, subject, settings, facts = inputs()
            exp['processes'][0]['bootstrap'] = value
            self.assertIn('processes.0.bootstrap', [r.member for r in experiment.validate(exp, subject, settings, facts)])
        exp, subject, settings, facts = inputs()
        exp['processes'][0]['bootstrap'] = {'unknown': True}
        members = {r.member for r in experiment.validate(exp, subject, settings, facts)}
        self.assertTrue({'processes.0.bootstrap.stream', 'processes.0.bootstrap.action', 'processes.0.bootstrap.unknown'} <= members)
        for field, values in (('stream', ('stderr', 'stdin', None, [], 1)),
                              ('action', ('missing', 'Bad', '', None, [], 1))):
            for value in values:
                exp, subject, settings, facts = inputs()
                exp['processes'][0]['bootstrap'][field] = value
                self.assertIn('processes.0.bootstrap.' + field,
                              [r.member for r in experiment.validate(exp, subject, settings, facts)])

    def test_recipient_cannot_declare_stdin_even_empty_or_await(self):
        for value in ('', None, 'ordinary'):
            exp, subject, settings, facts = inputs()
            exp['actions'][0]['stdin'] = value
            self.assertIn('processes.0.bootstrap.action', [r.member for r in experiment.validate(exp, subject, settings, facts)])
        exp, subject, settings, facts = inputs()
        exp['actions'][0]['await'] = dict(process='app', stream='stdout', prefix='fresh ', capture='word')
        self.assertIn('processes.0.bootstrap.action', [r.member for r in experiment.validate(exp, subject, settings, facts)])

    def test_duplicates_private_readers_and_recipient_errors_are_aggregated(self):
        exp, subject, settings, facts = inputs()
        exp['processes'][1]['bootstrap'] = dict(stream='stdout', action='seed')
        exp['processes'][0]['ready'][0]['stream'] = 'stdout'
        exp['actions'][0].update(stdin='', **{'await': dict(process='backend', stream='stdout', prefix='fresh ', capture='word')})
        problems = experiment.validate(exp, subject, settings, facts)
        members = {r.member for r in problems}
        self.assertTrue({'processes.0.bootstrap.action', 'processes.1.bootstrap.action',
                         'processes.0.ready.0.stream', 'processes.1.ready.0.stream', 'actions.0.await.stream'} <= members)
        self.assertTrue(any('unique' in r.why for r in problems))
        self.assertTrue(any('stdin' in r.why for r in problems))
        self.assertTrue(any('await' in r.why for r in problems))

    def test_other_actions_may_await_public_stream_but_never_private_stdout(self):
        exp, subject, settings, facts = inputs()
        exp['actions'].append(dict(name='other', argv=['/usr/bin/helper'], env={},
                                   **{'await': dict(process='backend', stream='stderr', prefix='fresh ', capture='word')}))
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])
        exp['actions'][1]['await']['stream'] = 'stdout'
        self.assertIn('actions.1.await.stream', [r.member for r in experiment.validate(exp, subject, settings, facts)])

    def test_only_processes_accept_bootstrap(self):
        for group in ('actions', 'collect', 'checks'):
            exp, subject, settings, facts = inputs()
            exp[group][0]['bootstrap'] = dict(stream='stdout', action='seed')
            self.assertIn(group + '.0.bootstrap', [r.member for r in experiment.validate(exp, subject, settings, facts)])
