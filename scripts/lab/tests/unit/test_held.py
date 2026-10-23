"""Held metadata, exclusive handoffs and release decisions with supplied times."""
from contextlib import redirect_stderr, redirect_stdout
import errno
import io
import os
from pathlib import Path
import stat
import tempfile
import unittest
from unittest.mock import patch
from runner import canonical, checks, cli, experiment, held, life, records, store
from runner.loop import Run
from runner.records import Refused
from .fixtures import fixture
from .test_runtime_store import run_record

TAKEN = '2026-10-23T12:34:56.000Z'
RUN = '20261023T123456000Z-000001'
BASE = 'runs/' + RUN


def validation_world():
    exp, subject, settings, facts, _lab = fixture()
    exp.update(held=['token'], release=[dict(name='return', argv=['/usr/bin/helper', '{held.token}'], env={})])
    exp['actions'][0]['argv'].append('{held.token}')
    facts['held'] = {'token': 'kept'}
    return exp, subject, settings, facts


class Validation(unittest.TestCase):
    def test_optional_members_are_not_inserted_into_experiment(self):
        exp, subject, settings, facts, _ = fixture()
        before = canonical.record_bytes(exp)
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])
        self.assertEqual(canonical.record_bytes(exp), before)

    def test_held_and_release_names_types_and_duplicates_refuse(self):
        for field, value, member in (
                ('held', None, 'held'), ('held', ['Bad'], 'held.0'), ('held', ['a_b'], 'held.0'),
                ('held', ['1a'], 'held.0'), ('held', [''], 'held.0'), ('held', [True], 'held.0'),
                ('held', ['token', 'token'], 'held.1'), ('release', {}, 'release'),
                ('release', [dict(name='BAD', argv=[], env={})], 'release.0.name')):
            exp, subject, settings, facts = validation_world()
            exp[field] = value
            with self.subTest(field=field, value=value):
                self.assertIn(member, [r.member for r in experiment.validate(exp, subject, settings, facts)])
        exp, subject, settings, facts = validation_world()
        exp['release'] *= 2
        self.assertIn('release.1.name', [r.member for r in experiment.validate(exp, subject, settings, facts)])

    def test_missing_taken_and_invalid_files_name_the_held_refusal(self):
        for status in ('not held', 'taken by run ' + RUN, 'expected an owned regular 0600 file with one link'):
            exp, subject, settings, facts = validation_world()
            facts['held']['token'] = status
            self.assertIn(records.Refusal('held.token', status), experiment.validate(exp, subject, settings, facts))
        exp, subject, settings, facts = validation_world()
        del facts['held']
        self.assertIn(records.Refusal('held.token', 'not held'), experiment.validate(exp, subject, settings, facts))

    def test_held_templates_are_known_only_in_seed_and_release_argv(self):
        for group, field, member in (
                ('actions', 'env', 'actions.0.env.TOKEN'), ('actions', 'stdin', 'actions.0.stdin'),
                ('release', 'env', 'release.0.env.TOKEN'), ('release', 'stdin', 'release.0.stdin'),
                ('processes', 'argv', 'processes.0.argv.1'), ('collect', 'argv', 'collect.0.argv.1'),
                ('checks', 'argv', 'checks.0.argv.1')):
            exp, subject, settings, facts = validation_world()
            item = exp[group][0]
            if field == 'argv':
                item['argv'][1] = '{held.token}'
            elif field == 'env':
                item['env']['TOKEN'] = '{held.token}'
            else:
                item['stdin'] = '{held.token}'
            with self.subTest(member=member):
                self.assertIn(records.Refusal(member, 'unknown or unavailable template: held.token'),
                              experiment.validate(exp, subject, settings, facts))
        exp, subject, settings, facts = validation_world()
        facts['experiment_files']['app.conf'] = '{held.token}'
        self.assertIn('processes.2.files.0.from', [r.member for r in experiment.validate(exp, subject, settings, facts)])
        exp, subject, settings, facts = validation_world()
        exp['driver'] = dict(charter='charter', tools=[dict(name='read', argv=['/usr/bin/helper', '{held.token}'],
                                                        env={}, reads=[], writes=[])])
        self.assertIn(records.Refusal('driver.tools.0.argv.1', 'unknown or unavailable template: held.token'),
                      experiment.validate(exp, subject, settings, facts))

    def test_unknown_held_name_and_release_await_and_cluster_refuse(self):
        for group in ('actions', 'release'):
            exp, subject, settings, facts = validation_world()
            exp[group][0]['argv'].append('{held.missing}')
            self.assertIn('unknown or unavailable template: held.missing',
                          [r.why for r in experiment.validate(exp, subject, settings, facts)])
        for field, value in (('await', {}), ('cluster', 'up')):
            exp, subject, settings, facts = validation_world()
            exp['release'][0][field] = value
            self.assertIn('release.0.' + field, [r.member for r in experiment.validate(exp, subject, settings, facts)])

    def test_release_knows_seed_action_inputs_and_no_collection(self):
        exp, subject, settings, facts = validation_world()
        exp['actions'][0]['await'] = dict(process='backend', stream='stdout', prefix='fresh ', capture='word')
        exp['release'][0]['argv'] += ['{proc.backend.ready.address}', '{proc.app.dir}', '{dir.client}',
                                     '{action.seed.word}']
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])
        # The release runs as seed actions do (lab.md 4.8); a collection is known in checks alone (4.2).
        for cluster in ('up', 'stopped'):
            with self.subTest(cluster=cluster):
                exp['collect'][0]['cluster'] = cluster
                exp['release'][0]['argv'][6:] = ['{collected.obs}']
                self.assertIn('release.0.argv.6', [r.member for r in experiment.validate(exp, subject, settings, facts)])


