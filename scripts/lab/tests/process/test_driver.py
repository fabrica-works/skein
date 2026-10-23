"""A contained scripted app-server drives one turn without any model call."""
from contextlib import redirect_stderr, redirect_stdout
import io
import json
import os
from pathlib import Path
import unittest
import time
from runner import canonical, cli, store
from . import test_runtime as native


class Driver(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.origin = time.monotonic()
        native.Runtime.setUpClass.__func__(cls)

    @classmethod
    def tearDownClass(cls):
        print(f'scripted driver stories: {time.monotonic() - cls.origin:.3f} s', flush=True)

    def setUp(self):
        native.Runtime.setUp(self)
        self.program = self.root / 'scripted_codex.py'
        self.program.write_bytes((native.PACKAGE / 'tests/support/scripted_codex.py').read_bytes())
        self.program.chmod(0o700)
        self.script = self.program.with_suffix('.json')
        self.script.write_text('{}')
        self.script.chmod(0o600)
        self.home = self.root / 'codex'
        self.home.mkdir(mode=0o700)
        self.settings = canonical.loads((self.lab_root / 'settings.json').read_bytes())
        self.settings.update(driver=dict(program=dict(path=str(self.program),
            sha256=store.file_digest(self.program.read_bytes()), version='scripted/0.162.1'), model='scripted', effort='low'),
            codex_home=str(self.home))
        self.settings['costs']['smoke'].update(driver_seconds=2, driver_actions=5, driver_input_tokens=100)
        self.exp = dict(format='lab-experiment/1', name='native', cost='smoke', dirs=[], processes=[], actions=[],
                        collect=[], checks=[], driver=dict(charter='charter.md', tools=[
                            dict(name='echo', argv=['/usr/bin/echo'], env={}, reads=[], writes=[])]), faults=[], planted=[])
        (self.directory / 'charter.md').write_text('This charter exercises the scripted driver.')

    tearDown = native.Runtime.tearDown

    def cli(self, command, *, seed):
        # The native worker, app-server, MCP server and views remain separate processes.
        stdout, stderr = io.StringIO(), io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            status = cli.main([command, str(self.directory), '--subject', self.subject, '--seed', seed],
                              lab_root=self.lab_root)
        return status, stdout.getvalue().strip(), stderr.getvalue()

    records = native.Runtime.records
    assert_empty = native.Runtime.assert_empty

    def story(self, script, expected):
        self.script.write_text(json.dumps(script))
        settings = self.lab_root / 'settings.json'
        settings.chmod(0o600)
        settings.write_bytes(canonical.record_bytes(self.settings))
        (self.directory / 'experiment.json').write_bytes(canonical.record_bytes(self.exp))
        status, _, stderr = self.cli('run', seed='1')
        self.assertEqual(status, expected, stderr)
        record = self.records()[2][0]
        self.assert_empty(record)
        self.assertIsNotNone(record['driver'])
        self.assertIsNotNone(record['usage']['driver'])
        self.root_run = self.lab_root / 'keys/native' / record['key'] / 'runs' / record['run']
        return record

    def test_pass_records_commands_calls_usage_inventory_and_report(self):
        self.exp['driver']['tools'].append(dict(name='read', argv=['/usr/bin/cat'], env={}, reads=[], writes=[]))
        record = self.story(dict(pages=True, items=[
            dict(type='mcpToolCall', server='lab', tool='echo', arguments=dict(args=['early'], stdin='input'), early=True),
            dict(type='commandExecution', argv=['/bin/sh', '-c', 'printf observed > evidence; printf command-output']),
            dict(type='mcpToolCall', server='lab', tool='read', arguments=dict(args=['evidence'], stdin=None))],
            usage=[dict(inputTokens=10, cachedInputTokens=4, outputTokens=3)]), 0)
        self.assertEqual((self.root_run / 'driver/work/evidence').read_text(), 'observed')
        driver = record['driver']
        self.assertEqual([a['n'] for a in driver['actions']], [1, 2, 3])
        command = next(a for a in driver['actions'] if a['kind'] == 'commandExecution')
        read = next(a for a in driver['actions'] if a.get('call', {}).get('tool') == 'read')
        self.assertEqual(command['end']['aggregatedOutput'], 'command-output')
        self.assertEqual(driver['end']['status'], 0)
        self.assertEqual(record['usage']['driver']['tokens'], dict(input=10, cached=4, output=3))
        self.assertEqual(record['usage']['driver']['turns'], 1)
        self.assertEqual(record['usage']['driver']['actions'], 3)
        self.assertEqual({n for n, v in driver['inventory'].items() if v['state'] == 'unobserved'},
                         {'prompts', 'plugins', 'built_in_tools'})
        self.assertEqual(json.loads((self.root_run / 'driver/report.json').read_text()),
                         dict(done=True, retried=[], unseen=[], surprises=[]))
        self.assertEqual(len((self.root_run / 'driver/transcript.jsonl').read_text().splitlines()), 3)
        calls = [json.loads(line) for line in (self.root_run / 'driver/calls.jsonl').read_text().splitlines()]
        self.assertEqual([(c['event'], c['n']) for c in calls], [('start', 1), ('end', 1), ('start', read['n']), ('end', read['n'])])
        self.assertEqual([set(a['sources']) for a in driver['actions'] if a['kind'] == 'mcpToolCall'],
                         [{'host', 'broker'}, {'host', 'broker'}])
        self.assertEqual((self.root_run / f'driver/calls/{read["n"]}.stdout').read_text(), 'observed')
        transcript = [json.loads(line) for line in (self.root_run / 'driver/transcript.jsonl').read_text().splitlines()]
        for action in transcript:
            if action['kind'] == 'mcpToolCall':
                end = action['broker']['end']
                self.assertEqual(action['end']['result']['content'][0]['type'], 'text')
                result = json.loads(action['end']['result']['content'][0]['text'])
                self.assertEqual(result['stdout'], (self.root_run / end['stdout']['path']).read_text())
                for name in ('stdin', 'stdout', 'stderr'):
                    if name in end:
                        data = (self.root_run / end[name]['path']).read_bytes()
                        self.assertEqual(end[name]['bytes'], len(data))
                        self.assertEqual(end[name]['sha256'], store.file_digest(data))
        self.assertTrue(self.program.with_suffix('.held').exists())
        probed = json.loads(self.program.with_suffix('.probe').read_text())
        self.assertIn(probed['profile'], (self.root_run / 'driver/config.toml').read_text())
        self.assertTrue(all(row['ok'] for row in driver['probe'].values()))
        tools = canonical.loads((self.root_run / 'driver/tools.json').read_bytes())
        self.assertEqual(tools['socket'], str(self.root_run / 'driver/broker.sock'))
        self.assertEqual(tools['tools'][0]['argv'], ['/usr/bin/echo'])
        self.assertFalse((self.root_run / 'driver/broker.sock').exists())
        self.assertTrue(self.program.with_suffix('.turn').exists())
        key = json.loads((self.root_run.parents[1] / 'key.json').read_bytes())
        config = key['lab']['driver']['config']
        self.assertNotIn(str(self.root_run), json.dumps(config))
        self.assertNotIn(str(self.home), json.dumps(config))
        self.assertIn('{driver.work}', config['toml'])
        self.assertEqual(config['schema'], json.loads(self.program.with_suffix('.turn').read_text())['outputSchema'])
        self.assertEqual((self.root_run / 'driver/host.stderr').stat().st_mode & 0o777, 0o600)

    def catalog(self, tool_mode):
        path = self.root / 'catalog.json'
        path.write_text(json.dumps(dict(models=[dict(slug='scripted', tool_mode=tool_mode, multi_agent_version=None,
                                                       experimental_supported_tools=[], apply_patch_tool_type=None,
                                                       supports_search_tool=False)])))
        self.settings['driver']['catalog'] = dict(path=str(path), sha256=store.file_digest(path.read_bytes()))
        return path

    def validate(self):
        settings = self.lab_root / 'settings.json'
        settings.chmod(0o600)
        settings.write_bytes(canonical.record_bytes(self.settings))
        (self.directory / 'experiment.json').write_bytes(canonical.record_bytes(self.exp))
        stdout, stderr = io.StringIO(), io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            status = cli.main(['validate', str(self.directory), '--subject', self.subject], lab_root=self.lab_root)
        return status, stdout.getvalue().strip(), stderr.getvalue()

    def test_pinned_catalog_is_keyed_named_and_checked(self):
        path = self.catalog(None)
        status, identity, stderr = self.validate()
        self.assertEqual(status, 0, stderr)
        key = canonical.loads((self.lab_root / 'keys/native' / identity / 'key.json').read_bytes())
        self.assertEqual(key['lab']['driver']['catalog'], self.settings['driver']['catalog'])
        self.assertIn('model_catalog_json = ' + json.dumps(str(path)), key['lab']['driver']['config']['toml'])
        path.write_text(json.dumps(dict(models=[])))
        status, _, stderr = self.validate()
        self.assertNotEqual(status, 0)
        self.assertIn('catalog differs from its configured digest', stderr)
        self.catalog('code_mode_only')
        status, _, stderr = self.validate()
        self.assertNotEqual(status, 0)
        self.assertIn("the driver model's tool_mode must be null", stderr)
        path.unlink()
        status, _, stderr = self.validate()
        self.assertNotEqual(status, 0)
        self.assertIn('settings.driver.catalog', stderr)

    def test_seconds_bound_stops_a_waiting_host(self):
        self.settings['costs']['smoke']['driver_seconds'] = 1
        record = self.story(dict(items=[dict(type='commandExecution', argv=['/bin/true'], wait=True)]), 1)
        self.assertEqual([f['signature'] for f in record['failures']], ['bound/driver_seconds'])
        self.assertLess(record['usage']['driver']['elapsed_ms'], 1500)

    def waiting_tool(self, *, command_timeout=False):
        endpoint = str(self.root / 'endpoint')
        helper = str(native.PACKAGE / 'tests/support/tool_client.py')
        self.exp['driver']['tools'] = [dict(name='wait', argv=[native.PYTHON, '-I', '-B', '-S', helper],
            env={}, reads=[helper], writes=[str(self.root)])]
        return dict(endpoint=endpoint, command_timeout=command_timeout, items=[
            dict(type='mcpToolCall', server='lab', tool='wait', arguments=dict(
                args=['wait', endpoint, str(self.root)], stdin=None), wait=True, trigger=True)])

    def assert_view_ended(self, record):
        self.assertEqual(self.program.with_suffix('.ended').read_text(), 'view and detached descendant ended')
        calls = [json.loads(line) for line in (self.root_run / 'driver/calls.jsonl').read_text().splitlines()]
        self.assertEqual([(c['event'], c['n']) for c in calls], [('start', 1)])
        transcript = json.loads((self.root_run / 'driver/transcript.jsonl').read_text().splitlines()[0])
        self.assertIsNone(transcript['broker']['end'])
        self.assertIsNone(record['driver']['actions'][0]['completed_ms'])

    def test_actions_bound_ends_a_running_call_and_detached_descendant(self):
        self.settings['costs']['smoke']['driver_actions'] = 1
        record = self.story(self.waiting_tool(), 1)
        self.assertEqual([f['signature'] for f in record['failures']], ['bound/driver_actions'])
        self.assertEqual(record['usage']['driver']['actions'], 2)
        self.assert_view_ended(record)

    def test_command_bound_ends_a_running_call_and_detached_descendant(self):
        self.settings['costs']['smoke']['command_seconds'] = 1
        record = self.story(self.waiting_tool(command_timeout=True), 1)
        self.assertEqual([f['signature'] for f in record['failures']], ['bound/command_seconds'])
        self.assert_view_ended(record)

    def test_probe_failure_spends_nothing_and_starts_no_process(self):
        self.exp['processes'] = [dict(name='withheld', argv=['/bin/true'], env={},
            ready=[dict(stream='stdout', prefix='ready ', capture=None)], memory_bytes=None)]
        self.exp['actions'] = [dict(name='withheld', argv=['/bin/true'], env={})]
        self.script.write_text(json.dumps(dict(probe_failure='network')))
        settings = self.lab_root / 'settings.json'
        settings.chmod(0o600)
        settings.write_bytes(canonical.record_bytes(self.settings))
        (self.directory / 'experiment.json').write_bytes(canonical.record_bytes(self.exp))
        status, _, stderr = self.cli('run', seed='1')
        self.assertEqual(status, 4, stderr)
        record = self.records()[2][0]
        self.assertEqual(record['processes'], {})
        self.assertEqual(record['commands'], [])
        self.assertEqual(record['usage']['driver']['actions'], 0)
        self.assertFalse(record['driver']['probe']['network']['ok'])
        root = self.lab_root / 'keys/native' / record['key'] / 'runs' / record['run']
        events = [json.loads(line) for line in (root / 'events.jsonl').read_text().splitlines()]
        self.assertFalse(any(e['event'] in ('start', 'containment') for e in events))
        self.assertFalse(self.program.with_suffix('.turn').exists())

    def test_input_tokens_bound_records_overshoot_and_unknown_cached(self):
        record = self.story(dict(usage=[dict(inputTokens=107, outputTokens=3)]), 1)
        self.assertEqual([f['signature'] for f in record['failures']], ['bound/driver_input_tokens'])
        self.assertEqual(record['usage']['driver']['input_tokens_overshoot'], 7)
        self.assertEqual(record['usage']['driver']['tokens'], dict(input=107, cached=None, output=3))

    def test_unadmitted_item_is_an_error(self):
        record = self.story(dict(items=[dict(type='webSearch')]), 4)
        self.assertTrue(any('unadmitted item kind' in e['detail'] for e in record['errors']))

    def test_inventory_mismatch_is_an_error_and_never_starts_a_turn(self):
        record = self.story(dict(inventory={'thread/start': dict(activePermissionProfile=dict(id='wrong'))}), 4)
        self.assertEqual(record['driver']['inventory']['permission_profile']['state'], 'mismatch')
        self.assertFalse(self.program.with_suffix('.turn').exists())
        self.assertEqual(record['usage']['driver']['turns'], 0)

    def test_use_report_is_an_untriaged_use_failure(self):
        record = self.story(dict(report=dict(done=False, retried=[], unseen=[dict(step='sign in', looked_for='button')], surprises=[])), 1)
        self.assertEqual([f['signature'] for f in record['failures']], ['use/untriaged', 'use/untriaged'])
        self.assertTrue(all(f['kind'] == 'use' for f in record['failures']))

    def test_malformed_report_with_clean_host_is_incomplete(self):
        record = self.story(dict(report='not a report'), 3)
        self.assertEqual(record['errors'], [])
        self.assertNotIn('report.json', record['driver']['files'])

    def test_codex_resource_calls_are_counted_actions(self):
        empty = dict(content=[dict(type='text', text='{"resources":[]}')], structuredContent=None, _meta=None)
        record = self.story(dict(items=[
            dict(type='mcpToolCall', server='codex', tool='list_mcp_resources', arguments={}, status='completed', result=empty),
            dict(type='mcpToolCall', server='codex', tool='read_mcp_resource', arguments=dict(server='lab', uri='lab://x'),
                 status='failed', result=None, error=dict(message='resource not found'))],
            usage=[dict(inputTokens=10, outputTokens=3)]), 0)
        self.assertEqual(record['usage']['driver']['actions'], 2)
        transcript = [json.loads(line) for line in (self.root_run / 'driver/transcript.jsonl').read_text().splitlines()]
        self.assertEqual([(a['n'], a['kind'], a['sources'], a['end']['tool']) for a in transcript],
                         [(1, 'mcpToolCall', ['host'], 'list_mcp_resources'), (2, 'mcpToolCall', ['host'], 'read_mcp_resource')])

    def test_codex_resource_content_is_an_error(self):
        listed = dict(content=[dict(type='text', text='{"resources":[{"uri":"lab://x"}]}')], structuredContent=None, _meta=None)
        record = self.story(dict(items=[dict(type='mcpToolCall', server='codex', tool='list_mcp_resources', arguments={},
                                             status='completed', result=listed)]), 4)
        self.assertTrue(any('list_mcp_resources returned content' in e['detail'] for e in record['errors']))

    def test_another_mcp_server_never_starts_a_turn(self):
        lab = dict(name='lab', runtimeStatus='connected', toolsError=None, tools=dict(echo={}), resources=[], resourceTemplates=[])
        record = self.story(dict(inventory={'mcpServerStatus/list': dict(data=[lab, {**lab, 'name': 'other'}])}), 4)
        self.assertEqual(record['driver']['inventory']['mcp_servers_and_tools']['state'], 'mismatch')
        self.assertFalse(self.program.with_suffix('.turn').exists())
        self.assertEqual(record['usage']['driver']['turns'], 0)

    def test_lab_resource_templates_never_start_a_turn(self):
        lab = dict(name='lab', runtimeStatus='connected', toolsError=None, tools=dict(echo={}), resources=[],
                   resourceTemplates=[dict(uriTemplate='lab://{x}', name='x')])
        record = self.story(dict(inventory={'mcpServerStatus/list': dict(data=[lab])}), 4)
        self.assertEqual(record['driver']['inventory']['resources']['state'], 'mismatch')
        self.assertFalse(self.program.with_suffix('.turn').exists())
        self.assertEqual(record['usage']['driver']['turns'], 0)

    def test_inventory_transport_error_stops_host_without_a_turn(self):
        record = self.story(dict(rpc_error='skills/list'), 4)
        self.assertTrue(any(e['what'] == 'driver/transport' for e in record['errors']))
        self.assertFalse(self.program.with_suffix('.turn').exists())
