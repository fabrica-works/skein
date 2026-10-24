"""Terminal host failures retain delegated fallback ownership and return once."""
import os
from pathlib import Path
import tempfile
import unittest
from runner import canonical, evidence, store
from runner.loop import Run
from tests.unit.fixtures import fixture


class CallBudget(BaseException):
    """A livelocked runner must escape its own Exception handler and fail."""


class PopulatedHost:
    def __init__(self, _unit, _systemctl, _cost):
        self.children = {}
        self.calls = 0
        self.kills = []

    def counts(self, _identity):
        self.calls += 1
        if self.calls > 12:
            raise CallBudget('terminal host error did not return within the supplied call budget')
        return dict(populated=True, tree_peak_bytes=0, pids=1)

    def kill_tree(self, identity):
        self.kills.append(identity)


class TerminalErrors(unittest.TestCase):
    def test_populated_tree_in_terminal_phases_returns_and_copies_nothing(self):
        for phase in ('collect', 'check', 'stop', 'stopped-collect', 'stopped-check', 'judge', 'closed'):
            with self.subTest(phase=phase), tempfile.TemporaryDirectory() as temporary:
                with store.open_lab(Path(temporary) / 'lab') as lab:
                    exp, subject, _settings, _facts, inputs = fixture()
                    lab.write_once('settings.json', canonical.record_bytes(_settings))
                    exp.update(processes=[], actions=[], collect=[], checks=[])
                    for group in ('collect', 'checks'):
                        exp[group] = [dict(name='pending', cluster='stopped', argv=['/never-start'], env={})]
                    exp['collect'][0]['scan'] = []
                    subject_id = canonical.digest(subject)
                    lab.capture('subjects', subject_id,
                                [dict(path='subject.json', data=canonical.record_bytes(subject), exec=False)])
                    exp_id = canonical.digest(exp)
                    lab.capture('experiments', exp_id,
                                [dict(path='experiment.json', data=canonical.record_bytes(exp), exec=False)])
                    key = dict(format='lab-key/1', experiment=exp_id, subject=subject_id, lab=inputs)
                    identity = canonical.digest(key)
                    key_path = 'keys/hello/' + identity
                    lab.write_once(key_path + '/key.json', canonical.record_bytes(key))
                    spec = dict(key=identity, key_path=key_path, run='20261023T052500000Z-000001',
                                seed='1', attempt=1, campaign=None, unit='test-owned.service', systemctl='/unused',
                                bounds={'name': 'smoke', **_settings['costs']['smoke']})
                    run = Run(lab, spec, host_factory=PopulatedHost)
                    run.ms = lambda: 1
                    group = 'checks' if phase in ('check', 'stopped-check') else 'collect'
                    command = dict(group=group, name='active', cluster='stopped',
                                   memory=dict(tree_peak_bytes=None), output={}, exit=dict(status=0, stopped=False, killed=False))
                    def admitted():
                        run.effects.clear()
                        run.host = run.host_factory(spec['unit'], spec['systemctl'], run.cost)
                        child = dict(reaped=True, streams={}, exec_ack=None)
                        run.host.children['active'] = child
                        run.items['active'] = dict(name='active', group=group, record=command, cleanup=0,
                                                   kill_deadline=None, cancelled=False, timed_out=False)
                        run.command_records.append(command)
                        run.state.data.update(phase=phase, current='active',
                                              started_commands=[dict(group=group, name='active')])
                    record = run.run(admitted)
                    self.assertEqual(record['outcome'], 'error')
                    self.assertLessEqual(run.host.calls, 3)
                    self.assertIn('active', run.host.children)
                    if phase in ('collect', 'check', 'stopped-collect', 'stopped-check'):
                        self.assertTrue(run.items['active']['cancelled'])
                    self.assertTrue((lab.root / run.base / 'live.json').is_file())
                    self.assertTrue(any(e['what'] == 'containment/active' for e in record['errors']))
                    pending = [c for c in record['commands'] if c['name'] == 'pending']
                    self.assertEqual(len(pending), 2)
                    self.assertTrue(all('skipped' in c and 'exit' not in c for c in pending))
                    self.assertEqual({u['check'] for u in record['undecided'] if 'pending' in u['check']},
                                     {'collect/pending', 'checks/pending'})
                    events = [canonical.loads(line + b'\n') for line in lab.read(run.base + '/events.jsonl').splitlines()]
                    self.assertFalse(any(e['event'] in ('tree_empty', 'start') for e in events))
                    self.assertEqual(evidence.read(lab, key_path)[0]['outcome'], 'error')
                    with self.assertRaisesRegex(Exception, 'owner recovery required'):
                        evidence.require_recovered(lab)
                for directory, _, _ in os.walk(temporary):
                    os.chmod(directory, 0o700)
