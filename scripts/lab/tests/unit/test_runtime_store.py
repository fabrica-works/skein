"""Immutable, additive publication and refusing unsafe runtime admissions."""
from contextlib import redirect_stderr, redirect_stdout
import io
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from runner import canonical, cli, evidence, judge, records, store
from runner.records import Refused
from .fixtures import D


class Timeline(unittest.TestCase):
    def test_exit_event_retains_observed_time_instead_of_resampling_it(self):
        from types import SimpleNamespace
        from runner.loop import Run
        written = []
        run = Run.__new__(Run)
        run.base = 'run'
        run.lab = SimpleNamespace(append_line=lambda path, value: written.append((path, value)))
        run.ms = lambda: 99
        run.timeline('exit', ms=17, name='server', status=101)
        run.timeline('phase', phase='stop')
        self.assertEqual(written[0], ('run/events.jsonl', dict(format='lab-event/1', event='exit',
                                                            ms=17, name='server', status=101)))
        self.assertEqual(written[1][1]['ms'], 99)

    def test_awaited_event_and_record_share_observed_time_and_capture(self):
        from types import SimpleNamespace
        from runner.loop import Run
        written = []
        run = Run.__new__(Run)
        run.base = 'run'
        run.lab = SimpleNamespace(append_line=lambda path, value: written.append(value))
        run.ms = lambda: 99
        run.state = SimpleNamespace(data={'bounds': []})
        command = dict(name='refresh', group='actions', exit=None, awaited=None)
        ordinary = dict(name='other', group='actions', exit=None)
        run.command_records = [command, ordinary]
        run.items = {}
        run.effect(dict(op='awaited', name='refresh', ms=17, captures={'word': 'invented-word'}))
        self.assertEqual(command['awaited'], dict(ms=17, captures={'word': 'invented-word'}))
        self.assertEqual(written[0], dict(format='lab-event/1', event='awaited', ms=17,
                                         name='refresh', captures={'word': 'invented-word'}))
        self.assertNotIn('awaited', ordinary)
        command.update(exit=dict(status=1, stopped=False, killed=False), awaited=None)
        run.effect(dict(op='awaited', name='refresh', ms=18, captures={'word': 'late'}))
        self.assertIsNone(command['awaited'])
        command.update(exit=dict(status=0, stopped=False, killed=False), awaited=dict(ms=17, captures={'word': 'invented-word'}))
        run.failures = []
        run.state.data['bounds'].append(dict(name='command_seconds', who='refresh', ms=19))
        run.effect(dict(op='bound', name='command_seconds', who='refresh'))
        self.assertIsNone(command['awaited'])
        run.effect(dict(op='awaited', name='refresh', ms=18, captures={'word': 'late'}))
        self.assertIsNone(command['awaited'])
        self.assertNotIn('awaited', ordinary)


def run_record(number, seed, outcome='pass', attempt=1):
    return dict(format='lab-run/1', run=f'20261020T123456000Z-{number:06x}', key=D, experiment='sample',
                seed=seed, attempt=attempt, campaign=None, started='2026-10-20T12:34:56.000Z',
                ended='2026-10-20T12:34:56.001Z', outcome=outcome, schedule={}, processes={}, commands=[],
                driver=None, failures=[], undecided=[], errors=[], bounds=[], usage={'driver': None})


