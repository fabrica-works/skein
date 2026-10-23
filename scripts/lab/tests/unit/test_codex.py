"""The pinned adapter's inventory, audit, action identity, usage and report vectors."""
import json
from pathlib import Path
import tomllib
import unittest
from runner import canonical, codex, experiment, life
from runner.records import Refused
from .fixtures import fixture


REPORT = dict(done=True, retried=[], unseen=[], surprises=[])


def inventory():
    return {'thread/start': dict(activePermissionProfile=dict(id='lab'), instructionSources=[]),
            'mcpServerStatus/list': [dict(data=[dict(name='lab', runtimeStatus='connected', toolsError=None,
                tools=dict(echo={}), resources=[], resourceTemplates=[])], nextCursor=None)],
            'app/installed': dict(apps=[]), 'plugin/list': dict(marketplaces=[], marketplaceLoadErrors=[]),
            'hooks/list': dict(data=[dict(hooks=[], errors=[], warnings=[])]),
            'skills/list': dict(data=[dict(skills=[], errors=[])]),
            'config/read': dict(layers=[dict(name=dict(type='user', file='/lab/codex/config.toml'), config=dict(model='script')),
                                      dict(name=dict(type='system'), config={})])}


def drive(cost=None):
    exp, _, settings, _, _ = fixture()
    exp.update(processes=[], actions=[], collect=[], checks=[], driver=dict(charter='charter.md', tools=[]))
    state = life.initial(exp, '1', cost or settings['costs']['smoke'], {}, {})
    for kind in ('begin', 'validated', 'provisioned', 'driver_spawned'):
        state, effects = life.step(state, dict(kind=kind, ms=0))
    return state


def message(method, **params):
    return dict(method=method, params=params)


