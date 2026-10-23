"""Scripts decide from supplied Responses requests, without any host effects."""
from copy import deepcopy
import json
from pathlib import Path
import unittest
from runner import canonical, codex, script
from runner.records import Refused


def value(output=None, capture=None):
    return dict(format='lab-script/1', turns=[dict(output=output or [dict(text='done')], **({} if capture is None else dict(capture=capture)))])


class Script(unittest.TestCase):
    def test_parse_closed_shapes_names_and_regex_groups(self):
        good = value([dict(call='exec_command', namespace=None, arguments=dict(cmd='true'))], dict(result=r'value: (\d+)'))
        self.assertEqual(script.parse(canonical.record_bytes(good)), good)
        variants = [dict(good, extra=1), dict(good, format='lab-run/1'), dict(good, turns={}),
                    dict(good, turns=[{}]), dict(good, turns=[dict(output=[], extra=1)]),
                    value(capture=[]), value(capture={'Invalid': '(x)'}), value(capture={'x': '['}),
                    value(capture={'x': 'no group'}), value(capture={'x': 1}),
                    value([dict(text=1)]), value([dict(text='x', extra=1)]),
                    value([dict(call='', namespace=None, arguments={})]),
                    value([dict(call='tool', namespace='', arguments={})]),
                    value([dict(call='tool', arguments={})]),
                    value([dict(call='tool', namespace=None, arguments=[])]),
                    value([dict(call='tool', namespace=None, arguments={}, extra=1)])]
        for malformed in variants:
            with self.subTest(malformed=malformed), self.assertRaises(Refused):
                script.parse(canonical.record_bytes(malformed))
        with self.assertRaises(Refused):
            script.parse(json.dumps(good).encode())

    def test_latest_outputs_and_nested_substitution(self):
        request = dict(input=[dict(type='function_call_output', output='value: 1'),
                              dict(type='message', role='assistant', content=[]),
                              dict(type='function_call', name='tool'),
                              dict(type='function_call_output', output='value: 7'),
                              dict(type='custom_tool_call_output', output='value: 9')])
        entry = value([dict(call='tool', namespace='mcp__lab', arguments=dict(nested=[dict(text='{capture.result}', old='{capture.old}')]))],
                      dict(result=r'value: (\d+)'))
        captured = dict(old='kept')
        items, new = script.respond(entry, 0, request, captured)
        self.assertEqual(items[0]['arguments']['nested'], [dict(text='7', old='kept')])
        self.assertEqual(new, dict(old='kept', result='7'))
        self.assertEqual(captured, dict(old='kept'))
        request['input'] = request['input'][:3]
        self.assertEqual(script.respond(entry, 0, request, {})[0], [dict(text='script: capture result missed at turn 0')])

    def test_missing_unknown_and_past_turn_return_text(self):
        entry = value([dict(text='{capture.unknown}')])
        self.assertEqual(script.respond(entry, 0, {}, {})[0], [dict(text='script: capture unknown missed at turn 0')])
        self.assertEqual(script.respond(entry, 1, {}, {})[0], [dict(text='script: no turn 1')])

    def test_provisioning_calls_each_tool_in_order_then_reports(self):
        tools = [dict(name=name, provision=dict(args=[name], status=0)) for name in ('first', 'second')]
        generated = script.provisioning(tools)
        self.assertEqual(script.parse(canonical.record_bytes(generated)), generated)
        self.assertEqual([t['output'][0]['call'] for t in generated['turns'][:-1]], ['first', 'second'])
        self.assertEqual(generated['turns'][0]['output'][0]['arguments'], dict(args=['first'], stdin=None))
        self.assertEqual(codex.report(generated['turns'][-1]['output'][0]['text']), dict(done=True, retried=[], unseen=[], surprises=[]))

    def test_stream_has_created_items_and_completed_usage(self):
        items = [dict(call='ledger', namespace='mcp__lab', arguments=dict(args=['task'], stdin=None)), dict(text='report')]
        usage = dict(input_tokens=100, output_tokens=10, input_tokens_details=dict(cached_tokens=0))
        body = script.stream(items, 'resp_0', usage)
        events = [json.loads(line[6:]) for line in body.decode().splitlines() if line.startswith('data: ')]
        self.assertEqual([e['type'] for e in events], ['response.created', 'response.output_item.done', 'response.output_item.done', 'response.completed'])
        self.assertEqual(events[1]['item']['namespace'], 'mcp__lab')
        self.assertEqual(events[-1]['response']['usage'], usage)

    def test_offered_and_admitted_on_a_real_codex_request(self):
        request = json.loads((Path(__file__).parent / 'vectors/responses-request.json').read_bytes())
        names = script.offered(request)
        self.assertEqual(names, ['exec_command', 'list_mcp_resource_templates', 'list_mcp_resources',
                                 'mcp__lab.ledger', 'read_mcp_resource', 'write_stdin'])
        self.assertEqual(script.admitted(names, [dict(name='ledger')]), [])
        self.assertEqual(script.admitted(names + ['invented', 'mcp__other.ledger'], []),
                         ['invented', 'mcp__lab.ledger', 'mcp__other.ledger'])


class ProvisionLife(unittest.TestCase):
    def state(self):
        from runner import life
        from .fixtures import fixture
        exp, _, settings, _, _ = fixture()
        exp.update(processes=[], actions=[], collect=[], checks=[], driver=dict(charter='charter', tools=[]))
        state = life.initial(exp, '1', settings['costs']['smoke'], {}, {})
        state, _ = life.step(state, dict(kind='validated', ms=0))
        return life.step(state, dict(kind='provisioned', ms=0))

    def test_provisioning_precedes_driver_and_has_no_drive_actions_or_meter(self):
        from runner import life
        state, effects = self.state()
        self.assertEqual(effects[-1], dict(op='provision_tools'))
        self.assertIsNone(state.data['driver']['started_ms'])
        self.assertIsNone(state.data['meter'])
        state, effects = life.step(state, dict(kind='provision_inventory', ms=1, inventory=dict(x=dict(state='observed'))))
        self.assertEqual(effects, [dict(op='driver_turn')])
        item = dict(type='mcpToolCall', server='lab', tool='tool', arguments=dict(args=[], stdin=None))
        state, _ = life.step(state, dict(kind='provision_call', ms=2, item=item))
        self.assertEqual(state.data['driver']['observed']['actions'], [])
        state, effects = life.step(state, dict(kind='tools_provisioned', ms=3, errors=[]))
        self.assertEqual(effects, [dict(op='start_driver')])
        self.assertIsNone(state.data['driver']['started_ms'])

    def test_timeout_and_inventory_errors_stop_provisioning_without_drive(self):
        from runner import life
        for event in (dict(kind='tick', ms=120000), dict(kind='provision_inventory', ms=1, inventory=dict(x=dict(state='mismatch')))):
            with self.subTest(event=event):
                state, _ = self.state()
                state, effects = life.step(state, event)
                self.assertEqual(effects, [dict(op='stop_driver')])
                self.assertTrue(state.data['errors'][0]['what'].startswith('provision/'))
                self.assertIsNone(state.data['driver']['started_ms'])
