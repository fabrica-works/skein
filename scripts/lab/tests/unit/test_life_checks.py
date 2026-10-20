"""Scripted life stories; the clock and every completion are inputs."""
import unittest
from runner import canonical, checks, draws, judge, life
from runner.records import Refused
from .fixtures import fixture


def world():
    exp, _subject, settings, _facts, _lab = fixture()
    known = {'experiment': '/experiment', 'artifact.app': '/app', 'run.id': 'run', 'run.seed': '1',
             'dir.client': '/client', 'collected.obs': '/collected/obs'}
    known.update({'proc.' + p['name'] + '.dir': '/' + p['name'] for p in exp['processes']})
    return life.initial(exp, '1', settings['costs']['smoke'], known,
                        {'app.conf': 'endpoint={proc.backend.ready.address}'}), exp


def event(state, kind, ms=0, **fields):
    return life.step(state, dict(kind=kind, ms=ms, **fields))


def start():
    state, _exp = world()
    state, effects = event(state, 'begin')
    assert [x['op'] for x in effects] == ['validate']
    state, effects = event(state, 'validated')
    assert [x['op'] for x in effects] == ['phase', 'provision']
    state, effects = event(state, 'provisioned')
    return state, effects


def ready(state, name, address):
    state, _ = event(state, 'spawned', name=name, group='processes')
    return event(state, 'line', name=name, stream='stdout', text='ready ' + address)


def seeded():
    state, effects = start()
    for name in ('backend', 'middle', 'app'):
        assert next(x for x in effects if x['op'] == 'spawn')['name'] == name
        state, effects = ready(state, name, name + ':123')
    return state, effects