class Codex(unittest.TestCase):
    def test_config_and_key_use_templates_and_profile_parameter(self):
        schema = json.loads((Path(__file__).resolve().parents[2] / 'schemas/report.json').read_text())
        value = codex.configuration('/usr/bin/python3', '/runner', '/bin/codex', ['echo'], 'script', 'low', True, schema)
        self.assertEqual(value['schema'], schema)
        text = value['toml']
        parsed = tomllib.loads(text)
        self.assertIn('{driver.work}', parsed['permissions']['lab']['filesystem'])
        self.assertIn('{source}', parsed['permissions']['lab']['filesystem'])
        self.assertEqual(parsed['mcp_servers']['lab']['args'], ['-I', '-B', '/runner/runner/launch.py', '{lab.root}', 'tool', '{run.id}'])
        self.assertEqual(parsed['mcp_servers']['lab']['env_vars'], [])
        self.assertEqual(parsed['mcp_servers']['lab']['default_tools_approval_mode'], 'approve')
        self.assertFalse(parsed['model_providers']['lab-openai']['supports_websockets'])
        self.assertNotIn('base_url', parsed['model_providers']['lab-openai'])
        self.assertEqual(parsed['shell_environment_policy'], {'inherit': 'none', 'set': {'PATH': '/usr/bin:/bin'}})
        self.assertEqual(parsed['features'], {**dict.fromkeys(codex.DISABLED, False), 'skip_host_skill_discovery': True})
        self.assertFalse(parsed['skills']['bundled']['enabled'])
        self.assertEqual(parsed['tools'], {'experimental_request_user_input': {'enabled': False}})
        self.assertEqual(parsed['default_permissions'], 'lab')
        self.assertIn('{codex.home}', value['inventory']['configuration_layers'])
        text = codex.config_toml('/python', '/runner', 'r', [], 'script', 'low', 'default_permissions="special"\n')
        self.assertEqual(tomllib.loads(text)['default_permissions'], 'special')
        self.assertEqual(json.loads(canonical.record_bytes(value)), value)

    def test_a_pinned_catalog_is_named_at_the_root(self):
        schema = json.loads((Path(__file__).resolve().parents[2] / 'schemas/report.json').read_text())
        plain = codex.configuration('/python', '/runner', '/bin/codex', [], 'm', 'low', False, schema)
        self.assertNotIn('model_catalog_json', tomllib.loads(plain['toml']))
        pinned = codex.configuration('/python', '/runner', '/bin/codex', [], 'm', 'low', False, schema, '/lab/catalog.json')
        self.assertEqual(tomllib.loads(pinned['toml'])['model_catalog_json'], '/lab/catalog.json')

    def test_catalog_must_give_the_driver_model_direct_tools(self):
        def refused(value, model='m'):
            data = value if type(value) is bytes else json.dumps(value).encode()
            return [r.why for r in codex.catalog_refusals(data, model)]
        entry = dict(slug='m', tool_mode=None, multi_agent_version=None, experimental_supported_tools=[],
                     apply_patch_tool_type=None, supports_search_tool=False, context_window=1)
        self.assertEqual(refused(dict(models=[entry, dict(slug='other', tool_mode='code_mode_only')])), [])
        self.assertEqual(refused(b'{'), ['catalog is not JSON'])
        self.assertEqual(refused(b'\xff'), ['catalog is not JSON'])
        self.assertEqual(refused([entry]), ['catalog has no models list'])
        self.assertEqual(refused(dict(models=[entry]), model='absent'), ["catalog must hold the driver model 'absent' once"])
        self.assertEqual(refused(dict(models=[entry, entry])), ["catalog must hold the driver model 'm' once"])
        for field, offering in (('tool_mode', 'code_mode_only'), ('multi_agent_version', 'v2'),
                                ('experimental_supported_tools', ['clock']), ('apply_patch_tool_type', 'freeform'),
                                ('experimental_supported_tools', None), ('apply_patch_tool_type', False),
                                ('supports_search_tool', True), ('supports_search_tool', 0), ('supports_search_tool', None)):
            for changed in ({**entry, field: offering}, {k: v for k, v in entry.items() if k != field}):
                with self.subTest(field=field, changed=changed):
                    wanted = {'experimental_supported_tools': '[]', 'supports_search_tool': 'false'}.get(field, 'null')
                    self.assertEqual(refused(dict(models=[changed])), [f"the driver model's {field} must be {wanted}"])
        self.assertEqual(len(refused(dict(models=[dict(slug='m')]))), 5)

    def test_catalog_settings_are_absolute_and_digested(self):
        exp, subject, settings, facts, _ = fixture()
        exp['driver'] = dict(charter='charter.md', tools=[])
        facts.update(driver_host=True)
        facts['experiment_files']['charter.md'] = 'charter'
        program = dict(path='/codex', sha256='sha256:' + 'a' * 64, version='1')
        settings['driver'] = dict(program=program, model='m', effort='low',
                                  catalog=dict(path='/lab/catalog.json', sha256='sha256:' + 'b' * 64))
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])
        settings['driver']['catalog'] = dict(path='catalog.json', sha256='b', extra=1)
        members = [p.member for p in experiment.validate(exp, subject, settings, facts)]
        self.assertEqual(sorted(members), ['settings.driver.catalog.extra', 'settings.driver.catalog.path',
                                           'settings.driver.catalog.sha256'])

    def test_every_inventory_row_and_unobserved_items(self):
        expected = codex.expected_inventory(['echo'], '/lab/codex/config.toml')
        observed = codex.observe(inventory(), expected)
        self.assertEqual(set(observed), set(expected))
        for name, row in observed.items():
            self.assertEqual(row['state'], 'unobserved' if name in ('prompts', 'plugins', 'built_in_tools') else 'observed', name)
        self.assertEqual(codex.observe(dict(expected=expected, **inventory())), observed)

    def test_apps_pass_only_when_none_is_enabled_or_callable(self):
        expected = codex.expected_inventory(['echo'], '/lab/codex/config.toml')
        off = dict(id='connector_a', runtimeName='A', enabled=False, callable=False)
        vectors = [([], 'observed'), ([off, {**off, 'id': 'connector_b'}], 'observed'),
                   ([off, {**off, 'enabled': True}], 'mismatch'), ([{**off, 'callable': True}], 'mismatch'),
                   ([{k: v for k, v in off.items() if k != 'callable'}], 'mismatch'),
                   ([{**off, 'enabled': 0}], 'mismatch'), (['app'], 'mismatch'), (None, 'mismatch')]
        for apps, state in vectors:
            values = inventory()
            values['app/installed'] = dict(apps=apps)
            with self.subTest(apps=apps):
                row = codex.observe(values, expected)['apps']
                self.assertEqual(row['state'], state)
                self.assertEqual(row['evidence'], dict(apps=apps))

    def test_each_observer_detects_missing_or_mismatched_fields(self):
        expected = codex.expected_inventory(['echo'], '/lab/codex/config.toml')
        vectors = [
            ('permission_profile', 'thread/start', dict(activePermissionProfile=dict(id='unsafe'))),
            ('instruction_files', 'thread/start', dict(instructionSources=['AGENTS.md'])),
            ('mcp_servers_and_tools', 'mcpServerStatus/list', []),
            ('resources', 'mcpServerStatus/list', [dict(data=[dict(resources=['x'], resourceTemplates=[])], nextCursor=None)]),
            ('resources', 'mcpServerStatus/list', [dict(data=[dict(resources=[], resourceTemplates=['x'])], nextCursor=None)]),
            ('mcp_servers_and_tools', 'mcpServerStatus/list', [dict(data=[
                inventory()['mcpServerStatus/list'][0]['data'][0], dict(name='other', resources=[], resourceTemplates=[])],
                nextCursor=None)]),
            ('apps', 'app/installed', dict(apps=['app'])),
            ('plugins', 'plugin/list', dict(marketplaces=['plugin'])),
            ('hooks', 'hooks/list', dict(data=[dict(hooks=[], errors=[], warnings=['warning'])])),
            ('skills', 'skills/list', dict(data=[])),
            ('configuration_layers', 'config/read', dict(layers=[])),
        ]
        for row, method, override in vectors:
            values = inventory()
            if type(override) is dict:
                values[method].update(override)
            else:
                values[method] = override
            with self.subTest(row=row):
                self.assertEqual(codex.observe(values, expected)[row]['state'], 'mismatch')
        for method in ('thread/start', *codex.INVENTORY_METHODS):
            values = inventory()
            del values[method]
            self.assertTrue(any(v['state'] == 'mismatch' for v in codex.observe(values, expected).values()), method)
            values[method] = None
            self.assertTrue(any(v['state'] == 'mismatch' for v in codex.observe(values, expected).values()), method)

    def test_inventory_pages_and_only_empty_system_layers(self):
        expected = codex.expected_inventory(['echo'], '/lab/codex/config.toml')
        values = inventory()
        values['mcpServerStatus/list'].insert(0, dict(data=[], nextCursor='p2'))
        self.assertEqual(codex.observe(values, expected)['mcp_servers_and_tools']['state'], 'observed')
        values['mcpServerStatus/list'][-1]['nextCursor'] = 'incomplete'
        self.assertEqual(codex.observe(values, expected)['mcp_servers_and_tools']['state'], 'mismatch')
        for layer in (dict(name=dict(type='system'), config=dict(extra=True)),
                      dict(name=dict(type='project'), config={}),
                      dict(name=dict(type='user', file='/other'), config={})):
            values = inventory()
            values['config/read']['layers'].append(layer)
            self.assertEqual(codex.observe(values, expected)['configuration_layers']['state'], 'mismatch')

    def test_audit_admitted_and_unadmitted_kinds(self):
        for kind in codex.NON_ACTIONS:
            self.assertEqual(codex.item_kind(dict(type=kind)), 'message')
        self.assertEqual(codex.item_kind(dict(type='commandExecution')), 'action')
        self.assertEqual(codex.item_kind(dict(type='mcpToolCall', server='lab')), 'action')
        for kind in ('fileChange', 'webSearch', 'imageView', 'dynamicToolCall', 'collabAgentToolCall', 'subAgentActivity',
                     'imageGeneration', 'hookPrompt', 'sleep', 'enteredReviewMode', 'exitedReviewMode', 'functionCallOutput', 'newKind'):
            with self.subTest(kind=kind), self.assertRaises(Refused):
                codex.item_kind(dict(type=kind))
        with self.assertRaises(Refused):
            codex.item_kind(dict(type='mcpToolCall', server='other'))

    def test_raw_calls_pair_with_items_and_unhandled_calls_are_counted(self):
        def fold(*messages):
            state = codex.initial()
            for n, m in enumerate(messages):
                state = codex.fold(state, m, n)
            return state
        call = lambda cid, name='exec_command', **more: message('rawResponseItem/completed', item=dict(
            type='function_call', call_id=cid, name=name, arguments=more.pop('arguments', '{}'),
            internal_chat_message_metadata_passthrough=dict(create_time=1.5), **more))
        output = lambda cid: message('rawResponseItem/completed', item=dict(type='function_call_output', call_id=cid, output=''))
        command = dict(type='commandExecution', id='a', processId='77')
        done = message('turn/completed', turn=dict(id='t', status='completed'))
        state = fold(message('rawResponseItem/completed', item=dict(type='message', role='user')),
                     message('rawResponseItem/completed', item=dict(type='reasoning')),
                     call('a'), message('item/started', item=command), output('a'),
                     call('w', 'write_stdin', arguments='{"session_id":77,"chars":""}'), output('w'), done)
        self.assertIsNone(state['unhandled'])
        self.assertEqual([(a['kind'], a['item_id']) for a in state['actions']], [('commandExecution', 'a'), ('write_stdin', 'w')])
        vectors = [
            ((call('x', 'bogus'), output('x')), 'unhandled call: x', 1),
            ((call('x', 'tool_search_tool', namespace='tool_search'), output('x')), 'unhandled call: x', 1),
            ((call('a'), message('item/started', item=command), output('a'),
              call('w', 'write_stdin', arguments='{"session_id":78}'), output('w')), 'unhandled call: w', 2),
            ((call('w', 'write_stdin', arguments='not json'), output('w')), 'unhandled call: w', 1),
            ((message('rawResponseItem/completed', item=dict(type='web_search_call', id='s')),), 'unhandled call kind: web_search_call', 1),
            ((message('rawResponseItem/completed', item=dict(type='tool_search_call', call_id='s')),), 'unhandled call kind: tool_search_call', 1),
            ((call('x'), call('x')), 'unhandled call kind: function_call', 1),
            ((call('x'), done), 'unanswered call: x', 1),
            ((message('item/started', item=command), done), 'actions without raw response items', 1),
        ]
        for messages, why, count in vectors:
            with self.subTest(why=why, messages=messages):
                state = fold(*messages)
                self.assertEqual(state['unhandled'], why)
                self.assertEqual(len(state['actions']), count)
                self.assertEqual(state['actions'][-1]['kind'], 'commandExecution' if why.startswith('actions') else 'unhandled')
                canonical.dumps(state)

    def test_codex_resource_tools_are_admitted_actions_that_reach_nothing(self):
        for tool in ('list_mcp_resources', 'list_mcp_resource_templates', 'read_mcp_resource'):
            self.assertEqual(codex.item_kind(dict(type='mcpToolCall', server='codex', tool=tool)), 'action')
        for tool in ('exec_command', 'apply_patch', 'spawn_agent', None):
            with self.subTest(tool=tool), self.assertRaises(Refused):
                codex.item_kind(dict(type='mcpToolCall', server='codex', tool=tool))

        def run(tool, status, result=None, error=None):
            call = dict(type='mcpToolCall', id='r', server='codex', tool=tool, arguments={})
            state = codex.fold(codex.initial(), message('item/started', item=dict(call, status='inProgress')), 1)
            state = codex.fold(state, message('item/completed', item=dict(call, status=status, result=result, error=error)), 2)
            self.assertEqual(len(state['actions']), 1)
            self.assertEqual((state['actions'][0]['kind'], state['actions'][0]['sources']), ('mcpToolCall', ['host']))
            self.assertNotIn('call', state['actions'][0])
            return state

        def text(value):
            return dict(content=[dict(type='text', text=value if type(value) is str else json.dumps(value))],
                        structuredContent=None, _meta=None)
        for tool, payload in (('list_mcp_resources', dict(resources=[])),
                              ('list_mcp_resources', dict(resources=[], server='lab')),
                              ('list_mcp_resource_templates', dict(resourceTemplates=[]))):
            run(tool, 'completed', text(payload))
        for tool in codex.RESOURCE_TOOLS:
            run(tool, 'failed', None, dict(message='unknown MCP server'))
        for tool, result in (('list_mcp_resources', text(dict(resources=[dict(uri='lab://x')]))),
                             ('list_mcp_resources', text(dict(resources=[], nextCursor='more'))),
                             ('list_mcp_resources', text(dict(resourceTemplates=[]))),
                             ('list_mcp_resource_templates', text(dict(resourceTemplates=[dict(uriTemplate='lab://{x}')]))),
                             ('list_mcp_resource_templates', text(dict(resourceTemplates=[], extra=1))),
                             ('list_mcp_resources', text('not json')),
                             ('list_mcp_resources', dict(content=[])),
                             ('list_mcp_resources', None),
                             ('read_mcp_resource', text(dict(contents=[dict(uri='lab://x', text='secret')]))),
                             ('read_mcp_resource', text(dict(contents=[])))):
            with self.subTest(tool=tool, result=result), self.assertRaises(Refused) as caught:
                run(tool, 'completed', result)
            self.assertIn(tool + ' returned content', str(caught.exception))

    def test_actions_numbered_and_broker_duplicates_pair_in_both_orders(self):
        for first, second in (('host', 'broker'), ('broker', 'host')):
            state = codex.initial()
            call = dict(type='mcpToolCall', id='call', server='lab', tool='echo', arguments=dict(args=['word'], stdin=None))
            action = codex.action_start(state, call, 4, first)
            codex.action_start(state, dict(type='commandExecution', id='command'), 5)
            self.assertIs(action, codex.action_start(state, call, 6, second))
            self.assertEqual([a['n'] for a in state['actions']], [1, 2])
            # Two simultaneous identical calls remain distinct and pair FIFO.
            other = {**call, 'id': 'other'}
            codex.action_start(state, other, 7, first)
            codex.action_start(state, other, 8, second)
            self.assertEqual(len(state['actions']), 3)
            self.assertEqual(state['actions'][-1]['sources'], [first, second])

    def test_usage_folds_per_turn_and_unknown_stays_null(self):
        state = codex.initial()
        for turn, usage in [('one', dict(inputTokens=10, cachedInputTokens=4, outputTokens=3)),
                            ('one', dict(inputTokens=12, cachedInputTokens=5, outputTokens=4)),
                            ('two', dict(inputTokens=7, outputTokens=2))]:
            state = codex.fold(state, message('thread/tokenUsage/updated', turnId=turn, tokenUsage=dict(last=usage)), 1)
        self.assertEqual(state['tokens'], dict(input=19, cached=None, output=6))
        self.assertEqual(len(state['turns']), 2)
        with self.assertRaises(Refused):
            codex.fold(state, message('thread/tokenUsage/updated', turnId='one', tokenUsage=dict(last=dict(inputTokens=True))), 1)

    def test_host_payloads_outside_the_key_subset_remain_opaque_evidence(self):
        item = dict(type='commandExecution', id='one', durationMs=1.5, output={'é': 'word'})
        state = codex.fold(codex.initial(), message('item/completed', item=item), 2)
        self.assertEqual(json.loads(state['actions'][0]['end']['json']), item)
        canonical.dumps(state)

    def test_report_schema_and_final_agent_message(self):
        schema = json.loads((Path(__file__).resolve().parents[2] / 'schemas/report.json').read_text())
        self.assertEqual(set(schema['required']), set(REPORT))
        self.assertFalse(schema['additionalProperties'])
        self.assertEqual(codex.report(json.dumps(REPORT)), REPORT)
        for value in ('not json', '{}', json.dumps({**REPORT, 'extra': 1}), json.dumps({**REPORT, 'done': 1}),
                      json.dumps({**REPORT, 'retried': [dict(step='x', tries=[{}])]}),
                      json.dumps({**REPORT, 'unseen': [dict(step='x')]}), json.dumps({**REPORT, 'surprises': [False]})):
            with self.subTest(value=value), self.assertRaises(Refused):
                codex.report(value)
        state = codex.initial()
        for text in ('intermediate prose', json.dumps(REPORT)):
            state = codex.fold(state, message('item/completed', item=dict(type='agentMessage', text=text)), 2)
        state = codex.fold(state, message('turn/completed', turn=dict(id='one', status='completed')), 3)
        self.assertEqual(state['report'], REPORT)


