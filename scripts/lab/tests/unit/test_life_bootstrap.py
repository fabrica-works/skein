"""Life receives private channel facts, never its byte state or payload."""
import unittest
from runner import canonical
from .test_stopped import world, send, done


def starting():
    state = world()
    state.data['exp']['processes'][0].update(bootstrap=dict(stream='stdout', action='seed'))
    state.data['exp']['processes'][0]['ready'][0]['stream'] = 'stderr'
    state.data['exp']['actions'] = [dict(name='seed', argv=['/seed'], env={})]
    state, _ = send(state, 'provisioned')
    state, _ = send(state, 'spawned', ms=10, name='backend', group='processes')
    return state


def public(state, ms=11):
    return send(state, 'line', ms=ms, name='backend', stream='stderr', text='ready address')


def private(state, ms=12):
    return send(state, 'bootstrap_received', ms=ms, name='backend')


class BootstrapLife(unittest.TestCase):
    def test_public_readiness_and_private_receipt_in_both_orders(self):
        for order in ((public, private), (private, public)):
            state = starting()
            deadline = state.data['deadline']
            old = canonical.dumps(state.data)
            state, effects = order[0](state, ms=11)
            self.assertEqual(state.data['deadline'], deadline)
            self.assertFalse(any(e['op'] == 'spawn' for e in effects))
            self.assertNotIn('proc.backend.ready.address', state.data['known'])
            state, effects = order[1](state, ms=12)
            self.assertEqual(state.data['phase'], 'seed')
            self.assertEqual(state.data['known']['proc.backend.ready.address'], 'address')
            spawn = next(e for e in effects if e['op'] == 'spawn')
            self.assertEqual(spawn, dict(op='spawn', group='actions', name='seed', argv=['/seed'], env={},
                                         stdin=None, files=[], bootstrap=dict(process='backend')))
            self.assertTrue(b'private-canary' not in canonical.dumps(state.data), 'life contains only value-free facts')
            self.assertTrue(b'private-canary' not in canonical.dumps(dict(effects=effects)), 'effects contain only value-free facts')
            self.assertNotEqual(canonical.dumps(state.data), old)
            state, effects = private(state, ms=20)
            self.assertEqual(effects, [])
            self.assertEqual(state.data['processes']['backend']['bootstrap']['received_ms'], 11 if order[0] == private else 12)
            state, effects = send(state, 'bootstrap_delivered', ms=21, name='backend')
            self.assertEqual(effects, [dict(op='bootstrap_delivered', process='backend', action='seed', ms=21)])
            self.assertEqual(state.data['processes']['backend']['bootstrap']['delivered_ms'], 21)

    def test_receipt_timeline_effect_has_only_source_recipient_and_observed_time(self):
        state, effects = private(starting(), ms=17)
        self.assertEqual(effects, [dict(op='bootstrap_received', process='backend', action='seed', ms=17)])
        self.assertEqual(state.data['processes']['backend']['bootstrap'],
                         dict(stream='stdout', action='seed', received_ms=17, delivered_ms=None))

    def test_receipt_or_last_public_line_at_ready_bound_cuts_without_seed(self):
        for last in (public, private):
            for delta in (-1, 0, 1):
                state = starting()
                bound = state.data['deadline']
                state, _ = (private if last == public else public)(state)
                state, effects = last(state, ms=bound + delta)
                if delta < 0:
                    self.assertEqual(state.data['phase'], 'seed')
                    self.assertEqual(effects[-1]['bootstrap'], dict(process='backend'))
                else:
                    self.assertTrue(state.data['cut'])
                    self.assertEqual(state.data['phase'], 'collect')
                    self.assertIn(dict(op='bound', name='ready_seconds', who='backend'), effects)
                    self.assertFalse(any(e.get('group') == 'actions' for e in effects))

    def test_no_line_uses_existing_ready_tick_and_never_delivers(self):
        state, _ = public(starting())
        bound = state.data['deadline']
        state, effects = send(state, 'tick', ms=bound)
        self.assertTrue(state.data['cut'])
        self.assertIn(dict(op='bound', name='ready_seconds', who='backend'), effects)
        self.assertIsNone(state.data['processes']['backend']['bootstrap']['received_ms'])
        self.assertIsNone(state.data['processes']['backend']['bootstrap']['delivered_ms'])

    def test_framing_failure_during_start_and_recipient_cancels_like_process_exit(self):
        for phase in ('start', 'seed'):
            state = starting()
            if phase == 'seed':
                state, _ = public(state)
                state, _ = private(state)
                state, _ = send(state, 'spawned', name='seed', group='actions')
            state, effects = send(state, 'bootstrap_failed', name='backend')
            self.assertTrue(state.data['cut'])
            self.assertEqual(state.data['phase'], 'collect')
            self.assertFalse(any(e.get('group') == 'actions' for e in effects))
            self.assertEqual(any(e['op'] == 'cancel_command' for e in effects), phase == 'seed')
            state, _ = done(state, 'collect', 'obs')
            state, _ = done(state, 'checks', 'end')
            state, _ = send(state, 'exit', name='backend', result=dict(status=0))
            state, _ = send(state, 'tree_empty', name='backend')
            state, effects = send(state, 'closed')
            self.assertEqual(effects[-1]['cluster'], 'stopped')

    def test_interrupt_and_error_prevent_recipient_and_stopped_commands(self):
        for kind in ('interrupt', 'error'):
            for received in (False, True):
                state = starting()
                if received:
                    state, _ = private(state)
                state, effects = send(state, kind, signal=2, what='runner')
                self.assertFalse(any(e['op'] == 'spawn' for e in effects))
                self.assertIsNone(state.data['processes']['backend']['bootstrap']['delivered_ms'])
                self.assertTrue(any(e['op'] == 'unavailable' and e.get('cluster') == 'stopped' for e in effects))

    def test_ordinary_spawn_and_record_have_no_bootstrap_or_cluster_members(self):
        state = world()
        state, effects = send(state, 'provisioned')
        self.assertNotIn('bootstrap', effects[-1])
        self.assertNotIn('cluster', effects[-1])
        state, _ = send(state, 'spawned', name='backend', group='processes')
        self.assertNotIn('bootstrap', state.data['processes']['backend'])