class Life(unittest.TestCase):
    def test_readiness_only_and_late_transitive_file_capture(self):
        state, effects = start()
        self.assertEqual(effects[-1]['name'], 'backend')
        state, _ = event(state, 'spawned', name='backend', group='processes')
        old = canonical.dumps(state.data)
        state, effects = event(state, 'line', stream='stderr', name='backend', text='ready wrong')
        self.assertEqual(effects, [])
        self.assertEqual(canonical.dumps(state.data), old)
        state, effects = event(state, 'line', stream='stdout', name='backend', text='ready 127.0.0.1:345')
        self.assertEqual(effects[-1]['name'], 'middle')
        state, effects = ready(state, 'middle', 'middle:456')
        app = effects[-1]
        self.assertEqual(app['name'], 'app')
        self.assertEqual(app['files'], [dict(path='app.conf', text='endpoint=127.0.0.1:345')])
        self.assertEqual(state.data['schedule']['draws']['backend'], draws.draw('1', 'backend'))
        self.assertIsInstance(state.data['schedule']['draws']['start-order.0'], str)
        canonical.u64(state.data['schedule']['draws']['start-order.0'])

    def test_exit_before_ready_prevents_later_starts_but_collects_and_checks(self):
        state, _ = start()
        state, _ = event(state, 'spawned', name='backend', group='processes')
        state, effects = event(state, 'exit', name='backend', result={'status': 17})
        self.assertEqual(state.data['phase'], 'collect')
        self.assertFalse(state.data['processes']['backend']['exit']['stopped'])
        self.assertEqual([x['group'] for x in effects if x['op'] == 'spawn'], ['collect'])
        self.assertFalse(any(x.get('name') in ('app', 'middle', 'seed') for x in effects))
        state, effects = event(state, 'command_done', name='obs', record=dict(name='obs', group='collect'))
        self.assertEqual(state.data['phase'], 'check')
        self.assertEqual(effects[-1]['group'], 'checks')

    def test_captures_become_known_only_after_all_ready_lines(self):
        state, _ = world()
        state.data['exp']['processes'][0]['ready'].append(dict(stream='stderr', prefix='complete', capture=None))
        state, _ = event(state, 'validated')
        state, _ = event(state, 'provisioned')
        state, _ = event(state, 'spawned', name='backend', group='processes')
        state, effects = event(state, 'line', name='backend', stream='stdout', text='ready address-one')
        self.assertEqual(effects, [])
        self.assertNotIn('proc.backend.ready.address', state.data['known'])
        state, effects = event(state, 'line', name='backend', stream='stdout', text='ready address-two')
        self.assertEqual(effects, [])
        state, effects = event(state, 'line', name='backend', stream='stderr', text='complete')
        self.assertEqual(state.data['known']['proc.backend.ready.address'], 'address-one')
        self.assertEqual(effects[-1]['name'], 'middle')

    def test_stop_skips_a_tree_already_closed_out_of_order(self):
        state, _ = seeded()
        state, _ = event(state, 'interrupt', signal=2)
        state, _ = event(state, 'exit', name='middle', result={'status': 0})
        state, _ = event(state, 'tree_empty', name='middle')
        state, _ = event(state, 'signalled', name='app')
        state, _ = event(state, 'exit', name='app', result={'status': 0})
        state, effects = event(state, 'tree_empty', name='app')
        self.assertEqual(effects[-1]['name'], 'backend')

    def test_cut_of_each_active_command_phase(self):
        for phase, group in (('seed', 'actions'), ('collect', 'collect'), ('check', 'checks')):
            state, _ = seeded()
            state.data.update(phase=phase, current='active', index=0)
            state, effects = event(state, 'exit', name='backend', result={'signal': 'SIGTERM'})
            self.assertTrue(state.data['cut'])
            self.assertTrue(any(x['op'] == 'cancel_command' and x['name'] == 'active' for x in effects))
            if phase == 'seed':
                self.assertEqual(state.data['phase'], 'collect')
            else:
                self.assertEqual(state.data['phase'], phase)

    def test_ready_command_and_stop_bounds_use_supplied_time(self):
        state, _ = start()
        state, _ = event(state, 'spawned', ms=9, name='backend', group='processes')
        bound = state.data['cost']['ready_seconds'] * 1000 + 9
        state, effects = event(state, 'tick', ms=bound - 1)
        self.assertEqual(effects, [])
        state, effects = event(state, 'tick', ms=bound)
        self.assertEqual(state.data['phase'], 'collect')
        self.assertTrue(any(x['op'] == 'bound' and x['name'] == 'ready_seconds' for x in effects))
        state, _ = seeded()
        state, _ = event(state, 'spawned', ms=10, name='seed', group='actions')
        state, effects = event(state, 'tick', ms=10 + state.data['cost']['command_seconds'] * 1000)
        self.assertTrue(any(x['op'] == 'cancel_command' for x in effects))
        state, effects = event(state, 'interrupt', signal=2)
        self.assertEqual(effects[-1]['name'], 'app')
        state, _ = event(state, 'signalled', ms=20, name='app')
        state, effects = event(state, 'tick', ms=20 + state.data['cost']['stop_seconds'] * 1000)
        self.assertTrue(any(x['op'] == 'kill_tree' and x['name'] == 'app' for x in effects))

    def test_interrupt_goes_directly_to_reverse_stop(self):
        state, _ = seeded()
        state, effects = event(state, 'interrupt', signal=15)
        self.assertEqual(state.data['phase'], 'stop')
        self.assertFalse(any(x.get('op') == 'spawn' for x in effects))
        self.assertEqual(effects[-1], dict(op='signal_first', name='app', signal='SIGTERM'))
        for name, following in (('app', 'middle'), ('middle', 'backend'), ('backend', None)):
            state, _ = event(state, 'signalled', name=name)
            state, effects = event(state, 'exit', name=name, result={'status': 0})
            self.assertEqual(effects, [dict(op='reap_tree', name=name)])
            state, effects = event(state, 'tree_empty', name=name)
            if following:
                self.assertEqual(effects[-1]['name'], following)
            else:
                self.assertEqual(effects[-1], dict(op='judge'))

    def test_exit_before_requested_term_is_still_unexpected(self):
        state, _ = seeded()
        state, _ = event(state, 'interrupt', signal=2)
        state, _ = event(state, 'exit', name='app', result={'status': 0})
        self.assertFalse(state.data['processes']['app']['exit']['stopped'])
        state, _ = event(state, 'tree_empty', name='app')
        state, _ = event(state, 'signalled', name='middle')
        state, _ = event(state, 'exit', name='middle', result={'status': 0})
        self.assertTrue(state.data['processes']['middle']['exit']['stopped'])

    def test_error_after_exit_resumes_tree_cleanup_when_pending_effects_are_lost(self):
        state, _ = start()
        state, _ = event(state, 'spawned', name='backend', group='processes')
        state, pending = event(state, 'exit', name='backend', result={'status': 101})
        self.assertIn(dict(op='reap_tree', name='backend'), pending)
        # An effect failure replaces the pending requests with the error's requests.
        state, effects = event(state, 'error', what='evidence', detail='write refused')
        self.assertEqual(state.data['phase'], 'stop')
        self.assertIn(dict(op='reap_tree', name='backend'), effects)
        state, effects = event(state, 'tree_empty', name='backend')
        self.assertEqual(effects[-1], dict(op='judge'))

    def test_stale_command_completion_cannot_advance_another_group(self):
        state, _ = seeded()
        state.data.update(phase='collect', current='same-name', index=0)
        state, effects = event(state, 'command_done', name='same-name', record=dict(group='actions'))
        self.assertEqual(state.data['index'], 0)
        self.assertEqual(effects, [])

    def test_missing_capture_is_explicitly_unjudged_and_nonnull_driver_refused(self):
        state, _exp = world()
        state.data['exp']['driver'] = {'charter': 'charter.md', 'tools': []}
        with self.assertRaises(Refused):
            life.initial(state.data['exp'], '1', state.data['cost'], {}, {})
        state, _ = start()
        state.data['exp']['collect'][0]['argv'].append('{proc.app.ready.address}')
        state, _ = event(state, 'spawned', name='backend', group='processes')
        state, effects = event(state, 'exit', name='backend', result={'status': 1})
        self.assertTrue(any(e['op'] == 'unavailable' and e['name'] == 'obs' for e in effects))
        # The check uses only collected paths and can still report its missing input.
        self.assertEqual(effects[-1]['group'], 'checks')