def release_world(phase='check', live=True):
    exp, _subject, settings, _facts = validation_world()
    exp['release'].append(dict(name='settle', argv=['/usr/bin/helper'], env={}))
    state = life.initial(exp, '1', settings['costs']['smoke'], {'held.token': '/run/held/token'}, {})
    state.data.update(phase=phase, current='end' if phase == 'check' else 'seed', order=['backend'],
                      processes={'backend': dict(exit=None if live else dict(status=1), stopping=False)},
                      index=0)
    return state


def send(state, kind, ms=17, **fields):
    return life.step(state, dict(kind=kind, ms=ms, **fields))


def complete(state, name, status=0):
    state, _ = send(state, 'spawned', group='release', name=name)
    return send(state, 'command_done', name=name,
                record=dict(group='release', name=name, exit=dict(status=status)))


class ReleaseLife(unittest.TestCase):
    def test_release_follows_live_checks_then_gives_before_stop(self):
        state = release_world()
        state, effects = send(state, 'command_done', name='end', record=dict(group='checks'))
        self.assertEqual(state.data['phase'], 'release')
        self.assertEqual(effects[-1]['argv'], ['/usr/bin/helper', '/run/held/token'])
        state, effects = complete(state, 'return')
        self.assertEqual(effects[-1]['name'], 'settle')
        state, effects = complete(state, 'settle')
        self.assertEqual(effects[:2], [dict(op='give_held'), dict(op='phase', phase='stop')])
        self.assertEqual(effects[-1], dict(op='signal_first', name='backend', signal='SIGTERM'))

    def test_release_after_cuts_in_start_seed_and_drive(self):
        for phase in ('start', 'seed', 'drive'):
            state = release_world(phase)
            state.data['exp']['collect'] = []
            state.data['exp']['checks'] = []
            state.data['order'].append('other')
            state.data['processes']['other'] = dict(exit=None, stopping=False)
            state, effects = send(state, 'exit', name='other', result=dict(status=1))
            with self.subTest(phase=phase):
                self.assertEqual(state.data['phase'], 'release')
                self.assertEqual([e['phase'] for e in effects if e['op'] == 'phase'], ['collect', 'check', 'release'])
                self.assertEqual(effects[-1]['group'], 'release')

    def test_interrupt_enters_release_and_second_interrupt_cannot_cancel_it(self):
        state = release_world('seed')
        state, effects = send(state, 'interrupt', signal=15)
        self.assertEqual(state.data['phase'], 'release')
        self.assertIn(dict(op='cancel_command', name='seed'), effects)
        state, _ = send(state, 'spawned', group='release', name='return')
        deadline = state.data['deadline']
        state, effects = send(state, 'interrupt', signal=2)
        self.assertEqual(effects, [])
        self.assertEqual(state.data['deadline'], deadline)
        self.assertEqual(state.data['current'], 'return')

    def test_machinery_errors_skip_release_but_still_request_give_back(self):
        for what in ('containment', 'runner', 'spawn/seed'):
            state, effects = send(release_world('seed'), 'error', what=what)
            self.assertEqual(state.data['phase'], 'stop')
            skipped = [e for e in effects if e['op'] == 'unavailable']
            self.assertEqual([e['name'] for e in skipped], ['return', 'settle'])
            self.assertTrue(all('machinery error' in e['reason'] for e in skipped))
            self.assertIn(dict(op='give_held'), effects)

    def test_machinery_error_during_release_skips_pending_commands(self):
        state, _ = send(release_world(), 'command_done', name='end', record=dict(group='checks'))
        state, _ = send(state, 'spawned', group='release', name='return')
        state, effects = send(state, 'error', what='spawn/return')
        self.assertIn(dict(op='cancel_command', name='return'), effects)
        self.assertEqual([e['name'] for e in effects if e['op'] == 'unavailable'], ['return', 'settle'])
        self.assertIn(dict(op='give_held'), effects)
        self.assertEqual(state.data['phase'], 'stop')

    def test_first_interrupt_in_release_keeps_command_and_deadline(self):
        state, _ = send(release_world(), 'command_done', name='end', record=dict(group='checks'))
        state, _ = send(state, 'spawned', group='release', name='return')
        deadline = state.data['deadline']
        state, effects = send(state, 'interrupt', signal=15)
        self.assertEqual((state.data['phase'], state.data['current'], state.data['deadline']),
                         ('release', 'return', deadline))
        self.assertEqual(effects, [])

    def test_no_live_process_skips_every_release_command(self):
        state, effects = send(release_world(live=False), 'command_done', name='end', record=dict(group='checks'))
        skipped = [e for e in effects if e['op'] == 'unavailable']
        self.assertEqual([e['name'] for e in skipped], ['return', 'settle'])
        self.assertTrue(all('no started process is alive' in e['reason'] for e in skipped))

    def test_failed_release_runs_next_and_timeout_keeps_its_bound(self):
        state, _ = send(release_world(), 'command_done', name='end', record=dict(group='checks'))
        state, effects = complete(state, 'return', status=7)
        self.assertEqual(effects[-1]['name'], 'settle')
        state, _ = send(state, 'spawned', group='release', name='settle')
        deadline = state.data['deadline']
        state, effects = send(state, 'tick', ms=deadline - 1)
        self.assertEqual(effects, [])
        state, effects = send(state, 'tick', ms=deadline)
        self.assertEqual(effects, [dict(op='bound', name='command_seconds', who='settle'),
                                   dict(op='cancel_command', name='settle')])
        state, _ = send(state, 'command_done', name='settle', record=dict(group='release'))
        self.assertEqual(state.data['phase'], 'stop')

    def test_release_exit_and_signal_have_release_signatures(self):
        for end, suffix in ((dict(status=7, stopped=False), 'exit:7'),
                            (dict(signal='SIGABRT', stopped=False), 'signal:SIGABRT')):
            record = dict(exit=end, output=dict(stderr=dict(path='commands/release-return.stderr')))
            self.assertEqual(checks.release('return', record)[0]['signature'], 'release/return/' + suffix)
            record['exit']['stopped'] = True
            self.assertEqual(checks.release('return', record), [])

    def test_late_release_completion_records_command_bound(self):
        state, _ = send(release_world(), 'command_done', name='end', record=dict(group='checks'))
        state, _ = send(state, 'spawned', group='release', name='return')
        state, effects = send(state, 'command_done', ms=state.data['deadline'], name='return',
                              record=dict(group='release', name='return', exit=dict(status=0)))
        self.assertIn(dict(op='bound', name='command_seconds', who='return'), effects)
        self.assertEqual(effects[-1]['name'], 'settle')


