"""A contained triager reads retained evidence through a scripted host, with no model."""
from contextlib import redirect_stderr, redirect_stdout
import io
import json
from pathlib import Path
import unittest
from runner import canonical, cli, store
from runner.records import triage_refusals
from . import test_driver, test_runtime


class Triage(unittest.TestCase):
    setUpClass = classmethod(test_runtime.Runtime.setUpClass.__func__)
    tearDown = test_runtime.Runtime.tearDown
    records = test_runtime.Runtime.records
    assert_empty = test_runtime.Runtime.assert_empty
    cli = test_driver.Driver.cli
    story = test_driver.Driver.story

    def setUp(self):
        test_driver.Driver.setUp(self)
        self.settings['triager'] = dict(self.settings['driver'], model='triage-scripted')
        self.settings['costs']['smoke'].update(triage_seconds=2, triage_tokens=100)
        (self.directory / 'check.py').write_text(
            'import sys; print(\'{"fail":"new","summary":"new failure"}\'); sys.exit(1)\n')
        self.exp['checks'] = [dict(name='new', argv=[test_runtime.PYTHON, '-I', '-B', '-S',
                                                  '{experiment}/check.py'], env={})]
        self.run = self.story(dict(report=dict(done=False, retried=[], unseen=[], surprises=[])), 1)
        self.original = {name: (self.root_run / name).read_bytes() for name in ('run.json', 'events.jsonl')}

    def settings_file(self):
        (self.lab_root / 'settings.json').write_bytes(canonical.record_bytes(self.settings))

    def histories(self):
        return {path.name: path.read_bytes() for path in (self.lab_root / 'findings').glob('*.jsonl')}

    def invoke(self, script, expected, *, run=None):
        self.script.write_text(json.dumps(dict(triage=script)))
        self.settings_file()
        self.program.with_suffix('.turn').unlink(missing_ok=True)
        stdout, stderr = io.StringIO(), io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            status = cli.main(['triage', self.run['run'] if run is None else run], lab_root=self.lab_root)
        self.assertEqual(status, expected, stderr.getvalue())
        return stdout.getvalue().strip(), stderr.getvalue()

    def triage(self, script, expected=0, *, n=1):
        outcome, _ = self.invoke(script, expected)
        self.base = self.root_run / 'triage' / str(n)
        value = canonical.loads((self.base / 'triage.json').read_bytes())
        self.assertEqual(triage_refusals(value), [])
        self.assertEqual(outcome, value['outcome'])
        self.assertEqual(value['n'], n)
        self.assertEqual(value['network'], 'host')
        self.assertFalse((self.base / 'live.json').exists())
        events = [json.loads(line) for line in (self.base / 'events.jsonl').read_text().splitlines()]
        groups = [e['cgroup'] for e in events if e.get('event') == 'containment']
        self.assertTrue(all(not Path(group).exists() for group in groups))
        for name, before in self.original.items():
            self.assertEqual((self.root_run / name).read_bytes(), before)
        for name in ('events.jsonl', 'host.stdout', 'host.stderr'):
            self.assertEqual((self.base / name).stat().st_mode & 0o777, 0o600)
        for name in ('input.json', 'output.json', 'triage.json'):
            self.assertEqual((self.base / name).stat().st_mode & 0o777, 0o400)
        for name in ('input', 'output'):
            data = (self.base / (name + '.json')).read_bytes()
            self.assertEqual(value[name], dict(path=name + '.json', bytes=len(data), sha256=store.file_digest(data)))
        return value

    def test_proposals_keep_candidate_and_create_use_finding_with_usage_and_evidence(self):
        with store.open_lab(self.lab_root) as lab, lab.lock():
            for name in ('scratch/secret', 'dirs/client/secret', 'driver/home/secret',
                         'driver/rehearsal/home/secret', 'driver/provision/home/secret',
                         'driver/probe/home/secret'):
                lab.write_once(str(self.root_run.relative_to(lab.root)) + '/' + name, b'private fixture')
        arg0 = self.root_run / 'driver/probe/codex/tmp/arg0/x'
        arg0.mkdir(parents=True, mode=0o700)
        (arg0 / 'apply_patch').symlink_to(self.program)
        self.settings['brake'] = dict(argv=['/usr/bin/echo', 'admitted triage'], env={})
        value = self.triage(dict(items=[dict(type='commandExecution', argv=[test_runtime.PYTHON,
            '-I', '-B', '-S', '-c', 'from pathlib import Path; assert Path("../evidence/run.json").is_file(); '
            'assert not Path("../evidence/scratch").exists(); Path("observed").write_text("retained")'])],
            usage=[dict(inputTokens=12, cachedInputTokens=5, outputTokens=3)]))
        self.assertEqual([(e['id'], e['value']['event']) for e in value['finding_events']],
                         [('F-0002', 'candidate'), ('F-0002', 'proposed'), ('F-0001', 'proposed')])
        self.assertTrue(all(e['value']['by'] == 'triager' for e in value['finding_events']))
        supplied = json.loads((self.base / 'input.json').read_bytes())
        self.assertEqual([f['kind'] for f in supplied['failures']], ['use', 'check'])
        self.assertFalse(supplied['report']['done'])
        self.assertEqual(value['usage']['tokens'], dict(input=12, cached=5, output=3))
        self.assertEqual(value['usage']['turns'], 1)
        self.assertEqual(value['brake']['line'], 'admitted triage')
        self.assertEqual((self.base / 'work/observed').read_text(), 'retained')
        self.assertIsNone(value['usage']['tokens_overshoot'])
        self.assertEqual(value['invocation']['env']['CODEX_HOME'], str(self.home))
        self.assertEqual(value['expected_inventory']['mcp_servers_and_tools'], {})
        self.assertTrue(all(row['ok'] for row in value['probe'].values()))
        probe = json.loads(self.program.with_suffix('.probe').read_text())
        self.assertEqual(probe['argv'][:3], ['sandbox', '-P', 'triage'])
        self.assertIn(probe['profile'], (self.base / 'config.toml').read_text())
        paths = {file['path'] for file in store.snapshot(self.base / 'evidence')}
        self.assertIn('run.json', paths)
        self.assertIn('commands/checks-new.stderr', paths)
        self.assertIn('driver/events.jsonl', paths)
        self.assertIn('driver/probe.json', paths)
        self.assertFalse((self.base / 'evidence/driver/probe').exists())
        self.assertFalse(any(path.startswith(('scratch/', 'dirs/', 'driver/home/',
            'driver/rehearsal/home/', 'driver/provision/home/', 'driver/probe/')) for path in paths))
        index = json.loads((self.lab_root / 'findings/index.json').read_bytes())['findings']
        self.assertEqual([entry['proposed'] for entry in index], ['tools', 'tools'])
        self.assertTrue(all(entry['class'] is None for entry in index))

    def test_use_match_appends_occurrence_and_nonuse_match_stays_in_reason(self):
        value = self.triage(dict(match_use='F-0001', match_nonuse='F-0001'))
        self.assertEqual([(e['id'], e['value']['event']) for e in value['finding_events']],
                         [('F-0001', 'occurred'), ('F-0001', 'proposed')])
        self.assertIn('Suggested match: F-0001', value['finding_events'][-1]['value']['reason'])
        self.assertEqual(len(self.histories()), 1)
        before = self.histories()['F-0001.jsonl'].splitlines()
        repeated = self.triage(dict(match_use='F-0001'), n=2)
        self.assertEqual([e['value']['event'] for e in repeated['finding_events']], ['proposed'])
        self.assertEqual(len(self.histories()['F-0001.jsonl'].splitlines()), len(before) + 1)

    def test_malformed_output_appends_no_findings(self):
        before = self.histories()
        value = self.triage(dict(output=dict(failures=[])), 4)
        self.assertEqual(value['outcome'], 'error')
        self.assertEqual(value['finding_events'], [])
        self.assertEqual(self.histories(), before)

    def test_host_transport_error_appends_no_findings(self):
        before = self.histories()
        value = self.triage(dict(rpc_error='turn/start'), 4)
        self.assertTrue(any('scripted host error' in e['detail'] for e in value['errors']))
        self.assertEqual(value['finding_events'], [])
        self.assertEqual(self.histories(), before)

    def test_time_bound_ends_host_without_findings_or_known_tokens(self):
        self.settings['costs']['smoke'].update(triage_seconds=1, stop_seconds=1)
        before = self.histories()
        value = self.triage(dict(wait_after_actions=True), 1)
        self.assertIn('bound/triage_seconds', [e['what'] for e in value['errors']])
        self.assertGreaterEqual(value['usage']['seconds'], 1)
        self.assertEqual(value['usage']['tokens'], dict(input=None, cached=None, output=None))
        self.assertEqual(self.histories(), before)

    def test_tokens_at_bound_propose_and_one_past_records_overshoot(self):
        self.settings['costs']['smoke']['triage_tokens'] = 15
        self.triage(dict(match_use='F-0001', usage=[dict(inputTokens=12, cachedInputTokens=5, outputTokens=3)]))
        before = self.histories()
        value = self.triage(dict(usage=[dict(inputTokens=12, cachedInputTokens=5, outputTokens=4)]), 1, n=2)
        self.assertIn('bound/triage_tokens', [e['what'] for e in value['errors']])
        self.assertEqual(value['usage']['tokens_overshoot'], 1)
        self.assertEqual(self.histories(), before)

    def test_probe_failure_prevents_any_turn_and_finding(self):
        before = self.histories()
        value = self.triage(dict(probe_failure='source_write'), 4)
        self.assertFalse(value['probe']['source_write']['ok'])
        self.assertIsNone(value['invocation'])
        self.assertFalse(self.program.with_suffix('.turn').exists())
        self.assertEqual(self.histories(), before)

    def test_retry_after_probe_error_uses_next_directory_without_walking_probe_home(self):
        failed = self.triage(dict(probe_failure='source_write'), 4)
        self.assertEqual(failed['outcome'], 'error')
        previous = self.base
        record = (previous / 'triage.json').read_bytes()
        arg0 = previous / 'probe/codex/tmp/arg0/x'
        arg0.mkdir(parents=True, mode=0o700)
        (arg0 / 'apply_patch').symlink_to(self.program)
        value = self.triage({}, n=2)
        self.assertEqual(value['outcome'], 'proposed')
        self.assertEqual((previous / 'triage.json').read_bytes(), record)
        self.assertFalse((self.base / 'evidence/triage').exists())

    def test_inventory_with_a_tool_prevents_turn_and_findings(self):
        before = self.histories()
        server = dict(name='unexpected', runtimeStatus='connected', toolsError=None,
                      tools=dict(read={}), resources=[], resourceTemplates=[])
        value = self.triage(dict(inventory={'mcpServerStatus/list': dict(data=[server])}), 4)
        self.assertEqual(value['inventory']['mcp_servers_and_tools']['state'], 'mismatch')
        self.assertFalse(self.program.with_suffix('.turn').exists())
        self.assertEqual(self.histories(), before)

    def test_completion_before_inventory_never_publishes_proposals(self):
        before = self.histories()
        output = dict(failures=[dict(id=f['id'], match=None,
            signature='use/early' if f['kind'] == 'use' else None,
            title='Early proposal', reason='Not admitted', **{'class': 'tools'}) for f in self.run['failures']])
        value = self.triage(dict(before_inventory=output), 4)
        self.assertTrue(any('before inventory and turn admission' in e['detail'] for e in value['errors']))
        self.assertFalse(self.program.with_suffix('.turn').exists())
        self.assertEqual(self.histories(), before)

    def test_unadmitted_raw_tool_ends_triage_without_findings(self):
        before = self.histories()
        value = self.triage(dict(items=[dict(type='raw', item=dict(type='function_call',
            call_id='tool', name='apply_patch', arguments='{}'))]), 4)
        self.assertTrue(any('unadmitted raw call' in e['detail'] for e in value['errors']))
        self.assertEqual(self.histories(), before)

    def test_null_triager_and_missing_bounds_refuse_without_triage_directory(self):
        configured = self.settings['triager']
        self.settings['triager'] = None
        _, error = self.invoke({}, 2)
        self.assertIn('settings.triager', error)
        self.settings['triager'] = configured
        for field in ('triage_seconds', 'triage_tokens'):
            old = self.settings['costs']['smoke'].pop(field)
            _, error = self.invoke({}, 2)
            self.assertIn(field, error)
            self.settings['costs']['smoke'][field] = old
        self.assertEqual(list((self.root_run / 'triage').glob('*')), [])

    def test_pause_and_brake_refuse_without_triage_directory(self):
        pause = self.lab_root / 'pause'
        pause.touch(mode=0o600)
        _, error = self.invoke({}, 2)
        self.assertIn('brake: pause', error)
        pause.unlink()
        self.settings['brake'] = dict(argv=['/bin/false'], env={})
        _, error = self.invoke({}, 2)
        self.assertIn('brake: exit 1', error)
        self.assertEqual(list((self.root_run / 'triage').glob('*')), [])

    def test_writer_lock_refuses_triage_without_starting_host(self):
        with store.open_lab(self.lab_root) as lab, lab.lock():
            _, error = self.invoke({}, 2)
        self.assertIn('lock', error)
        self.assertEqual(list((self.root_run / 'triage').glob('*')), [])

    def test_existing_occurrence_without_use_has_nothing_to_triage(self):
        self.script.write_text('{}')
        status, _, error = self.cli('run', seed='1')
        self.assertEqual(status, 1, error)
        second = self.records()[2][-1]
        _, error = self.invoke({}, 2, run=second['run'])
        self.assertIn('no use failures or candidates created by this run', error)
        self.assertFalse((self.root_run.parent / second['run'] / 'triage').exists())

    def rehearsal_story(self, name, expected):
        self.settings['costs']['smoke']['triage_tokens'] = 1000
        self.settings['brake'] = dict(argv=['/usr/bin/false'], env={})
        self.settings['codex_home'] = str(self.root / 'paid-home-sentinel')
        Path(self.settings['codex_home']).write_bytes(b'never open this')
        (self.lab_root / 'pause').write_text('paused')
        self.settings_file()
        before = self.histories()
        script = test_runtime.PACKAGE / 'tests/support/triage-rehearsal' / (name + '.json')
        stdout, stderr = io.StringIO(), io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            status = cli.main(['triage', self.run['run'], '--rehearse', str(script)], lab_root=self.lab_root)
        self.assertEqual(status, expected, stderr.getvalue())
        base = self.lab_root / 'rehearsals/triage' / self.run['run'] / '1'
        value = canonical.loads((base / 'triage.json').read_bytes())
        self.assertEqual(triage_refusals(value), [])
        self.assertEqual(stdout.getvalue().strip(), value['outcome'])
        self.assertEqual(value['rehearsal'], dict(script=store.file_digest(script.read_bytes())))
        self.assertEqual(value['network'], 'loopback')
        self.assertEqual(value['run'], self.run['run'])
        self.assertEqual(value['finding_events'], [])
        self.assertEqual(self.histories(), before)
        self.assertEqual(list((self.root_run / 'triage').glob('*')), [])
        self.assertFalse((base / 'live.json').exists())
        self.assertEqual(Path(self.settings['codex_home']).read_bytes(), b'never open this')
        self.assertEqual(value['invocation']['env']['CODEX_HOME'], str(base / 'rehearsal/home'))
        self.assertTrue(list((base / 'rehearsal/requests').glob('[0-9]*.json')))
        for name, before in self.original.items():
            self.assertEqual((self.root_run / name).read_bytes(), before)
        return value

    def test_correct_script_rehearses_triage_while_paused_and_proposes_without_findings(self):
        value = self.rehearsal_story('correct', 0)
        self.assertEqual(value['outcome'], 'proposed')
        self.assertEqual(value['usage']['tokens'], dict(input=200, cached=0, output=20))

    def test_wrong_script_rehearses_triage_and_errors_without_findings(self):
        value = self.rehearsal_story('wrong', 4)
        self.assertEqual(value['outcome'], 'error')
        self.assertTrue(any('triage.output' in error['detail'] for error in value['errors']))
