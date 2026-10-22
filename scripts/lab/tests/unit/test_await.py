from copy import deepcopy
import unittest
from runner import canonical, life
from .test_life_checks import event, ready, world


def awaiting(*, spawned=True):
    state, _ = world()
    action = state.data['exp']['actions'][0]
    action['await'] = dict(process='app', stream='stdout', prefix='fresh ', capture='word')
    state.data['exp']['actions'].append(dict(name='use', argv=['/usr/bin/helper'], env={}, stdin='{action.seed.word}\n'))
    state, _ = event(state, 'validated')
    state, effects = event(state, 'provisioned')
    for name in ('backend', 'middle', 'app'):
        state, effects = ready(state, name, name + ':123')
    if spawned:
        state, effects = event(state, 'spawned', ms=10, name='seed', group='actions')
    return state, effects


def line(state, ms=11, **fields):
    return event(state, 'line', ms=ms, **(dict(name='app', stream='stdout', text='fresh invented-word') | fields))


def done(state, ms=12, **result):
    return event(state, 'command_done', ms=ms, name='seed',
                 record=dict(name='seed', group='actions', exit={'status': 0, **result}))


class Await(unittest.TestCase):
    def test_line_then_exit_and_first_capture_rendered_in_next_stdin(self):
        state, _ = awaiting()
        old = canonical.dumps(state.data)
        state, effects = line(state)
        self.assertEqual(state.data['current'], 'seed')
        self.assertEqual(effects, [dict(op='awaited', name='seed', ms=11, captures={'word': 'invented-word'})])
        self.assertEqual(state.data['known']['action.seed.word'], 'invented-word')
        state, effects = line(state, text='fresh second-word')
        self.assertEqual(effects, [])
        state, effects = done(state)
        self.assertEqual(state.data['commands'][0]['awaited'], dict(ms=11, captures={'word': 'invented-word'}))
        self.assertEqual(effects[-1]['name'], 'use')
        self.assertEqual(effects[-1]['stdin'], 'invented-word\n')
        self.assertNotEqual(old, canonical.dumps(state.data))

    def test_exit_then_line_uses_same_deadline(self):
        state, _ = awaiting()
        deadline = state.data['deadline']
        state, effects = done(state)
        self.assertEqual(effects, [])
        self.assertEqual(state.data['current'], 'seed')
        self.assertEqual(state.data['deadline'], deadline)
        self.assertIsNone(state.data['commands'][0]['awaited'])
        state, effects = line(state, ms=13)
        self.assertEqual(effects[0]['op'], 'awaited')
        self.assertEqual(effects[-1]['stdin'], 'invented-word\n')
        self.assertEqual(state.data['commands'][0]['awaited']['ms'], 13)

    def test_before_spawn_older_time_wrong_process_stream_and_prefix_ignored(self):
        state, _ = awaiting(spawned=False)
        state, effects = line(state, ms=9)
        self.assertEqual(effects, [])
        state, _ = event(state, 'spawned', ms=10, name='seed', group='actions')
        for fields in (dict(ms=9), dict(name='backend'), dict(stream='stderr'), dict(text='other word')):
            before = deepcopy(state.data)
            state, effects = line(state, **fields)
            self.assertEqual(effects, [])
            self.assertEqual(state.data, before)
        self.assertNotIn('action.seed.word', state.data['known'])

    def test_deadline_after_exit_bounds_without_cancel_and_missing_capture_unavailable(self):
        state, _ = awaiting()
        deadline = state.data['deadline']
        state, _ = done(state)
        state, effects = event(state, 'tick', ms=deadline - 1)
        self.assertEqual(effects, [])
        state, effects = event(state, 'tick', ms=deadline)
        self.assertEqual(effects[0], dict(op='bound', name='command_seconds', who='seed'))
        self.assertFalse(any(e['op'] == 'cancel_command' for e in effects))
        self.assertTrue(any(e['op'] == 'unavailable' and e['name'] == 'use' for e in effects))
        self.assertIsNone(state.data['commands'][0]['awaited'])
        state, effects = line(state, ms=deadline + 1)
        self.assertNotIn('action.seed.word', state.data['known'])
        self.assertEqual(effects, [])

    def test_deadline_running_bounds_and_cancels_once(self):
        state, _ = awaiting()
        deadline = state.data['deadline']
        state, effects = event(state, 'tick', ms=deadline)
        self.assertEqual(effects, [dict(op='bound', name='command_seconds', who='seed'),
                                   dict(op='cancel_command', name='seed')])
        state, effects = line(state, ms=deadline + 1)
        self.assertEqual(effects, [])
        state, effects = event(state, 'tick', ms=deadline + 2)
        self.assertEqual(effects, [])
        state, effects = done(state, ms=deadline + 3, status=None, stopped=True, killed=True)
        self.assertIsNone(state.data['commands'][0]['awaited'])
        self.assertTrue(any(e['op'] == 'unavailable' and e['name'] == 'use' for e in effects))

    def test_nonzero_signal_cancelled_and_killed_do_not_await(self):
        for result in (dict(status=7), dict(status=None, signal='SIGTERM'),
                       dict(stopped=True), dict(killed=True)):
            for seen in (False, True):
                with self.subTest(result=result, seen=seen):
                    state, _ = awaiting()
                    if seen:
                        state, _ = line(state)
                    state, effects = done(state, **result)
                    self.assertIsNone(state.data['commands'][0]['awaited'])
                    self.assertNotIn('action.seed.word', state.data['known'])
                    self.assertTrue(any(e['op'] == 'unavailable' and e['name'] == 'use' for e in effects))

    def test_process_stop_while_waiting_cuts_and_stale_completion_does_not_advance(self):
        state, _ = awaiting()
        state, _ = done(state)
        state, effects = event(state, 'exit', ms=13, name='app', result={'status': 17})
        self.assertEqual(state.data['phase'], 'collect')
        self.assertTrue(any(e['op'] == 'cut' for e in effects))
        self.assertFalse(any(e.get('name') == 'use' for e in effects))
        state, effects = line(state, ms=14)
        self.assertEqual(effects, [])
        state, effects = done(state, ms=15)
        self.assertEqual(effects, [])

    def test_await_has_one_deadline_even_after_line(self):
        state, _ = awaiting()
        deadline = state.data['deadline']
        state, _ = line(state)
        self.assertEqual(state.data['deadline'], deadline)
        state, effects = event(state, 'tick', ms=deadline)
        self.assertIn(dict(op='cancel_command', name='seed'), effects)
        state, _ = done(state, ms=deadline + 1, stopped=True, killed=True)
        self.assertIsNone(state.data['commands'][0]['awaited'])

    def test_zero_exit_racing_timeout_cannot_restore_capture(self):
        state, _ = awaiting()
        deadline = state.data['deadline']
        state, _ = line(state)
        state, _ = event(state, 'tick', ms=deadline)
        state, effects = done(state, ms=deadline + 1)
        self.assertIsNone(state.data['commands'][0]['awaited'])
        self.assertNotIn('action.seed.word', state.data['known'])
        self.assertTrue(any(e['op'] == 'unavailable' and e['name'] == 'use' for e in effects))

    def test_zero_exit_at_deadline_is_bounded_even_before_tick(self):
        for seen in (False, True):
            state, _ = awaiting()
            deadline = state.data['deadline']
            if seen:
                state, _ = line(state)
            state, effects = done(state, ms=deadline)
            self.assertIn(dict(op='bound', name='command_seconds', who='seed'), effects)
            self.assertFalse(any(e['op'] == 'cancel_command' for e in effects))
            self.assertIsNone(state.data['commands'][0]['awaited'])
            self.assertNotIn('action.seed.word', state.data['known'])

    def test_unavailable_action_with_await_keeps_null_record(self):
        state, _ = awaiting()
        state.data['exp']['actions'][1]['await'] = dict(process='app', stream='stderr', prefix='fresh ', capture='next')
        state, _ = done(state, status=1)
        skipped = next(c for c in state.data['commands'] if c['name'] == 'use')
        self.assertIn('skipped', skipped)
        self.assertIsNone(skipped['awaited'])