class Checks(unittest.TestCase):
    def test_command_contract(self):
        cases = [
            (0, b'', (0, 0, 0)),
            (1, b'{"fail":"state/missing","summary":"absent"}\n', (1, 0, 0)),
            (2, b'{"undecided":"file absent"}\n', (0, 1, 0)),
            (1, b'{"fail":"x","summary":"bad"}\n{"undecided":"another input absent"}\n', (1, 1, 0)),
            (0, b'\n', (0, 0, 1)), (1, b'', (0, 0, 1)), (2, b'', (0, 0, 1)),
            (3, b'', (0, 0, 1)), (None, b'', (0, 0, 1)),
            (0, b'{"fail":"x","summary":"bad"}\n', (0, 0, 1)),
            (2, b'{"fail":"x","summary":"bad"}\n', (0, 0, 1)),
            (1, b'{"fail":"has space","summary":"bad"}\n', (0, 0, 1)),
            (1, b'{"fail":"x","summary":"bad","extra":1}\n', (0, 0, 1)),
            (1, b'{"fail":"x","fail":"y","summary":"bad"}\n', (0, 0, 1)),
            (2, b'{"undecided":null}\n', (0, 0, 1)), (0, b'\xff', (0, 0, 1)),
        ]
        for status, data, expected in cases:
            with self.subTest(status=status, data=data):
                self.assertEqual(tuple(map(len, checks.command('end', status, data))), expected)
        failures, unknown, errors = checks.command('end', None, b'junk', timed_out=True)
        self.assertEqual(failures[0]['signature'], 'bound/command_seconds/end')
        self.assertFalse(unknown or errors)

    def test_panics_suppress_only_stop_and_missing_peak_stays_unknown(self):
        panic = checks.panics('app', "thread 'main' panicked at src/main.rs:12:3:\nmessage\n", ['proc/app/stderr'])
        self.assertEqual(panic[0]['signature'], 'panic/app/src/main.rs:12:3')
        self.assertEqual(panic[0]['summary'], 'message')
        record = dict(exit=dict(stopped=False, status=101),
                      memory=dict(peak_bytes=None, bound_bytes=100, tree_peak_bytes=999999))
        failures, unknown = checks.process('app', record, panic)
        self.assertEqual(failures, panic)
        self.assertEqual(len(unknown), 1)
        record['memory']['peak_bytes'] = 101
        failures, unknown = checks.process('app', record, [])
        self.assertEqual({f['signature'] for f in failures}, {'stop/app/exit:101', 'memory/app'})
        self.assertFalse(unknown)
        self.assertEqual(judge.outcome(failures, ['interrupted'], unknown, None, driver_required=False), 'fail')


