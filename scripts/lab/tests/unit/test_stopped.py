"""Stopped commands need confirmed closure and whole terminal collections."""
from copy import deepcopy
import unittest
from runner import experiment, life
from .fixtures import fixture


def world():
    exp, _subject, settings, _facts, _lab = fixture()
    exp['processes'] = [exp['processes'][0]]
    exp['actions'] = []
    exp['collect'] += [dict(name='copy', cluster='stopped', argv=['/copy', '{collect.dir}'], env={}, scan=[])]
    exp['checks'] += [dict(name='state', cluster='stopped', argv=['/check', '{collected.copy}', '{collected.obs}'], env={}),
                      dict(name='independent', cluster='stopped', argv=['/check'], env={})]
    known = {'artifact.app': '/app', 'experiment': '/experiment', 'proc.backend.dir': '/backend',
             'collected.obs': '/collected/obs', 'collected.copy': '/collected/copy'}
    state = life.initial(exp, '1', settings['costs']['smoke'], known, {})
    return state


def send(state, kind, ms=1, **fields):
    return life.step(state, dict(kind=kind, ms=ms, **fields))


def boot():
    state = world()
    state, _ = send(state, 'provisioned')
    state, _ = send(state, 'spawned', name='backend', group='processes')
    return send(state, 'line', name='backend', stream='stdout', text='ready address')


def done(state, group, name, *, status=0, stopped=False, killed=False, ms=3, **fields):
    state, _ = send(state, 'spawned', ms=2, group=group, name=name)
    record = dict(group=group, name=name, exit=dict(status=status, stopped=stopped, killed=killed), **fields)
    if state.data['phase'].startswith('stopped-'):
        record['cluster'] = 'stopped'
    return send(state, 'command_done', ms=ms, name=name, record=record)


def stopping():
    state, effects = boot()
    assert effects[-1]['name'] == 'obs'
    state, _ = done(state, 'collect', 'obs')
    state, effects = done(state, 'checks', 'end')
    assert state.data['phase'] == 'stop'
    return state, effects


def closed(state):
    state, _ = send(state, 'signalled', name='backend')
    state, _ = send(state, 'exit', name='backend', result=dict(status=0))
    state, _ = send(state, 'tree_empty', name='backend')
    return send(state, 'closed')


class Validation(unittest.TestCase):
    def test_cluster_scope_and_templates(self):
        exp, subject, settings, facts, _ = fixture()
        original = deepcopy(exp)
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])
        self.assertEqual(exp, original)
        for collection in ('up', 'stopped'):
            for check in ('up', 'stopped'):
                exp['collect'][0]['cluster'] = collection
                exp['checks'][0]['cluster'] = check
                problems = experiment.validate(exp, subject, settings, facts)
                if collection == 'stopped' and check == 'up':
                    self.assertEqual([r.member for r in problems], ['checks.0.argv.1'])
                else:
                    self.assertEqual(problems, [])

    def test_wrong_values_types_and_members_all_refused(self):
        for value in (None, False, [], {}, 1, 'down', ''):
            exp, subject, settings, facts, _ = fixture()
            exp['collect'][0]['cluster'] = value
            exp['checks'][0]['cluster'] = value
            exp['actions'][0]['cluster'] = 'stopped'
            exp['processes'][0]['cluster'] = 'stopped'
            members = {r.member for r in experiment.validate(exp, subject, settings, facts)}
            self.assertTrue({'collect.0.cluster', 'checks.0.cluster', 'actions.0.cluster', 'processes.0.cluster'} <= members)


