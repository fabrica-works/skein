"""Pure vectors for the tool server's profile, views and wire decisions."""
import ast
import json
from pathlib import Path
import tomllib
import unittest
from runner import canonical, views


TOOL = dict(name='query', argv=['/experiment/client', 'fixed'], env={'ONLY': 'declared'},
            writes=['/cluster/client'], reads=['/cluster/endpoint'])


class DriverTools(unittest.TestCase):
    def request(self, method, params=None):
        return views.mcp_request(json.dumps(dict(jsonrpc='2.0', id=4, method=method,
                                                params={} if params is None else params)), [TOOL])

    def test_profile_roots_and_network(self):
        for source in (None, '/subject/source'):
            profile = tomllib.loads(views.profile_toml('/codex', source, '/run/work'))
            self.assertEqual(profile['default_permissions'], 'lab')
            expected = {':minimal': 'read', '/codex': 'read', '/run/work': 'write'}
            if source is not None:
                expected[source] = 'read'
            self.assertEqual(profile['permissions']['lab']['filesystem'], expected)
            self.assertEqual(profile['permissions']['lab']['network'], dict(enabled=False))

    def test_profile_quotes_paths(self):
        text = views.profile_toml('/a"b', '/s\\x', '/work/😀')
        self.assertIn('/a"b', tomllib.loads(text)['permissions']['lab']['filesystem'])
        self.assertIn('/work/😀', tomllib.loads(text)['permissions']['lab']['filesystem'])

    def test_views_mount_only_members_and_append_literal_args(self):
        argv = views.tool_argv('/bwrap', TOOL, '/run/work', ['$(evil)', '../outside'], ['/usr', '/etc'])
        for flag, path in (('--ro-bind', '/usr'), ('--ro-bind', '/etc'), ('--ro-bind', '/experiment/client'),
                           ('--ro-bind', '/cluster/endpoint'), ('--ro-bind', '/run/work'),
                           ('--bind', '/cluster/client')):
            self.assertIn([flag, path, path], [argv[i:i+3] for i in range(len(argv))])
        self.assertEqual(argv[argv.index('--')+1:], ['/usr/bin/env', '-i', '--', 'ONLY=declared',
                                                 *TOOL['argv'], '$(evil)', '../outside'])
        for flag in ('--unshare-pid', '--unshare-user', '--share-net', '--die-with-parent', '--clearenv'):
            self.assertIn(flag, argv)
        self.assertIn(['--tmpfs', '/tmp'], [argv[i:i+2] for i in range(len(argv))])
        self.assertIn(['--proc', '/proc'], [argv[i:i+2] for i in range(len(argv))])
        self.assertEqual(argv[argv.index('--clearenv')+1:argv.index('--')], ['--setenv', 'ONLY', 'declared'])

    def test_nested_mounts_narrow_outer_grant(self):
        tool = dict(TOOL, writes=['/cluster'], reads=['/cluster/check'])
        argv = views.tool_argv('/bwrap', tool, '/cluster/work', [], ['/usr'])
        self.assertLess(argv.index('--bind'), argv.index('/cluster/check'))
        self.assertIn(['--ro-bind', '/cluster/work', '/cluster/work'], [argv[i:i+3] for i in range(len(argv))])

    def test_initialize_tools_only(self):
        response, call = self.request('initialize')
        self.assertIsNone(call)
        self.assertEqual(response['result']['capabilities'], {'tools': {}})

    def test_list_schema(self):
        response, _ = self.request('tools/list')
        self.assertEqual([t['name'] for t in response['result']['tools']], ['query'])
        schema = response['result']['tools'][0]['inputSchema']
        self.assertEqual(schema['properties']['stdin']['type'], ['string', 'null'])
        self.assertFalse(schema['additionalProperties'])

    def test_call_then_broker_start(self):
        for stdin in (None, '', 'input\n', '\0'):
            response, call = self.request('tools/call', dict(name='query', arguments=dict(args=['a'], stdin=stdin)))
            self.assertIsNone(response)
            self.assertEqual(views.broker_start(call), dict(start=dict(tool='query', args=['a'], stdin=stdin is not None)))

    def test_invalid_call(self):
        for arguments in ({}, {'args': 'text'}, {'args': [4]}, {'args': ['\0']}, {'args': [], 'stdin': 4}, {'args': [], 'extra': True}):
            with self.subTest(arguments=arguments):
                response, call = self.request('tools/call', dict(name='query', arguments=arguments))
                self.assertEqual(response['error']['code'], -32602)
                self.assertIsNone(call)
        self.assertEqual(self.request('tools/call', dict(name='other', arguments=dict(args=[])))[0]['error']['code'], -32602)

    def test_unknown_methods_and_bad_frames(self):
        for method in ('resources/list', 'prompts/list', 'other'):
            self.assertEqual(self.request(method)[0]['error']['code'], -32601)
        for frame, code in ((b'{', -32700), (b'[]', -32600), (b'null', -32600), (b'\xff', -32700)):
            self.assertEqual(views.mcp_request(frame, [TOOL])[0]['error']['code'], code)
        self.assertEqual(views.mcp_request('{"jsonrpc":"2.0","method":"notifications/initialized"}', [TOOL]), (None, None))

    def test_broker_admission_and_refusal(self):
        for value in ({'go': 1}, {'refuse': 'inventory incomplete'}):
            self.assertEqual(views.broker_reply(json.dumps(value)), value)
        for value in ({'go': True}, {'go': -1}, {'go': '1'}, {'go': 1, 'refuse': 'no'}, {}, [], {'refuse': 3}):
            with self.subTest(value=value), self.assertRaises(ValueError):
                views.broker_reply(json.dumps(value))

    def test_result_text_and_error(self):
        result = views.call_result(4, dict(error='bound/driver_actions'), failed=True)
        self.assertTrue(result['result']['isError'])
        self.assertEqual(json.loads(result['result']['content'][0]['text']), dict(error='bound/driver_actions'))

    def test_tools_record_and_refusals(self):
        record = dict(format='lab-driver-tools/1', run='R', work='/work', source=None,
                      socket='/driver/broker.sock', tools=[TOOL])
        self.assertEqual(views.tools_record(canonical.record_bytes(record), 'R'), record)
        for delta in ({'run': 'other'}, {'work': '/'}, {'command_seconds': 2},
                      {'tools': [TOOL, TOOL]}, {'tools': [dict(TOOL, argv=['relative'])]}):
            with self.subTest(delta=delta), self.assertRaises(ValueError):
                views.tools_record(canonical.record_bytes(dict(record, **delta)), 'R')

    def test_noncanonical_tools_record_refused(self):
        for data in (b'{}', b'{\"a\":1,\"a\":1}\n', b'[]\n'):
            with self.subTest(data=data), self.assertRaises(ValueError):
                views.tools_record(data, 'R')

    def test_views_module_has_no_effect_imports(self):
        tree = ast.parse((Path(__file__).parents[2] / 'runner/views.py').read_text())
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                for item in node.names:
                    self.assertIn(item.name, {'json', 're'})
            if isinstance(node, ast.ImportFrom):
                self.assertEqual(node.level, 1)
                self.assertIsNone(node.module)
                self.assertEqual([item.name for item in node.names], ['canonical'])
            if isinstance(node, ast.Call) and isinstance(node.func, ast.Name):
                self.assertNotIn(node.func.id, {'open', 'exec', 'eval', '__import__'})
