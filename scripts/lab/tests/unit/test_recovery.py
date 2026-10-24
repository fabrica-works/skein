"""Interrupted records use supplied context and surviving events, without a clock."""
import unittest
from runner import canonical, draws, recovery, records
from runner.records import Refused
from .test_runtime_store import run_record


class Recovery(unittest.TestCase):
    def test_truncated_events_keep_observations_and_taken_handoffs(self):
        context = run_record(1, '7')
        context.update(schedule=dict(draws={}, start_order=[], faults=[]), held=[])
        observed = [dict(event='start', ms=1, name='server', group='processes', argv=['/server'], env={}),
                    dict(event='held_taken', ms=2, name='token'),
                    dict(event='held_taken', ms=3, name='returned'),
                    dict(event='held_given', ms=4, name='returned')]
        data = b''.join(canonical.record_bytes(e) for e in observed) + b'{"event":"exit"'
        value, event = recovery.interrupted(context, data, context['ended'], ['token', 'handoff-cut'])
        self.assertEqual(records.run_refusals(value), [])
        self.assertEqual(value['outcome'], 'error')
        self.assertEqual(value['errors'][0]['what'], 'interrupted')
        self.assertEqual(event['event'], 'interrupted')
        self.assertEqual(value['processes']['server']['argv'], ['/server'])
        self.assertNotIn('exit', value['processes']['server'])
        self.assertEqual([h['name'] for h in value['held'] if h['given_ms'] is None], ['token', 'handoff-cut'])
        self.assertIsNone(value['usage']['driver'])
        self.assertIsNone(next(h['taken_ms'] for h in value['held'] if h['name'] == 'handoff-cut'))
        self.assertEqual(context['processes'], {})
        self.assertEqual(recovery.events(data + b'\n' + canonical.record_bytes(event))[-1], event)

    def test_no_events_is_a_complete_error_record_with_unknown_usage(self):
        context = run_record(1, '7')
        value, _ = recovery.interrupted(context, b'', context['ended'])
        self.assertEqual(records.run_refusals(value), [])
        self.assertEqual((value['processes'], value['commands'], value['held']), ({}, [], []))
        self.assertEqual(value['outcome'], 'error')

    def test_only_confirmed_terminal_unit_states_admit_recovery(self):
        for state in ('inactive', 'failed'):
            self.assertTrue(recovery.unit_ended('LoadState=not-found\nActiveState=' + state))
        for state in ('active', 'activating', 'deactivating', 'reloading', 'refreshing', 'maintenance'):
            self.assertFalse(recovery.unit_ended('LoadState=loaded\nActiveState=' + state))
        for answer in ('', 'LoadState=loaded', 'ActiveState=inactive', 'ActiveState=unknown\nLoadState=loaded'):
            with self.assertRaises(Refused):
                recovery.unit_ended(answer)

    def test_triage_context_and_partial_events_become_error_without_proposals(self):
        run = run_record(1, '7')
        context = dict(format='lab-triage/1', run=run['run'], key=run['key'], n=1,
            started=run['started'], ended=run['ended'], outcome='error', invocation=None,
            expected_inventory=None, inventory=None, probe=None, input=None, output=None,
            bounds=dict(**run['bounds'], triage_seconds=2, triage_tokens=100),
            usage=dict(seconds=None, turns=0, tokens=dict(input=None, cached=None, output=None), tokens_overshoot=None),
            errors=[], finding_events=[], brake=None, rehearsal=dict(script=run['key']))
        value, event = recovery.interrupted(context, b'{"method":', run['ended'])
        self.assertEqual(records.triage_refusals(value), [])
        self.assertEqual(value['outcome'], 'error')
        self.assertEqual(value['finding_events'], [])
        self.assertEqual(value['usage']['tokens'], dict(input=None, cached=None, output=None))
        self.assertEqual(event['event'], 'interrupted')

    def test_surviving_driver_updates_keep_usage_even_without_a_completed_turn(self):
        context = run_record(1, '7')
        rows = [dict(t_ms=1, method='turn/started', params=dict(turn=dict(id='turn'))),
                dict(t_ms=2, method='thread/tokenUsage/updated', params=dict(turnId='turn',
                     tokenUsage=dict(total=dict(inputTokens=123, cachedInputTokens=12, outputTokens=9))))]
        data = b''.join(canonical.record_bytes(r) for r in rows) + b'{"method":'
        value, _event = recovery.interrupted(context, b'', context['ended'], driver_data=data)
        self.assertEqual(records.run_refusals(value), [])
        self.assertEqual(value['usage']['driver']['tokens'], dict(input=123, cached=12, output=9))
        self.assertEqual(value['usage']['driver']['turns'], 1)
        self.assertIsNone(value['driver']['turns'][0]['completed_ms'])

    def test_observed_fault_and_start_draws_survive_a_cut_before_publication(self):
        context = run_record(1, '7')
        context['schedule'] = dict(draws={}, faults=[], start_order=[])
        rows = [dict(event='start', ms=1, name='server', group='processes', argv=['/server'], env={}),
                dict(event='fault', ms=2, index=0, kill='server', signal='SIGKILL', n=1)]
        value, _event = recovery.interrupted(context, b''.join(canonical.record_bytes(e) for e in rows), context['ended'])
        self.assertEqual(value['schedule']['faults'], [dict(index=0, kill='server', signal='SIGKILL', n=1, fired=True, ms=2)])
        self.assertEqual(value['schedule']['draws'], {label: draws.draw('7', label) for label in ('start-order.0', 'fault.0')})