class Stopped(unittest.TestCase):
    def test_live_stop_closed_stopped_collect_check_sequence(self):
        state, effects = stopping()
        self.assertEqual(effects[-1], dict(op='signal_first', name='backend', signal='SIGTERM'))
        state, effects = send(state, 'closed')
        self.assertEqual(effects, [])
        state, effects = send(state, 'tree_empty', name='backend')
        self.assertEqual(effects, [])
        self.assertNotIn('backend', state.data['closed'])
        state, _ = send(state, 'signalled', name='backend')
        state, effects = send(state, 'exit', name='backend', result={'status': 0})
        self.assertEqual(effects, [dict(op='reap_tree', name='backend')])
        state, effects = send(state, 'closed')
        self.assertEqual(effects, [])
        state, effects = send(state, 'tree_empty', name='backend')
        self.assertEqual(state.data['phase'], 'stop')
        self.assertFalse(any(e['op'] == 'spawn' for e in effects))
        state, effects = send(state, 'closed')
        self.assertEqual(state.data['phase'], 'stopped-collect')
        self.assertEqual(effects[-1]['argv'], ['/copy', '/collected/copy'])
        self.assertEqual(effects[-1]['cluster'], 'stopped')
        state, effects = done(state, 'collect', 'copy')
        self.assertEqual(state.data['phase'], 'stopped-check')
        self.assertEqual(effects[-1]['name'], 'state')
        state, effects = done(state, 'checks', 'state')
        self.assertEqual(effects[-1]['name'], 'independent')
        state, effects = done(state, 'checks', 'independent')
        self.assertEqual(effects[-1], dict(op='judge'))
        self.assertEqual([c['name'] for c in state.data['commands']], ['obs', 'end', 'copy', 'state', 'independent'])
        self.assertTrue(all('cluster' not in c for c in state.data['commands'][:2]))

    def test_copy_terminal_record_controls_dependent_check(self):
        for bad in (dict(status=7), dict(stopped=True), dict(killed=True), dict(timed_out=True), dict(cancelled=True)):
            state, _ = stopping()
            state, _ = closed(state)
            state, effects = done(state, 'collect', 'copy', **bad)
            self.assertEqual(effects[-1]['name'], 'independent')
            skip = next(e for e in effects if e['op'] == 'unavailable')
            self.assertEqual(skip, dict(op='unavailable', group='checks', name='state', cluster='stopped',
                                        reason='collection input did not complete successfully'))
            self.assertFalse(state.data['collections']['copy'])
            self.assertEqual(state.data['commands'][-1]['skipped'], skip['reason'])

    def test_dependencies_in_environment_and_stdin_use_terminal_success(self):
        for field in ('env', 'stdin'):
            state, _ = stopping()
            check = state.data['exp']['checks'][1]
            check['argv'] = ['/check']
            if field == 'env':
                check['env'] = {'INPUT': '{collected.copy}'}
            else:
                check['stdin'] = '{collected.copy}\n'
            state, _ = closed(state)
            state, effects = done(state, 'collect', 'copy', status=7)
            self.assertEqual(effects[-1]['name'], 'independent')
            self.assertTrue(any(e['op'] == 'unavailable' and e['name'] == 'state' for e in effects))

    def test_live_collection_completed_at_bound_is_not_whole_for_stopped_check(self):
        state, _ = boot()
        deadline = 2 + state.data['cost']['command_seconds'] * 1000
        state, _ = done(state, 'collect', 'obs', ms=deadline)
        self.assertFalse(state.data['collections']['obs'])
        state, _ = done(state, 'checks', 'end')
        state, _ = closed(state)
        state, effects = done(state, 'collect', 'copy')
        self.assertEqual(effects[-1]['name'], 'independent')
        self.assertTrue(any(e['op'] == 'unavailable' and e['name'] == 'state' for e in effects))

    def test_missing_copy_capture_and_failed_live_input_do_not_retry(self):
        state, _ = stopping()
        state.data['exp']['collect'][1]['argv'].append('{proc.backend.ready.missing}')
        state, effects = closed(state)
        self.assertEqual(state.data['phase'], 'stopped-check')
        self.assertEqual(effects[-1]['name'], 'independent')
        self.assertEqual([e['name'] for e in effects if e['op'] == 'unavailable'], ['copy', 'state'])
        self.assertFalse(any(e.get('name') == 'copy' and e['op'] == 'spawn' for e in effects))
        state, _ = boot()
        state, _ = done(state, 'collect', 'obs', status=9)
        state, _ = done(state, 'checks', 'end')
        state, _ = closed(state)
        state, effects = done(state, 'collect', 'copy')
        self.assertEqual(effects[-1]['name'], 'independent')
        self.assertTrue(state.data['collections']['copy'])
        self.assertFalse(state.data['collections']['obs'])

    def test_unexpected_product_stop_still_permits_stopped_commands(self):
        state, _ = boot()
        state, effects = send(state, 'exit', name='backend', result=dict(status=17))
        self.assertTrue(state.data['cut'])
        self.assertIn(dict(op='cancel_command', name='obs'), effects)
        state, _ = done(state, 'collect', 'obs', stopped=True)
        state, _ = done(state, 'checks', 'end')
        state, _ = send(state, 'tree_empty', name='backend')
        state, effects = send(state, 'closed')
        self.assertEqual(effects[-1]['name'], 'copy')

    def test_interrupt_or_error_in_every_phase_skips_pending_stopped_commands(self):
        for phase in ('validate', 'provision', 'start', 'seed', 'drive', 'collect', 'check', 'stop',
                      'stopped-collect', 'stopped-check', 'judge', 'closed'):
            for kind in ('interrupt', 'error'):
                state, _ = stopping()
                state.data.update(phase=phase, current='active', deadline=None)
                state, effects = send(state, kind, signal=2, what='runner', detail='supplied error')
                self.assertFalse(any(e['op'] == 'spawn' for e in effects))
                self.assertEqual({e['name'] for e in effects if e['op'] == 'unavailable'}, {'copy', 'state', 'independent'})
                self.assertEqual(sum(e['op'] == 'cancel_command' for e in effects), int(phase in life.COMMAND_GROUP))
                state, _ = send(state, 'exit', name='backend', result=dict(status=0))
                state, _ = send(state, 'tree_empty', name='backend')
                state, effects = send(state, 'closed')
                self.assertFalse(any(e['op'] == 'spawn' for e in effects))
                state, effects = send(state, kind, signal=2, what='runner')
                self.assertFalse(any(e['op'] == 'unavailable' for e in effects))

    def test_active_stopped_command_is_cancelled_and_not_recorded_as_unstarted(self):
        for group, name in (('collect', 'copy'), ('checks', 'state')):
            state, _ = stopping()
            state, _ = closed(state)
            if group == 'checks':
                state, _ = done(state, 'collect', 'copy')
            state, _ = send(state, 'spawned', group=group, name=name)
            state, effects = send(state, 'interrupt', signal=15)
            self.assertIn(dict(op='cancel_command', name=name), effects)
            self.assertFalse(any(e['op'] == 'unavailable' and e['name'] == name for e in effects))
            self.assertEqual(state.data['phase'], 'judge')
            state, effects = send(state, 'command_done', name=name, record=dict(group=group, exit=dict(status=0)))
            self.assertEqual(effects, [])

    def test_stopped_command_bounds_and_completion_on_boundary(self):
        for group, name in (('collect', 'copy'), ('checks', 'state')):
            state, _ = stopping()
            state, _ = closed(state)
            if group == 'checks':
                state, _ = done(state, 'collect', 'copy')
            state, _ = send(state, 'spawned', ms=5, group=group, name=name)
            bound = 5 + state.data['cost']['command_seconds'] * 1000
            state, effects = send(state, 'tick', ms=bound - 1)
            self.assertEqual(effects, [])
            for ms in (bound, bound + 1):
                expired = deepcopy(state)
                expired, effects = send(expired, 'tick', ms=ms)
                self.assertIn(dict(op='cancel_command', name=name), effects)
                self.assertIn(dict(op='bound', name='command_seconds', who=name), effects)
            state, effects = send(state, 'command_done', ms=bound, name=name,
                                  record=dict(group=group, name=name, cluster='stopped',
                                              exit=dict(status=0, stopped=False, killed=False)))
            self.assertIn(dict(op='bound', name='command_seconds', who=name), effects)
            if group == 'collect':
                self.assertFalse(state.data['collections']['copy'])
                self.assertEqual(effects[-1]['name'], 'independent')
