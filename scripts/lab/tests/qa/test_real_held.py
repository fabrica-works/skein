"""Held-file ownership across cuts, bounds and failed releases."""
import json
import os
import socket
import subprocess

from .test_m2_tree import LAB
from .test_real_inputs import RealCase


class HeldQA(RealCase):
    def hold(self):
        self.secret = os.urandom(48)
        source = self.base / 'stand-in'
        source.write_bytes(self.secret)
        self.checked([LAB, 'held', 'put', 'session', source])
        self.assertFalse(source.exists())
        self.exp['held'] = ['session']
        return self.store / 'held/session'

    def move_in(self):
        self.exp['dirs'] = ['state']
        self.exp['actions'] = [self.command_spec('take', '/usr/bin/mv',
            '{held.session}', '{dir.state}/session')]
        self.exp['release'] = [self.command_spec('return', '/usr/bin/mv',
            '{dir.state}/session', '{held.session}')]

    def assert_returned(self, record, path):
        self.assertEqual(path.read_bytes(), self.secret)
        self.assertEqual(path.stat().st_mode & 0o777, 0o600)
        self.assertFalse(path.with_suffix('.taken').exists())
        self.assertEqual(len(record['held']), 1)
        held = record['held'][0]
        self.assertEqual(held['name'], 'session')
        self.assertIsInstance(held['taken_ms'], int)
        self.assertIsInstance(held['given_ms'], int)
        self.assertGreaterEqual(held['given_ms'], held['taken_ms'])

    def test_put_moves_and_refuses_existing_name(self):
        # lab.md 4.8: "moves FILE in ... refuses a name already held".
        path = self.hold()
        listing = self.checked([LAB, 'held'])
        self.assertEqual(listing.stdout, b'session kept\n')
        other = self.base / 'other'
        other.write_bytes(os.urandom(48))
        result = self.command([LAB, 'held', 'put', 'session', other])
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertTrue(other.exists())
        self.assertEqual(path.read_bytes(), self.secret)

    def test_release_follows_failed_command_and_keeps_no_secret_evidence(self):
        # lab.md 4.8: "one that fails ... and the next still runs";
        # "neither a record, an event nor a kept output holds it".
        path = self.hold()
        self.exp['processes'] = [self.service()]
        self.move_in()
        self.exp['release'].insert(0, self.command_spec('bad', '/usr/bin/false'))
        record, run, result = self.execute(self.subject())
        self.assertEqual(result.returncode, 1, record)
        self.assertIn('release/bad/exit:1', self.signatures(record))
        self.assert_returned(record, path)
        for file in run.rglob('*'):
            if file.is_file():
                self.assertNotIn(self.secret, file.read_bytes(), str(file))

    def test_seed_failure_still_releases_held_file(self):
        # lab.md 5.1: a cut permits "then the release, then phase 6 whole".
        path = self.hold()
        self.exp['processes'] = [self.service()]
        self.move_in()
        self.exp['actions'].append(self.command_spec('bad', '/usr/bin/false'))
        record, _, result = self.execute(self.subject())
        self.assertEqual(result.returncode, 1, record)
        self.assert_returned(record, path)

    def test_meter_bound_still_releases_held_file(self):
        # lab.md 4.8: release runs "however the drive ended ... a bound".
        path = self.hold()
        self.exp['processes'] = [self.service()]
        self.move_in()
        self.meter('3')
        record, _, result = self.execute(self.subject())
        self.assertEqual(result.returncode, 1, record)
        self.assertIn('bound/subject_spend', self.signatures(record))
        self.assert_returned(record, path)

    def test_interruption_returns_held_file_before_stopping(self):
        # lab.md 5.1: "A run interrupted ... runs its release ... then goes to phase 6".
        path = self.hold()
        self.move_in()
        host = self.driver({})
        endpoint = self.base / 'notify'
        self.exp['processes'] = [self.service(args=[str(endpoint), 'none', '{run.id}',
                                                   '{dir.state}/terminal'])]
        self.exp['release'].insert(0, self.command_spec('before-stop', '/usr/bin/test',
                                                       '!', '-e', '{dir.state}/terminal'))
        subject = self.subject()
        self.write()
        with socket.socket(socket.AF_UNIX) as listener:
            listener.bind(str(endpoint))
            listener.listen(1)
            listener.settimeout(5)
            process = subprocess.Popen([str(LAB), 'run', str(self.experiment), '--subject', subject,
                '--seed', '7'], env=self.env, cwd=self.base, stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            try:
                connection, _ = listener.accept()
                with connection, connection.makefile('rb') as stream:
                    notice = json.loads(stream.readline())
                    # The participant identifies the worker before readiness; the
                    # host reads its script at drive, after the held seed action.
                    host.with_suffix('.json').write_text(json.dumps(dict(items=[dict(
                        type='commandExecution', argv=['/usr/bin/kill', '-INT', str(notice['main'])],
                        wait=True)])))
                    connection.sendall(b'G')
                stdout, stderr = process.communicate(timeout=25)
            finally:
                if process.poll() is None:
                    process.kill()
                    process.communicate(timeout=5)
        self.assertEqual(process.returncode, 130, (stdout, stderr))
        record_path = next(self.store.glob('keys/*/*/runs/*/run.json'))
        record = json.loads(record_path.read_bytes())
        self.assertEqual(record['outcome'], 'error', record)
        self.assert_returned(record, path)
        before_stop = next(c for c in record['commands'] if c['name'] == 'before-stop')
        self.assertEqual(before_stop['exit']['status'], 0, record)
        self.assertFalse(any(c.get('cluster') == 'stopped' and 'skipped' not in c
                             for c in record['commands']))

    def test_lost_file_stays_taken_refuses_next_run_and_owner_can_settle(self):
        # lab.md 4.8: "release/held:<name> ... name stays taken";
        # put "settles one taken by a run that did not give it back".
        path = self.hold()
        self.exp['processes'] = [self.service()]
        self.move_in()
        self.exp['release'] = []
        subject = self.subject()
        record, run, result = self.execute(subject)
        self.assertEqual(result.returncode, 1, record)
        self.assertIn('release/held:session', self.signatures(record))
        self.assertIsNone(record['held'][0]['given_ms'])
        self.assertFalse(path.exists())
        self.assertTrue((run / 'dirs/state/session').is_file())
        taken = json.loads(path.with_suffix('.taken').read_bytes())
        self.assertEqual(taken['run'], record['run'])
        listing = self.checked([LAB, 'held'])
        self.assertIn(('session taken ' + record['run']).encode(), listing.stdout)
        refused = self.command([LAB, 'run', self.experiment, '--subject', subject, '--seed', '7'])
        self.assertEqual(refused.returncode, 2, refused.stderr)
        self.assertIn(record['run'].encode(), refused.stderr)
        self.assertEqual(len(list(self.store.glob('keys/*/*/runs/*/run.json'))), 1)
        self.checked([LAB, 'held', 'put', 'session', run / 'dirs/state/session'])
        self.assertEqual(path.read_bytes(), self.secret)
        self.assertFalse(path.with_suffix('.taken').exists())

    def test_giveback_refuses_wrong_mode_without_repair(self):
        # lab.md 4.8: given back only "0600, not a link"; otherwise stays taken.
        path = self.hold()
        self.exp['processes'] = [self.service()]
        self.exp['release'] = [self.command_spec('wrong-mode', '/usr/bin/chmod', '0644', '{held.session}')]
        record, run, result = self.execute(self.subject())
        self.assertEqual(result.returncode, 1, record)
        self.assertIn('release/held:session', self.signatures(record))
        self.assertIsNone(record['held'][0]['given_ms'])
        self.assertFalse(path.exists())
        self.assertEqual((run / 'held/session').stat().st_mode & 0o777, 0o644)

    def test_held_templates_refused_outside_seed_and_release_argv(self):
        # lab.md 4.2: held is known in "seed actions and the release, in argv alone".
        self.hold()
        subject = self.subject()
        cases = [
            ('stdin', dict(actions=[self.command_spec('bad', '/usr/bin/cat', stdin='{held.session}')])),
            ('env', dict(actions=[dict(name='bad', argv=['/usr/bin/true'], env=dict(FILE='{held.session}'))])),
            ('collect', dict(collect=[self.command_spec('bad', '/usr/bin/cat', '{held.session}', scan=[])])),
            ('process', dict(processes=[self.service(args=['{held.session}', 'none', '{run.id}'])])),
            ('undeclared', dict(actions=[self.command_spec('bad', '/usr/bin/cat', '{held.other}')])),
            ('await', dict(release=[self.command_spec('bad', '/usr/bin/true', **{'await':
                dict(process='service', stream='stdout', prefix='x', capture='x')})])),
        ]
        for name, replacement in cases:
            with self.subTest(name=name):
                self.exp.update(actions=[], release=[], processes=[], collect=[])
                self.exp.update(replacement)
                result = self.validate(subject)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertFalse(list(self.store.glob('keys/*/*/runs/*')))

    def test_raised_meter_bound_does_not_supersede_held_loss(self):
        # lab.md 8: "failures are all bounds"; 4.8: "release/held:<name>".
        path = self.hold()
        self.exp['processes'] = [self.service()]
        self.move_in()
        self.exp['release'] = []
        self.meter('3')
        subject = self.subject()
        record, run, result = self.execute(subject)
        self.assertEqual(result.returncode, 1, record)
        self.assertIn('bound/subject_spend', self.signatures(record))
        self.assertIn('release/held:session', self.signatures(record))
        # The owner settles ownership before asking judge about the same key.
        self.checked([LAB, 'held', 'put', 'session', run / 'dirs/state/session'])
        self.assertTrue(path.is_file())
        self.settings['costs']['qa']['subject_spend'] = 4
        self.save_settings()
        self.write()
        result = self.command([LAB, 'judge', self.experiment, '--subject', subject], timeout=25)
        # The next run loses the stand-in again; judge's following admission refuses.
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertEqual(len(list(self.store.glob('keys/*/*/runs/*/run.json'))), 2)
        verdict = json.loads((run.parents[1] / 'verdict.json').read_bytes())
        self.assertIn(record['run'], json.dumps(verdict['seeds']))