class HeldStore(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.lab = store.open_lab(self.root / 'lab')
        self.bytes = os.urandom(48)
        self.source = self.root / 'stand-in'
        self.source.write_bytes(self.bytes)

    def tearDown(self):
        self.lab.close()
        for path, _, _ in os.walk(self.root):
            os.chmod(path, 0o700)
        self.temp.cleanup()

    def cli(self, *args):
        output, errors = io.StringIO(), io.StringIO()
        with redirect_stdout(output), redirect_stderr(errors):
            status = cli.main(['held', *args], lab_root=self.lab.root)
        return status, output.getvalue(), errors.getvalue()

    def take(self):
        held.put(self.lab, 'token', self.source)
        held.take(self.lab, BASE, 'token', RUN, TAKEN)

    def loop(self):
        run = Run.__new__(Run)
        run.lab, run.base = self.lab, BASE
        run.ms = lambda: 17
        run.held_records = [dict(name='token', taken_ms=1, given_ms=None)]
        run.failures = []
        return run

    def test_cli_kept_taken_settled_lines_and_private_modes(self):
        self.assertEqual(self.cli(), (0, '', ''))
        self.assertEqual(self.cli('put', 'token', str(self.source)), (0, 'token kept\n', ''))
        self.assertFalse(self.source.exists())
        self.assertEqual(stat.S_IMODE((self.lab.root / 'held').stat().st_mode), 0o700)
        self.assertEqual(stat.S_IMODE((self.lab.root / 'held/token').stat().st_mode), 0o600)
        held.take(self.lab, BASE, 'token', RUN, TAKEN)
        self.assertEqual(self.cli(), (0, f'token taken {RUN} {TAKEN}\n', ''))
        self.assertEqual(held.availability(self.lab, ['token']), {'token': 'taken by run ' + RUN})
        returned = self.lab.root / BASE / 'held/token'
        self.assertEqual(self.cli('put', 'token', str(returned)), (0, 'token kept\n', ''))
        self.assertEqual(self.cli(), (0, 'token kept\n', ''))
        self.assertFalse((self.lab.root / 'held/token.taken').exists())

    def test_cli_sorted_names_and_already_kept_refusal_preserve_source(self):
        held.put(self.lab, 'zulu', self.source)
        self.source.write_bytes(self.bytes)
        held.put(self.lab, 'alpha', self.source)
        self.assertEqual(self.cli(), (0, 'alpha kept\nzulu kept\n', ''))
        self.source.write_bytes(self.bytes)
        status, output, errors = self.cli('put', 'alpha', str(self.source))
        self.assertEqual((status, output), (2, ''))
        self.assertIn('already kept', errors)
        self.assertEqual(self.source.read_bytes(), self.bytes)

    def test_cli_refuses_invalid_names_symlinks_directories_and_hardlinks(self):
        for name in ('BAD', '../escape', '', 'has.dot'):
            self.assertEqual(self.cli('put', name, str(self.source))[0], 2)
        link = self.root / 'link'
        link.symlink_to(self.source)
        for source in (link, self.root):
            self.assertEqual(self.cli('put', 'token', str(source))[0], 2)
        link.unlink()
        os.link(self.source, link)
        self.assertEqual(self.cli('put', 'token', str(link))[0], 2)
        self.assertEqual(self.source.read_bytes(), self.bytes)

    def test_take_writes_record_before_rename_without_reading_file(self):
        held.put(self.lab, 'token', self.source)
        with patch.object(self.lab, 'read', side_effect=AssertionError('held bytes read')):
            held.take(self.lab, BASE, 'token', RUN, TAKEN)
        self.assertEqual(held.taken_record(self.lab, 'token'), dict(format='lab-held/1', name='token', run=RUN, taken=TAKEN))
        self.assertEqual((self.lab.root / BASE / 'held/token').read_bytes(), self.bytes)
        self.assertEqual(stat.S_IMODE((self.lab.root / BASE / 'held').stat().st_mode), 0o700)

    def test_failed_take_leaves_taken_record_and_original_file(self):
        held.put(self.lab, 'token', self.source)
        with patch.object(self.lab, 'move', side_effect=OSError('rename refused')):
            with self.assertRaises(OSError):
                held.take(self.lab, BASE, 'token', RUN, TAKEN)
        self.assertEqual(held.taken_record(self.lab, 'token')['run'], RUN)
        self.assertEqual((self.lab.root / 'held/token').read_bytes(), self.bytes)

    def test_give_back_records_time_and_event_without_reading_file(self):
        self.take()
        run = self.loop()
        with patch.object(self.lab, 'read', side_effect=AssertionError('held bytes read')):
            run.give_held()
        self.assertEqual(run.held_records, [dict(name='token', taken_ms=1, given_ms=17)])
        self.assertEqual(run.failures, [])
        self.assertEqual((self.lab.root / 'held/token').read_bytes(), self.bytes)
        self.assertFalse((self.lab.root / 'held/token.taken').exists())
        event = canonical.loads(self.lab.read(BASE + '/events.jsonl'))
        self.assertEqual(event, dict(format='lab-event/1', event='held_given', name='token', ms=17))

    def test_public_file_fails_and_is_never_moved_or_chmodded(self):
        self.take()
        path = self.lab.root / BASE / 'held/token'
        path.chmod(0o644)
        run = self.loop()
        run.give_held()
        self.assertEqual(run.failures[0]['signature'], 'release/held:token')
        self.assertEqual(run.failures[0]['kind'], 'release')
        self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o644)
        self.assertEqual(path.read_bytes(), self.bytes)
        self.assertIsNone(run.held_records[0]['given_ms'])
        self.assertTrue((self.lab.root / 'held/token.taken').exists())

    def test_missing_file_fails_and_keeps_taken(self):
        self.take()
        (self.lab.root / BASE / 'held/token').unlink()
        run = self.loop()
        run.give_held()
        self.assertEqual(run.failures[0]['signature'], 'release/held:token')
        self.assertEqual(held.availability(self.lab, ['token'])['token'], 'taken by run ' + RUN)

    def test_links_special_files_and_wrong_owner_are_not_valid(self):
        self.take()
        path = self.lab.root / BASE / 'held/token'
        path.rename(self.source)
        for make in (lambda: path.symlink_to(self.source), lambda: path.mkdir(), lambda: os.mkfifo(path)):
            make()
            self.assertFalse(held.give_back(self.lab, BASE, 'token'))
            if path.is_dir():
                path.rmdir()
            else:
                path.unlink()
        os.link(self.source, path)
        self.assertFalse(held.give_back(self.lab, BASE, 'token'))
        with patch.object(held.os, 'getuid', return_value=-1):
            self.assertFalse(held.valid_file(self.source.stat()))

    def test_availability_checks_metadata_and_invalid_kept_modes(self):
        self.assertEqual(held.availability(self.lab, ['token']), {'token': 'not held'})
        held.put(self.lab, 'token', self.source)
        with patch.object(self.lab, 'read', side_effect=AssertionError('held bytes read')):
            self.assertEqual(held.availability(self.lab, ['token']), {'token': 'kept'})
        (self.lab.root / 'held/token').chmod(0o644)
        self.assertIn('0600', held.availability(self.lab, ['token'])['token'])

    def test_cross_filesystem_put_copies_fsyncs_unlinks_and_settles(self):
        self.take()
        (self.lab.root / BASE / 'held/token').rename(self.source)
        self.source.chmod(0o644)
        rename = store.os.rename
        def move(source, destination, **kwargs):
            if source == self.source.name:
                raise OSError(errno.EXDEV, 'different filesystem')
            return rename(source, destination, **kwargs)
        with patch.object(store.os, 'rename', side_effect=move):
            held.put(self.lab, 'token', self.source)
        self.assertFalse(self.source.exists())
        self.assertEqual((self.lab.root / 'held/token').read_bytes(), self.bytes)
        self.assertEqual(stat.S_IMODE((self.lab.root / 'held/token').stat().st_mode), 0o600)
        self.assertFalse((self.lab.root / 'held/token.taken').exists())

    def test_owner_can_put_a_regular_source_without_read_permission(self):
        self.source.chmod(0o000)
        held.put(self.lab, 'token', self.source)
        self.assertFalse(self.source.exists())
        self.assertEqual((self.lab.root / 'held/token').read_bytes(), self.bytes)
        self.assertEqual(stat.S_IMODE((self.lab.root / 'held/token').stat().st_mode), 0o600)

    def test_failed_cross_filesystem_copy_keeps_source_and_taken_record(self):
        self.take()
        (self.lab.root / BASE / 'held/token').rename(self.source)
        with patch.object(store.os, 'rename', side_effect=OSError(errno.EXDEV, 'different filesystem')), \
             patch.object(store.os, 'read', side_effect=OSError(errno.EIO, 'copy refused')):
            with self.assertRaises(OSError):
                held.put(self.lab, 'token', self.source)
        self.assertEqual(self.source.read_bytes(), self.bytes)
        self.assertFalse((self.lab.root / 'held/token').exists())
        self.assertEqual([p.name for p in (self.lab.root / 'held').iterdir()], ['token.taken'])


