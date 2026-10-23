"""Black-box contracts for named inputs, admission and metered runs."""
import hashlib
import json
import shutil

from .test_m2_tree import LAB, ROOT, RunnerCase, canonical


def digest(path):
    return 'sha256:' + hashlib.sha256(path.read_bytes()).hexdigest()


class RealCase(RunnerCase):
    def setUp(self):
        super().setUp()
        self.settings_path = self.store / 'settings.json'
        self.settings = json.loads(self.settings_path.read_bytes())
        self.settings['costs']['qa']['ready_seconds'] = 2
        self.save_settings()

    def save_settings(self):
        self.settings_path.write_bytes(canonical(self.settings))

    def checked(self, argv):
        result = self.command(argv, timeout=25)
        self.assertEqual(result.returncode, 0, result.stderr.decode(errors='replace'))
        return result

    def key(self, subject):
        result = self.validate(subject)
        self.assertEqual(result.returncode, 0, result.stderr.decode(errors='replace'))
        path = next(self.store.glob('keys/*/' + result.stdout.strip().decode() + '/key.json'))
        return json.loads(path.read_bytes()), path.parent

    def execute(self, subject, seed=7):
        before = set(self.store.glob('keys/*/*/runs/*/run.json'))
        self.write()
        result = self.command([LAB, 'run', self.experiment, '--subject', subject,
                               '--seed', str(seed)], timeout=25)
        paths = set(self.store.glob('keys/*/*/runs/*/run.json')) - before
        self.assertEqual(len(paths), 1, (result.returncode, result.stdout, result.stderr))
        path = paths.pop()
        return json.loads(path.read_bytes()), path.parent, result

    def command_spec(self, name, *argv, **extra):
        return dict(name=name, argv=list(argv), env={}, **extra)

    def meter(self, value='0'):
        self.exp['meter'] = self.command_spec('spend', '/usr/bin/printf', value + '\n')
        self.settings['costs']['qa'].update(subject_spend=2, meter_seconds=1)
        self.save_settings()

    def host(self, script, name='host'):
        path = self.base / (name + '.py')
        shutil.copyfile(ROOT / 'scripts/lab/tests/support/scripted_codex.py', path)
        path.chmod(0o700)
        path.with_suffix('.json').write_text(json.dumps(script))
        return path

    def driver(self, script):
        path = self.host(script)
        codex_home = self.store / 'invented-codex'
        codex_home.mkdir(mode=0o700)
        self.settings.update(codex_home=str(codex_home), driver=dict(
            program=dict(path=str(path), sha256=digest(path), version='0.162.1'),
            model='scripted', effort='low'))
        self.save_settings()
        (self.experiment / 'charter.md').write_text('charter: observe the invented subject.\n')
        self.exp['driver'] = dict(charter='charter.md', tools=[])
        return path

    def signatures(self, record):
        return {failure['signature'] for failure in record['failures']}


class InputsQA(RealCase):
    def test_key_tuning_ports_and_program_versions(self):
        # lab.md 2.3: "cost, the class's name alone"; a port "not in the key";
        # a named program is "among the key's programs, its version beside it".
        helper = self.base / 'echo'
        shutil.copyfile('/usr/bin/echo', helper)
        helper.chmod(0o700)
        self.settings.update(programs=dict(echo=dict(path=str(helper), sha256=digest(helper),
                                                   version='one')), ports=dict(forge=18001))
        self.save_settings()
        self.exp['actions'] = [self.command_spec('show', '{program.echo}', '{port.forge}')]
        subject = self.subject()
        first, directory = self.key(subject)
        self.assertEqual(first['lab']['cost'], dict(name='qa'))
        self.assertIn(dict(path=str(helper), sha256=digest(helper), version='one'),
                      first['lab']['programs'])
        self.settings['costs']['qa']['command_seconds'] = 3
        self.settings['ports']['forge'] = 18002
        self.save_settings()
        second, same = self.key(subject)
        self.assertEqual(directory, same)
        self.assertEqual(first, second)
        self.settings['programs']['echo']['version'] = 'two'
        self.save_settings()
        _, changed = self.key(subject)
        self.assertNotEqual(directory, changed)

    def test_unused_stale_program_is_ignored(self):
        # lab.md 2.3: "A name no template uses is never read."
        self.settings['programs'] = dict(absent=dict(path=str(self.base / 'absent'),
                                                    sha256='sha256:' + '0' * 64, version='one'))
        self.save_settings()
        self.key(self.subject())

    def test_changed_program_refuses_run_before_provision(self):
        # lab.md 2.3: "a run checks it again before it starts".
        helper = self.base / 'echo'
        shutil.copyfile('/usr/bin/echo', helper)
        helper.chmod(0o700)
        self.settings['programs'] = dict(echo=dict(path=str(helper), sha256=digest(helper), version='one'))
        self.save_settings()
        self.exp['actions'] = [self.command_spec('show', '{program.echo}', 'invented')]
        subject = self.subject()
        self.key(subject)
        helper.write_bytes(helper.read_bytes() + b'invented change')
        result = self.command([LAB, 'run', self.experiment, '--subject', subject, '--seed', '7'])
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn(b'program', result.stderr.lower())
        self.assertFalse(list(self.store.glob('keys/*/*/runs/*/run.json')))

    def test_unknown_program_and_port_report_both_members(self):
        # lab.md 4.7: "every problem found, each naming its member and why".
        self.exp['actions'] = [self.command_spec('missing', '{program.absent}', '{port.absent}')]
        result = self.validate(self.subject())
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn(b'actions.0.argv.0: program absent', result.stderr)
        self.assertIn(b'actions.0.argv.1: port absent', result.stderr)
        self.assertFalse(list(self.store.glob('keys/*/*/runs/*')))

    def test_ports_render_in_argv_environment_and_stdin(self):
        # lab.md 4.2: "Every string of argv, of env's values and of stdin".
        self.settings['ports'] = dict(forge=18001)
        self.save_settings()
        self.exp['actions'] = [dict(name='show', argv=['/usr/bin/printenv', 'KEPT'],
                                    env=dict(KEPT='{port.forge}')),
                               self.command_spec('echo', '/usr/bin/cat', stdin='{port.forge}\n')]
        record, run, result = self.execute(self.subject())
        self.assertEqual(result.returncode, 0, record)
        self.assertEqual(record['outcome'], 'pass')
        for command in record['commands']:
            self.assertEqual((run / command['output']['stdout']['path']).read_text(), '18001\n')
        self.assertEqual(record['bounds']['command_seconds'], 2)


