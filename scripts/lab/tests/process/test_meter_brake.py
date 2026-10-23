"""Meter and admission stories against contained commands and the scripted host."""
import json
from pathlib import Path
import subprocess
import time
import unittest
from runner import brake, canonical, evidence, store
from . import test_driver as driver_tests
from . import test_runtime as native


class MeterBrake(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        native.Runtime.setUpClass.__func__(cls)

    def setUp(self):
        self.story_origin = time.monotonic()
        driver_tests.Driver.setUp(self)
        self.settings['costs']['smoke'].update(subject_spend=10, meter_seconds=1,
                                              command_seconds=2, driver_seconds=5, stop_seconds=1)

    def tearDown(self):
        try:
            native.Runtime.tearDown(self)
        finally:
            print(f'{self.id().rsplit(".", 1)[-1]}: {time.monotonic() - self.story_origin:.3f} s', flush=True)

    def records(self):
        keys = list((self.lab_root / 'keys/native').glob('*/evidence.jsonl'))
        self.assertEqual(len(keys), 1)
        key_path = 'keys/native/' + keys[0].parent.name
        with store.open_lab(self.lab_root) as lab:
            entries = evidence.read(lab, key_path)
            records = [canonical.loads(lab.read(key_path + '/runs/' + e['run'] + '/run.json')) for e in entries]
        return key_path, entries, records

    assert_empty = native.Runtime.assert_empty
    cli = native.Runtime.cli

    def inputs(self, script=None):
        self.script.write_text(json.dumps(script or {}))
        path = self.lab_root / 'settings.json'
        path.chmod(0o600)
        path.write_bytes(canonical.record_bytes(self.settings))
        (self.directory / 'experiment.json').write_bytes(canonical.record_bytes(self.exp))

    def run_record(self, status):
        code, _, error = self.cli('run', seed='1')
        self.assertEqual(code, status, error)
        record = self.records()[2][0]
        self.assert_empty(record)
        self.run_root = self.lab_root / 'keys/native' / record['key'] / 'runs' / record['run']
        self.events = [canonical.loads(line + b'\n') for line in (self.run_root / 'events.jsonl').read_bytes().splitlines()]
        return record

    def meter(self, argv):
        self.exp['meter'] = dict(name='spend', argv=argv, env={})

    def brake(self):
        self.settings['brake'] = dict(argv=['{runner}/brake/codex', '--codex', str(self.program),
            '--home', str(self.home), '--five-hour', '50', '--week', '75', '--timeout', '2'], env={})

    def test_subject_spend_stops_drive_and_records_last_windows(self):
        spend = self.root / 'spend'
        self.meter(['/bin/cat', str(spend)])
        windows = dict(primary=dict(windowDurationMins=10080, usedPercent=8, resetsAt=None),
                       secondary=dict(windowDurationMins=300, usedPercent=20, resetsAt=123))
        self.inputs(dict(windows=[windows], items=[dict(type='commandExecution', wait=True,
            argv=[native.PYTHON, '-I', '-B', '-S', '-c', 'from pathlib import Path; import sys; Path(sys.argv[1]).write_text("11\\n")', str(spend)])]))
        record = self.run_record(1)
        self.assertEqual([f['signature'] for f in record['failures']], ['bound/subject_spend'])
        self.assertEqual(record['usage']['subject_spend'], 11)
        self.assertEqual(record['usage']['windows'], [dict(minutes=300, used_percent=20, resets_at=123),
                                                     dict(minutes=10080, used_percent=8, resets_at=None)])
        readings = [e for e in self.events if e['event'] == 'meter']
        self.assertEqual([e['n'] for e in readings], [1, 2])
        self.assertTrue(all(e['spend'] == 11 and not e['timed_out'] and e['status'] == 0 for e in readings))
        self.assertTrue((self.run_root / 'meter/2.stdout').is_file())
        self.assertEqual((self.run_root / 'meter/1.stdout').stat().st_mode & 0o777, 0o400)
        collect = next(i for i, e in enumerate(self.events) if e['event'] == 'phase' and e['phase'] == 'collect')
        self.assertLess(max(i for i, e in enumerate(self.events) if e['event'] == 'meter'), collect)
        self.assertEqual(record['bounds']['subject_spend'], 10)
        self.assertEqual(record['bounds']['meter_seconds'], 1)
        self.assertEqual(record['usage']['longest_ms'], 15000)

    def test_two_failed_readings_stop_drive(self):
        self.meter(['/bin/sh', '-c', 'exit 3'])
        self.inputs(dict(items=[dict(type='commandExecution', wait=True, argv=['/bin/true'])]))
        record = self.run_record(1)
        self.assertEqual([f['signature'] for f in record['failures']], ['meter/exit:3'])
        self.assertEqual(record['failures'][0]['kind'], 'meter')
        self.assertIsNone(record['usage']['subject_spend'])
        self.assertIsNone(record['usage']['windows'])
        self.assertEqual([e['status'] for e in self.events if e['event'] == 'meter'], [3, 3, 3])

    def test_codex_brake_one_shot_admits_and_refuses_without_a_run(self):
        self.exp['driver'] = None
        self.meter(['/bin/echo', '0'])
        self.brake()
        data = dict(primary=dict(windowDurationMins=10080, usedPercent=3),
                    secondary=dict(windowDurationMins=300, usedPercent=12))
        self.inputs(dict(rate_read=data))
        record = self.run_record(0)
        self.assertEqual(self.events[0]['event'], 'brake')
        self.assertEqual(self.events[0]['line'], 'go: five-hour 12%/50%, week 3%/75%')
        self.assertEqual(record['usage']['subject_spend'], 0)
        requests = self.program.with_suffix('.requests')
        self.assertEqual([json.loads(line)['method'] for line in requests.read_text().splitlines()],
                         ['initialize', 'initialized', 'account/rateLimits/read'])
        self.assertFalse(self.program.with_suffix('.turn').exists())
        self.assertFalse(Path(self.program.with_suffix('.brake-home').read_text()).exists())
        data['primary']['usedPercent'] = 80
        self.inputs(dict(rate_read=data))
        before = list((self.lab_root / 'keys/native').glob('*/runs/*'))
        code, stdout, stderr = self.cli('run', seed='2')
        self.assertEqual((code, stdout, stderr.strip()), (2, '', 'brake: refused: week 80%/75%'
                          ' (seen: primary 10080 min 80% resets ?, secondary 300 min 12% resets ?)'))
        self.assertEqual(list((self.lab_root / 'keys/native').glob('*/runs/*')), before)
        self.assertEqual([json.loads(line)['method'] for line in requests.read_text().splitlines()],
                         ['initialize', 'initialized', 'account/rateLimits/read'] * 2)

    def test_pause_refuses_meter_only_and_null_run_ignores_it(self):
        self.exp['driver'] = None
        self.meter(['/bin/echo', '0'])
        pause = self.lab_root / 'pause'
        pause.symlink_to('missing')
        self.inputs()
        code, stdout, stderr = self.cli('run', seed='1')
        self.assertEqual((code, stdout, stderr.strip()), (2, '', 'brake: pause'))
        self.assertFalse(list((self.lab_root / 'keys/native').glob('*/runs/*')))
        del self.exp['meter']
        self.inputs()
        record = self.run_record(0)
        self.assertIsNone(record['usage']['subject_spend'])
        self.assertFalse(any(e['event'] == 'brake' for e in self.events))

    def test_judge_brake_admits_twice_then_refuses_and_prints_verdict(self):
        self.exp['driver'] = None
        self.meter(['/bin/echo', '0'])
        counter = self.root / 'brake-count'
        code = ('from pathlib import Path; import sys; p=Path(sys.argv[1]); '
                'n=int(p.read_text())+1 if p.exists() else 1; p.write_text(str(n)); '
                'print("go" if n<3 else "refused: count 3"); sys.exit(0 if n<3 else 1)')
        self.settings['brake'] = dict(argv=[native.PYTHON, '-I', '-B', '-S', '-c', code, str(counter)], env={})
        self.inputs()
        status, stdout, stderr = self.cli('judge')
        self.assertEqual((status, stdout, stderr.strip()), (2, 'not enough evidence', 'brake: refused: count 3'))
        key_path, entries, records = self.records()
        self.assertEqual(counter.read_text(), '3')
        self.assertEqual(len(entries), 2)
        self.assertTrue(all(r['outcome'] == 'pass' for r in records))
        self.assertEqual(len(list((self.lab_root / key_path / 'runs').iterdir())), 2)
        verdict = canonical.loads((self.lab_root / key_path / 'verdict.json').read_bytes())
        self.assertEqual(verdict['verdict'], 'not enough evidence')
        self.assertEqual(len(verdict['seeds']), 2)

    def test_codex_brake_protocol_error_host_exit_and_timeout(self):
        args = [str(native.PACKAGE / 'brake/codex'), '--codex', str(self.program), '--home', str(self.home),
                '--five-hour', '50', '--week', '75', '--timeout', '1']
        for method in ('initialize', 'account/rateLimits/read'):
            self.inputs(dict(rate_read=dict(primary=None), rpc_error=method))
            result = subprocess.run(args, capture_output=True, check=False, timeout=3)
            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertEqual(result.stdout.strip(), b'refused: rateLimits: scripted host error')
        self.inputs(dict(rate_read=dict(primary=None), rate_exit=True))
        result = subprocess.run(args, capture_output=True, check=False, timeout=3)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(result.stdout.strip(), b'refused: host exited')
        self.inputs(dict(rate_read=dict(primary=None), rate_hang=True))
        result = subprocess.run(args, capture_output=True, check=False, timeout=3)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(result.stdout.strip(), b'refused: host timed out')

    def test_settings_brake_caps_output_uses_exact_env_and_kills_group(self):
        code = ('import json,os,sys; '
                'assert sys.stdin.read()==""; assert dict(os.environ)==dict(ONLY="value",LANG="C.UTF-8"); '
                'sys.stdout.write("x"*100000+"\\nlast line\\n"); sys.stdout.flush(); '
                'sys.stderr.write("e"*100000); sys.stderr.flush()')
        status, output, timed_out = brake.run([native.PYTHON, '-I', '-B', '-S', '-c', code], {'ONLY': 'value', 'LANG': 'C.UTF-8'}, 2)
        self.assertEqual((status, timed_out), (0, False), output['stderr'])
        self.assertEqual({name: len(data) for name, data in output.items()}, {'stdout': brake.CAP, 'stderr': brake.CAP})
        self.assertTrue(output['stdout'].endswith(b'last line\n'))
        code = ('import os,select; child=os.fork(); '
                'print(os.getpid(),flush=True); select.select([],[],[])')
        status, output, timed_out = brake.run([native.PYTHON, '-I', '-B', '-S', '-c', code], {}, 1)
        self.assertEqual((status, timed_out), (None, True))
        for pid in map(int, output['stdout'].splitlines()):
            # The orphaned child takes the group's SIGKILL but is not ours to wait: it may still be exiting.
            deadline = time.monotonic() + 2
            while True:
                try:
                    state = Path('/proc/' + str(pid) + '/stat').read_text().split(') ', 1)[1].split()[0]
                except (FileNotFoundError, ProcessLookupError):
                    state = None
                if state in (None, 'Z') or time.monotonic() > deadline:
                    break
                time.sleep(0.01)
            self.assertIn(state, (None, 'Z'))