class Interruptions(unittest.TestCase):
    def test_interrupt_in_each_phase_never_requests_collection_or_later_actions(self):
        for phase in ('validate', 'provision', 'start', 'seed', 'drive', 'collect', 'check', 'stop'):
            for sig in (2, 15):
                with self.subTest(phase=phase, signal=sig):
                    state, _ = seeded()
                    state.data.update(phase=phase, current='active', deadline=None)
                    state, effects = event(state, 'interrupt', signal=sig)
                    self.assertEqual(state.data['phase'], 'stop')
                    self.assertEqual(state.data['interrupted'], sig)
                    self.assertFalse(any(e['op'] == 'spawn' for e in effects))
                    self.assertEqual(state.data['errors'][-1]['what'], 'interrupted')

    def test_failure_error_incomplete_pass_precedence_is_explicit(self):
        cases = [(['failure'], ['error'], ['unknown'], 'fail'),
                 ([], ['error'], ['unknown'], 'error'), ([], [], ['unknown'], 'incomplete'),
                 ([], [], [], 'pass')]
        for failures, errors, unknown, expected in cases:
            self.assertEqual(judge.outcome(failures, errors, unknown, None, driver_required=False), expected)


class RunnerChecks(unittest.TestCase):
    def test_survivor_action_and_planted_checks_are_pure_observations(self):
        self.assertEqual(checks.survivors('worker', {'populated': False}), [])
        self.assertEqual(checks.survivors('worker', {'populated': True})[0]['signature'], 'survivor/worker')
        record = dict(exit=dict(status=7, stopped=False), output={'stderr': {'path': 'commands/actions-seed.stderr'}})
        self.assertEqual(checks.action('seed', record)[0]['signature'], 'action/seed/exit:7')
        record['exit']['stopped'] = True
        self.assertEqual(checks.action('seed', record), [])
        self.assertEqual(checks.planted(['expected'], [], 'error'), [])
        missing = checks.planted(['expected'], [], 'pass')
        self.assertEqual(missing[0]['signature'], 'planted-missing/expected')
        self.assertEqual(checks.planted(['expected'], [{'signature': 'expected'}], 'fail'), [])

    def test_tree_peak_never_supplies_the_own_process_peak(self):
        record = dict(exit=dict(status=0, stopped=True), memory=dict(peak_bytes=100, bound_bytes=100, tree_peak_bytes=1000000))
        self.assertEqual(checks.process('worker', record, []), ([], []))
        record['memory']['peak_bytes'] = None
        failures, unknown = checks.process('worker', record, [])
        self.assertEqual(failures, [])
        self.assertEqual(unknown, [dict(check='memory/worker', reason='own process peak unavailable')])