class Evidence(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.lab = store.open_lab(Path(self.temp.name) / 'lab')
        self.key_path = 'keys/sample/' + D
        os.close(self.lab.directory(self.key_path, create=True))

    def tearDown(self):
        self.lab.close()
        for directory, _, _ in os.walk(self.temp.name):
            os.chmod(directory, 0o700)
        self.temp.cleanup()

    def publish(self, record):
        with self.lab.lock():
            evidence.publish(self.lab, self.key_path, record)

    def test_optional_awaited_record_roundtrip_and_refusals_preserve_ordinary_records(self):
        ordinary = dict(group='actions', name='ordinary', exit=dict(status=0))
        awaiting = dict(group='actions', name='refresh', awaited=None)
        record = run_record(1, '1')
        record['commands'] = [ordinary, awaiting]
        self.assertEqual(records.run_refusals(record), [])
        awaiting['awaited'] = dict(ms=17, captures={'word': 'invented-word'})
        self.publish(record)
        saved = canonical.loads(self.lab.read(self.key_path + '/runs/' + record['run'] + '/run.json'))
        self.assertEqual(saved['commands'], record['commands'])
        self.assertNotIn('awaited', saved['commands'][0])
        for value, member in (({}, 'ms'), (dict(ms=-1, captures={'word': 'value'}), 'ms'),
                               (dict(ms=True, captures={'word': 'value'}), 'ms'),
                               (dict(ms=1, captures={}), 'captures'),
                               (dict(ms=1, captures={'Bad': 'value'}), 'captures'),
                               (dict(ms=1, captures={'word': None}), 'captures'),
                               (dict(ms=1, capture='value'), 'capture')):
            awaiting['awaited'] = value
            with self.subTest(value=value):
                self.assertIn('run.commands.1.awaited.' + member, [r.member for r in records.run_refusals(record)])

    def test_bootstrap_records_roundtrip_and_refuse_private_file_references(self):
        record = run_record(1, '1')
        source = dict(bootstrap=dict(stream='stdout', action='sign-in', received_ms=None, delivered_ms=None),
                      output=dict(stdout=None, stderr=dict(path='proc/server/stderr')))
        recipient = dict(group='actions', name='sign-in', bootstrap=dict(process='server'),
                         stdin=None, output=dict(stdout=None, stderr=None))
        record['processes'] = dict(server=source, ordinary=dict(output={}))
        record['commands'] = [recipient, dict(group='actions', name='ordinary')]
        self.assertEqual(records.run_refusals(record), [])
        source['bootstrap'].update(received_ms=17, delivered_ms=19)
        self.publish(record)
        saved = canonical.loads(self.lab.read(self.key_path + '/runs/' + record['run'] + '/run.json'))
        self.assertEqual(saved['processes'], record['processes'])
        self.assertEqual(saved['commands'], record['commands'])
        self.assertNotIn('bootstrap', saved['processes']['ordinary'])
        self.assertNotIn('bootstrap', saved['commands'][1])
        for field in ('received_ms', 'delivered_ms'):
            for value in (-1, True, [], '17'):
                source['bootstrap'][field] = value
                self.assertIn('run.processes.server.bootstrap.' + field,
                              [r.member for r in records.run_refusals(record)])
            source['bootstrap'][field] = 17
        for field, value in (('stream', 'stderr'), ('action', []), ('unknown', True)):
            source['bootstrap'][field] = value
            self.assertIn('run.processes.server.bootstrap.' + field,
                          [r.member for r in records.run_refusals(record)])
        source['output']['stdout'] = dict(path='forbidden')
        recipient.update(group='collect', stdin=dict(path='forbidden'))
        recipient['bootstrap'] = dict(process=[], unknown=True)
        recipient['output'] = dict(stdout=dict(path='forbidden'), stderr=dict(path='forbidden'))
        members = {r.member for r in records.run_refusals(record)}
        self.assertTrue({'run.processes.server.output.stdout', 'run.commands.0.bootstrap',
                         'run.commands.0.bootstrap.process', 'run.commands.0.bootstrap.unknown',
                         'run.commands.0.stdin', 'run.commands.0.output.stdout', 'run.commands.0.output.stderr'} <= members)

    def test_stopped_command_records_roundtrip_without_default_keys(self):
        record = run_record(1, '1')
        record['commands'] = [dict(group='collect', name='live'),
                              dict(group='collect', name='copy', cluster='stopped', exit=dict(status=0)),
                              dict(group='checks', name='state', cluster='stopped', skipped='input unavailable')]
        self.publish(record)
        saved = canonical.loads(self.lab.read(self.key_path + '/runs/' + record['run'] + '/run.json'))
        self.assertEqual(saved['commands'], record['commands'])
        self.assertNotIn('cluster', saved['commands'][0])
        for group, value in (('collect', None), ('collect', []), ('collect', 'up'), ('actions', 'stopped')):
            record['commands'][1].update(group=group, cluster=value)
            self.assertIn('run.commands.1.cluster', [r.member for r in records.run_refusals(record)])

    def test_three_seeds_and_additive_rerun_replace_only_derived_verdict(self):
        old_records = []
        for n in range(1, 4):
            self.publish(run_record(n, str(n)))
            old_records.append(self.lab.read(self.key_path + f'/runs/20261020T123456000Z-{n:06x}/run.json'))
            derived = canonical.loads(self.lab.read(self.key_path + '/verdict.json'))
            self.assertEqual(derived['verdict'], 'pass' if n == 3 else 'not enough evidence')
        before = self.lab.read(self.key_path + '/evidence.jsonl')
        with self.assertRaises(Refused):
            self.publish(run_record(4, '4'))
        self.assertEqual(self.lab.read(self.key_path + '/evidence.jsonl'), before)
        rerun = run_record(4, '1', 'fail', 2)
        rerun['failures'] = [dict(id='failure-1', signature='check/end/state', kind='check', summary='absent', evidence=[])]
        self.publish(rerun)
        self.assertTrue(self.lab.read(self.key_path + '/evidence.jsonl').startswith(before))
        derived = canonical.loads(self.lab.read(self.key_path + '/verdict.json'))
        self.assertEqual(derived['verdict'], 'flaky')
        for n, data in enumerate(old_records, 1):
            self.assertEqual(data, self.lab.read(self.key_path + f'/runs/20261020T123456000Z-{n:06x}/run.json'))
        self.assertEqual(len(evidence.read(self.lab, self.key_path)), 4)

    def test_error_and_incomplete_are_retained_until_explicit_rerun(self):
        self.publish(run_record(1, '1', 'error'))
        records = evidence.read(self.lab, self.key_path)
        self.assertEqual(judge.next_seed(records), dict(action='blocked', seed='1'))
        self.publish(run_record(2, '1', 'pass', 2))
        self.assertEqual(judge.next_seed(evidence.read(self.lab, self.key_path))['action'], 'draw')
        self.publish(run_record(3, '2', 'incomplete'))
        self.assertEqual(judge.next_seed(evidence.read(self.lab, self.key_path)), dict(action='blocked', seed='2'))
        with self.assertRaises(Refused):
            self.publish(run_record(4, '2', 'pass', 1))
        self.assertEqual(len(evidence.read(self.lab, self.key_path)), 3)

    def test_missing_or_corrupt_run_is_never_accepted_as_a_pass(self):
        item = evidence.line_for(run_record(1, '1'))
        self.lab.append_line(self.key_path + '/evidence.jsonl', item)
        with self.assertRaises(FileNotFoundError):
            evidence.read(self.lab, self.key_path)
        self.lab.write_once(self.key_path + '/runs/' + item['run'] + '/run.json', canonical.record_bytes(run_record(1, '2')))
        with self.assertRaises(Refused):
            evidence.read(self.lab, self.key_path)

    def test_derived_write_refuses_links_and_immutable_targets(self):
        target = self.lab.root / 'safe'
        target.write_bytes(b'safe')
        (self.lab.root / self.key_path / 'verdict.json').symlink_to(target)
        with self.assertRaises(Refused):
            self.lab.replace_derived(self.key_path + '/verdict.json', dict(format='lab-verdict/1'))
        with self.assertRaises(Refused):
            self.lab.replace_derived(self.key_path + '/run.json', {})
        self.assertEqual(target.read_bytes(), b'safe')

    def test_unfinished_run_is_retained_and_named_for_owner_recovery(self):
        path = self.key_path + '/runs/20261020T123456000Z-000001/live.json'
        self.lab.write_once(path, canonical.record_bytes(dict(format='lab-live/1', unit='lab-canary.service')))
        with self.assertRaisesRegex(Refused, 'owner recovery required'):
            evidence.require_recovered(self.lab)
        self.assertEqual(canonical.loads(self.lab.read(path))['unit'], 'lab-canary.service')

    def test_cli_lock_refusal_is_not_a_successful_noop(self):
        with self.lab.lock(), redirect_stderr(io.StringIO()) as error, redirect_stdout(io.StringIO()) as output:
            code = cli.main(['run', '/unused', '--subject', D, '--seed', '1'], lab_root=self.lab.root)
        self.assertEqual(code, 2)
        self.assertIn('another lab writer', error.getvalue())
        self.assertEqual(output.getvalue(), '')

    def test_judge_runs_three_new_seeds_once_and_stops_on_unjudged_evidence(self):
        from types import SimpleNamespace
        from runner import transport
        args = SimpleNamespace(command='judge', campaign=None, experiment='/experiment', subject=D)
        def launch(_lab, _fd, key_path, seed, _campaign):
            number = len(evidence.read(self.lab, key_path)) + 1
            record = run_record(number, seed)
            evidence.publish(self.lab, key_path, record)
            return record, 0
        with patch.object(cli, '_experiment_input', return_value=({'name': 'sample', 'driver': None}, [])), \
             patch.object(cli, 'validate_command', return_value=(D, [])), \
             patch.object(cli, '_new_seed', side_effect=['1', '2', '3']), \
             patch.object(transport, 'launch', side_effect=launch) as launched, \
             redirect_stdout(io.StringIO()), self.lab.lock() as fd:
            self.assertEqual(cli.runtime_command(self.lab, fd, args), 0)
            self.assertEqual(launched.call_count, 3)
            self.assertEqual(cli.runtime_command(self.lab, fd, args), 0)
            self.assertEqual(launched.call_count, 3)

    def test_judge_does_not_automatically_retry_error_or_run_another_seed(self):
        from types import SimpleNamespace
        from runner import transport
        args = SimpleNamespace(command='judge', campaign=None, experiment='/experiment', subject=D)
        def launch(_lab, _fd, key_path, seed, _campaign):
            record = run_record(1, seed, 'error')
            evidence.publish(self.lab, key_path, record)
            return record, 4
        with patch.object(cli, '_experiment_input', return_value=({'name': 'sample', 'driver': None}, [])), \
             patch.object(cli, 'validate_command', return_value=(D, [])), \
             patch.object(cli, '_new_seed', return_value='1'), \
             patch.object(transport, 'launch', side_effect=launch) as launched, \
             redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()), self.lab.lock() as fd:
            self.assertEqual(cli.runtime_command(self.lab, fd, args), 4)
            self.assertEqual(cli.runtime_command(self.lab, fd, args), 3)
            self.assertEqual(launched.call_count, 1)