class HeldRecords(unittest.TestCase):
    def test_held_is_optional_for_earlier_records_and_validated_when_present(self):
        record = run_record(1, '1')
        self.assertEqual(records.run_refusals(record), [])
        for held_value in ([], [dict(name='token', taken_ms=1, given_ms=None)],
                           [dict(name='token', taken_ms=1, given_ms=2)]):
            self.assertEqual(records.run_refusals({**record, 'held': held_value}), [])
        for held_value in ({}, [dict(name='Bad', taken_ms=1, given_ms=None)],
                           [dict(name='token', taken_ms=None, given_ms=None)],
                           [dict(name='token', taken_ms=2, given_ms=1)],
                           [dict(name='token', taken_ms=True, given_ms=None)],
                           [dict(name='token', taken_ms=1, given_ms=-1)],
                           [dict(name='token', taken_ms=1, given_ms=None)] * 2):
            self.assertTrue(records.run_refusals({**record, 'held': held_value}))

    def test_longest_time_includes_each_release_command(self):
        exp, _subject, settings, _facts = validation_world()
        run = Run.__new__(Run)
        run.exp, run.cost = exp, settings['costs']['smoke']
        before = run.longest_ms()
        run.exp['release'].append(dict(name='settle', argv=['/settle'], env={}))
        self.assertEqual(run.longest_ms() - before,
                         (settings['costs']['smoke']['command_seconds'] + settings['costs']['smoke']['stop_seconds']) * 1000)