class DriveLife(unittest.TestCase):
    def event(self, state, kind, ms=1, **fields):
        return life.step(state, dict(kind=kind, ms=ms, **fields))

    def test_inventory_before_turn_and_inventory_error(self):
        for mismatch in (False, True):
            state = drive()
            state, effects = self.event(state, 'driver_inventory', inventory={'x': dict(state='mismatch' if mismatch else 'observed')})
            self.assertEqual(any(e['op'] == 'driver_turn' for e in effects), not mismatch)
            self.assertEqual(bool(state.data['errors']), mismatch)
            self.assertEqual(state.data['driver']['ending'], mismatch)

    def test_time_actions_and_input_token_bounds_and_overshoot(self):
        for name, event in (
            ('driver_seconds', dict(kind='tick', ms=1000)),
            ('driver_actions', dict(kind='driver_event', ms=1, message=message('item/started', item=dict(type='commandExecution', id='second')))),
            ('driver_input_tokens', dict(kind='driver_event', ms=1,
                message=message('thread/tokenUsage/updated', turnId='one', tokenUsage=dict(last=dict(inputTokens=7)))))):
            state = drive({**fixture()[2]['costs']['smoke'], name: 1})
            if name == 'driver_actions':
                state, _ = self.event(state, 'driver_event', message=message('item/started', item=dict(type='commandExecution', id='first')))
            state, effects = life.step(state, event)
            self.assertIn(dict(op='bound', name=name), effects)
            self.assertTrue(state.data['driver']['ending'])
            self.assertEqual(state.data['driver']['observed']['overshoot'], 6 if name == 'driver_input_tokens' else None)

    def test_audit_error_stops_driver(self):
        state, effects = self.event(drive(), 'driver_event', message=message('item/started', item=dict(type='webSearch')))
        self.assertTrue(state.data['errors'])
        self.assertIn(dict(op='stop_driver'), effects)

    def test_commands_before_inventory_still_count_and_clean_exit_without_report(self):
        state = drive()
        state, _ = self.event(state, 'driver_event', message=message('item/started', item=dict(type='commandExecution', id='early')))
        self.assertEqual(len(state.data['driver']['observed']['actions']), 1)
        self.assertIsNone(state.data['driver']['inventory'])
        state, _ = self.event(state, 'driver_inventory', inventory={'x': dict(state='observed')})
        state, effects = self.event(state, 'driver_closed', result=dict(status=0))
        self.assertFalse(state.data['errors'])
        self.assertIsNone(state.data['driver']['observed']['report'])
        self.assertEqual(state.data['phase'], 'judge')

    def test_host_failure_or_incomplete_inventory_is_an_error(self):
        for status, complete in ((7, True), (0, False)):
            state = drive()
            if complete:
                state, _ = self.event(state, 'driver_inventory', inventory={'x': dict(state='observed')})
            state, _ = self.event(state, 'driver_closed', result=dict(status=status))
            self.assertTrue(state.data['errors'])

    def test_late_usage_after_stop_keeps_tokens_and_overshoot(self):
        state = drive({**fixture()[2]['costs']['smoke'], 'driver_seconds': 1, 'driver_input_tokens': 5})
        state, _ = self.event(state, 'tick', ms=1000)
        state, effects = self.event(state, 'driver_event', ms=1001, message=message(
            'thread/tokenUsage/updated', turnId='one', tokenUsage=dict(last=dict(inputTokens=8, cachedInputTokens=2))))
        self.assertEqual(state.data['driver']['observed']['tokens'], dict(input=8, cached=2, output=None))
        self.assertEqual(state.data['driver']['observed']['overshoot'], 3)
        self.assertIn(dict(op='bound', name='driver_input_tokens'), effects)

    def test_settings_config_and_triager_stay_refused(self):
        exp, subject, settings, facts, _ = fixture()
        exp['driver'] = dict(charter='charter.md', tools=[])
        settings['driver'] = dict(program=dict(path='/codex', sha256='sha256:' + 'a' * 64, version='1'),
                                  model='script', effort='low', config={})
        settings['triager'] = {}
        facts.update(driver_host=True)
        facts['experiment_files']['charter.md'] = 'charter'
        problems = experiment.validate(exp, subject, settings, facts)
        self.assertIn('settings.driver.config', [p.member for p in problems])
        self.assertIn('settings.triager', [p.member for p in problems])

    def test_faults_remain_refused(self):
        exp, _, settings, _, _ = fixture()
        exp['faults'] = [dict(kill='backend', signal='SIGTERM', after_actions=[1, 2])]
        with self.assertRaisesRegex(Refused, 'unsupported by this runtime'):
            life.initial(exp, '1', settings['costs']['smoke'], {}, {})

    def test_use_and_missing_or_malformed_report(self):
        for report, use in ((REPORT, False), ({**REPORT, 'done': False}, True),
                            ({**REPORT, 'unseen': [dict(step='x', looked_for='button')]}, True), ({}, False)):
            state = drive()
            state, _ = self.event(state, 'driver_event', message=message('item/completed',
                item=dict(type='agentMessage', text=json.dumps(report))))
            state, effects = self.event(state, 'driver_event', message=message('turn/completed', turn=dict(id='one', status='completed')))
            self.assertEqual(any(e['op'] == 'driver_use' for e in effects), use)
            self.assertFalse(state.data['errors'])
            self.assertTrue(state.data['driver']['ending'])
            self.assertEqual(state.data['driver']['observed']['report'], None if report == {} else report)
