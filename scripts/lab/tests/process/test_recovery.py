"""Sweep host decisions against a private store and supplied manager answers."""
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from runner import canonical, evidence, store
from runner.records import Refused
from tests.unit.fixtures import D, fixture
from tests.unit.test_runtime_store import run_record


class Sweep(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.lab = store.open_lab(Path(self.temp.name) / 'lab')
        self.record = run_record(1, '1')
        self.key = 'keys/sample/' + self.record['key']
        self.base = self.key + '/runs/' + self.record['run']
        self.marker = dict(format='lab-live/1', unit='lab-test.service', systemctl='/systemctl', context=self.record)

    def tearDown(self):
        self.lab.close()
        self.temp.cleanup()

    def live(self):
        if self.base.startswith('keys/') and '/triage/' not in self.base:
            self.publication_inputs()
        self.lab.write_once(self.base + '/live.json', canonical.record_bytes(self.marker))

    def publication_inputs(self):
        self.lab.write_once(self.key + '/key.json', canonical.record_bytes(
            dict(format='lab-key/1', experiment=D, subject=D, lab=fixture()[4])))
        self.lab.write_once('settings.json', canonical.record_bytes(fixture()[2]))

    def test_terminal_unit_sweeps_once_and_publishes_error(self):
        self.live()
        with patch('runner.host.manager', return_value='LoadState=not-found\nActiveState=inactive') as manager:
            evidence.require_recovered(self.lab, writing=True)
            evidence.require_recovered(self.lab, writing=True)
        self.assertEqual(manager.call_count, 1)
        self.assertEqual(manager.call_args.args[1], ['show', '--property=ActiveState,LoadState', 'lab-test.service'])
        self.assertEqual(manager.call_args.args[2], self.record['bounds']['command_seconds'])
        saved = canonical.loads(self.lab.read(self.base + '/run.json'))
        self.assertEqual(saved['outcome'], 'error')
        self.assertEqual(len(evidence.read(self.lab, self.key)), 1)
        self.assertFalse((self.lab.root / self.base / 'live.json').exists())

    def test_completed_record_wins_and_reconciles_publication(self):
        self.live()
        self.lab.write_once(self.base + '/run.json', canonical.record_bytes(self.record))
        with patch('runner.host.manager', return_value='LoadState=loaded\nActiveState=failed'):
            evidence.require_recovered(self.lab, writing=True)
        self.assertEqual(canonical.loads(self.lab.read(self.base + '/run.json')), self.record)
        self.assertEqual(evidence.read(self.lab, self.key)[0]['outcome'], 'pass')

    def test_active_unknown_legacy_and_reader_leave_marker(self):
        self.live()
        for answer in ('LoadState=loaded\nActiveState=active', ''):
            with patch('runner.host.manager', return_value=answer), self.assertRaises(Refused):
                evidence.require_recovered(self.lab, writing=True)
        with patch('runner.host.manager', side_effect=TimeoutError('manager timed out')), self.assertRaises(Refused):
            evidence.require_recovered(self.lab, writing=True)
        with patch('runner.host.manager', side_effect=AssertionError('reader asked manager')), self.assertRaises(Refused):
            evidence.require_recovered(self.lab)
        self.assertTrue((self.lab.root / self.base / 'live.json').exists())
        self.lab.remove_live(self.base + '/live.json')
        self.marker.pop('context')
        self.live()
        with self.assertRaisesRegex(Refused, 'owner recovery required'):
            evidence.require_recovered(self.lab, writing=True)

    def test_rehearsal_error_never_publishes(self):
        self.base = 'rehearsals/sample/' + self.record['run']
        self.record['rehearsal'] = dict(script=self.record['key'])
        self.live()
        with patch('runner.host.manager', return_value='LoadState=not-found\nActiveState=inactive'):
            evidence.require_recovered(self.lab, writing=True)
        self.assertEqual(canonical.loads(self.lab.read(self.base + '/run.json'))['outcome'], 'error')
        self.assertFalse((self.lab.root / self.key).exists())


    def test_triage_cut_after_appends_keeps_batch_once_and_records_error(self):
        from runner import finding_store, triage
        self.publication_inputs()
        self.record['outcome'] = 'fail'
        self.record['failures'] = [dict(id='failure-1', signature='check/planted', kind='check', summary='planted', evidence=[])]
        evidence.publish(self.lab, self.key, self.record)
        history, runs, _paths, facts = finding_store.load(self.lab)
        supplied = triage.input_value(self.record, history, runs)
        output = dict(failures=[dict(id='failure-1', match=None, signature=None, title='Proposal',
                                    reason='Evidence', **{'class': 'tools'})])
        additions = triage.additions(supplied, output, history, runs, self.record['ended'], facts)
        finding_store.append_triage(self.lab, additions)
        before = self.lab.read('findings/F-0001.jsonl')
        self.base += '/triage/1'
        self.marker['context'] = dict(format='lab-triage/1', run=self.record['run'], key=self.record['key'], n=1,
            started=self.record['started'], ended=self.record['ended'], outcome='error', invocation=None,
            expected_inventory=None, inventory=None, probe=None, input=None, output=None,
            bounds=dict(**self.record['bounds'], triage_seconds=2, triage_tokens=100),
            usage=dict(seconds=None, turns=0, tokens=dict(input=None, cached=None, output=None), tokens_overshoot=None),
            errors=[], finding_events=[], brake=None)
        self.live()
        with patch('runner.host.manager', return_value='LoadState=not-found\nActiveState=inactive'):
            evidence.require_recovered(self.lab, writing=True)
            evidence.require_recovered(self.lab, writing=True)
        saved = canonical.loads(self.lab.read(self.base + '/triage.json'))
        self.assertEqual(saved['outcome'], 'error')
        self.assertEqual(saved['errors'][0]['what'], 'interrupted')
        self.assertEqual(saved['finding_events'], [])
        self.assertEqual(self.lab.read('findings/F-0001.jsonl'), before)