class MeterBrakeQA(RealCase):
    def test_meter_requires_both_class_values(self):
        # lab.md 4.7: "a meter whose class lacks subject_spend or meter_seconds".
        self.exp['meter'] = self.command_spec('spend', '/usr/bin/printf', '0\n')
        subject = self.subject()
        for present, absent in [('subject_spend', 'meter_seconds'), ('meter_seconds', 'subject_spend')]:
            with self.subTest(absent=absent):
                self.settings['costs']['qa'].pop(absent, None)
                self.settings['costs']['qa'][present] = 1
                self.save_settings()
                result = self.validate(subject)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn(absent.encode(), result.stderr)

    def test_final_meter_reading_at_bound_precedes_collection(self):
        # lab.md 6.5: "once more after it, before the live collections";
        # "Past subject_spend" (equality is admitted).
        self.meter('2')
        self.exp['collect'] = [self.command_spec('collect', '/usr/bin/true', scan=[])]
        record, run, result = self.execute(self.subject())
        self.assertEqual(result.returncode, 0, record)
        self.assertEqual(record['usage']['subject_spend'], 2)
        events = [json.loads(line) for line in (run / 'events.jsonl').read_text().splitlines()]
        meter_index = next(i for i, e in enumerate(events) if e['event'] == 'meter')
        self.assertTrue(any(e['event'] == 'phase' and 'collect' in e.values()
                            for e in events[meter_index + 1:]), events)

    def test_final_meter_overspend_fails_without_driver(self):
        # lab.md 6.5: "Past subject_spend ... fails bound/subject_spend".
        self.meter('3')
        record, _, result = self.execute(self.subject())
        self.assertEqual(result.returncode, 1, record)
        self.assertEqual(record['usage']['subject_spend'], 3)
        self.assertIn('bound/subject_spend', self.signatures(record))

    def test_final_malformed_meter_cannot_pass_unmetered(self):
        # lab.md 6.5: "or prints no such line ... fails the run meter/reading";
        # "no subject spends unmetered".
        self.meter('01')
        record, _, result = self.execute(self.subject())
        self.assertEqual(record['usage']['subject_spend'], None)
        self.assertIn('meter/reading', self.signatures(record), record)
        self.assertEqual(result.returncode, 1, record)

    def test_pause_refuses_judge_and_makes_no_run(self):
        # lab.md 6.5: "every brake refuses while ... pause exists";
        # "lab judge stops at a refused run and prints the verdict so far".
        self.meter()
        subject = self.subject()
        self.write()
        (self.store / 'pause').symlink_to(self.base / 'absent')
        result = self.command([LAB, 'judge', self.experiment, '--subject', subject])
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn(b'brake: pause', result.stderr)
        self.assertIn(b'not enough evidence', result.stdout)
        self.assertFalse(list(self.store.glob('keys/*/*/runs/*')))

    def test_brake_refusal_before_driver(self):
        # lab.md 6.5: "any other end refuses it ... and makes no run".
        host = self.driver({})
        self.settings['brake'] = dict(argv=['/usr/bin/false'], env={})
        self.save_settings()
        subject = self.subject()
        self.write()
        result = self.command([LAB, 'run', self.experiment, '--subject', subject, '--seed', '7'])
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn(b'brake:', result.stderr)
        self.assertFalse(host.with_suffix('.probe').exists())
        self.assertFalse(host.with_suffix('.turn').exists())
        self.assertFalse(list(self.store.glob('keys/*/*/runs/*')))

    def test_codex_brake_reversed_windows_refuses_at_share(self):
        # lab.md 6.5: the brake "refuses past the settings' share of each";
        # README: "each strictly below its share", windows by duration.
        host = self.host(dict(rate_read=dict(primary=dict(windowDurationMins=10080,
            usedPercent=75, resetsAt=123), secondary=dict(windowDurationMins=300,
            usedPercent=1, resetsAt=456))), name='brake')
        codex_home = self.base / 'brake-codex'
        codex_home.mkdir(mode=0o700)
        self.meter()
        self.settings['brake'] = dict(argv=['{runner}/brake/codex', '--codex', str(host),
            '--home', str(codex_home), '--five-hour', '50', '--week', '75', '--timeout', '2'], env={})
        self.save_settings()
        subject = self.subject()
        self.write()
        result = self.command([LAB, 'run', self.experiment, '--subject', subject, '--seed', '7'])
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn(b'brake:', result.stderr)
        self.assertIn(b'75', result.stderr)
        requests = [json.loads(line)['method'] for line in host.with_suffix('.requests').read_text().splitlines()]
        self.assertEqual(requests, ['initialize', 'initialized', 'account/rateLimits/read'])
        self.assertFalse(list(self.store.glob('keys/*/*/runs/*')))
