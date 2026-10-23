"""Control sequences and broker decisions on supplied messages, no child or clock."""
import json
from types import SimpleNamespace
import unittest
from runner import codex, driver, life, transport
from .test_codex import drive, inventory


class AppServer(unittest.TestCase):
    def test_sequence_pages_and_turn_schema(self):
        received, inventories = [], []
        client = transport.AppServer('/work', 'preamble\ncharter', {'type': 'object'}, received.append, inventories.append)
        methods = []
        values = inventory()
        values['thread/start']['thread'] = dict(id='thread')
        while client.pending is not None:
            number, method = client.pending
            methods.append(method)
            result = dict(userAgent='script') if method == 'initialize' else values[method]
            if method == 'mcpServerStatus/list':
                if methods.count(method) == 1:
                    result = dict(data=[], nextCursor='second')
                else:
                    result = result[0]
            client.accept(json.dumps(dict(id=number, result=result)))
        self.assertEqual(methods, ['initialize', 'thread/start', 'mcpServerStatus/list', 'mcpServerStatus/list',
                                   'app/installed', 'plugin/list', 'hooks/list', 'skills/list', 'config/read'])
        self.assertEqual(len(inventories), 1)
        self.assertFalse(client.turn_sent)
        client.start_turn()
        sent = [json.loads(line) for line in client.outgoing.splitlines()]
        self.assertEqual(sent[0]['params']['capabilities'], dict(experimentalApi=True))
        self.assertEqual(sent[1]['method'], 'initialized')
        self.assertEqual(sent[-1]['params']['outputSchema'], {'type': 'object'})
        self.assertEqual(sent[-1]['params']['input'][0]['text'], 'preamble\ncharter')
        with self.assertRaises(RuntimeError):
            client.start_turn()

    def test_protocol_errors_and_incomplete_inventory(self):
        for response in (dict(id=99, result={}), dict(id=1, error=dict(message='host failure')),
                         dict(id=1, result=None), dict(id=1, method='approval/request')):
            client = transport.AppServer('/work', '', {}, lambda m: None, lambda m: None)
            with self.subTest(response=response), self.assertRaises(RuntimeError):
                client.accept(json.dumps(response))
        client = transport.AppServer('/work', '', {}, lambda m: None, lambda m: None)
        client.pending = (2, 'mcpServerStatus/list')
        with self.assertRaisesRegex(RuntimeError, 'incomplete MCP'):
            client.accept(json.dumps(dict(id=2, result=dict(data=[]))))
        client.pending = (3, 'mcpServerStatus/list')
        client.cursors.add('repeat')
        with self.assertRaisesRegex(RuntimeError, 'cursor'):
            client.accept(json.dumps(dict(id=3, result=dict(data=[], nextCursor='repeat'))))


class Broker(unittest.TestCase):
    def world(self):
        entries = []
        run = SimpleNamespace(state=drive(), exp=dict(driver=dict(tools=[dict(name='echo')])),
                              lab=SimpleNamespace(append_line=lambda path, value: entries.append((path, value))),
                              base='run', ms=lambda: 1, cost=dict(command_seconds=3), timeline=lambda *a, **k: None)
        run.feed = lambda kind, **fields: self.feed(run, kind, fields)
        run.file_record = lambda path: dict(path=path, bytes=3, sha256='sha256:' + 'a' * 64)
        run.spec = {}
        instance = driver.Driver(run)
        connection = dict(pending=[], active=set(), out=b'')
        return instance, connection, entries

    def feed(self, run, kind, fields):
        run.state, _ = life.step(run.state, dict(kind=kind, ms=run.ms(), **fields))

    def test_calls_hold_until_inventory_and_pair_with_host_action(self):
        instance, connection, entries = self.world()
        start = dict(tool='echo', args=['word'], stdin=True)
        instance.broker(connection, dict(start=start))
        self.assertEqual(connection['pending'], [(start, 1, 1)])
        self.assertEqual(connection['out'], b'')
        self.assertEqual(entries, [])
        self.assertEqual(instance.run.state.data['driver']['observed']['actions'][0]['n'], 1)
        run = instance.run
        run.feed('driver_event', message=dict(method='item/started', params=dict(item=dict(
            type='mcpToolCall', id='call', server='lab', tool='echo', arguments=dict(args=['word'], stdin='input')))))
        run.feed('driver_inventory', inventory=dict(tools=dict(state='observed')))
        instance.admit(connection)
        self.assertEqual(json.loads(connection['out']), dict(go=1))
        self.assertEqual(len(run.state.data['driver']['observed']['actions']), 1)
        instance.broker(connection, dict(end=1, status=0, signal=None))
        self.assertEqual([v['event'] for _, v in entries], ['start', 'end'])
        self.assertEqual(entries[1][1]['stdin']['bytes'], 3)
        self.assertEqual(entries[1][1]['stdout']['sha256'], 'sha256:' + 'a' * 64)
        self.assertEqual(entries[1][1]['stderr']['path'], 'driver/calls/1.stderr')
        self.assertEqual(connection['active'], set())

    def test_broker_action_past_bound_refuses_go(self):
        instance, connection, entries = self.world()
        instance.run.state.data['cost']['driver_actions'] = 1
        instance.run.feed('driver_inventory', inventory=dict(tools=dict(state='observed')))
        for _ in range(2):
            instance.broker(connection, dict(start=dict(tool='echo', args=[], stdin=False)))
            connection['active'].clear()
        self.assertEqual([json.loads(line) for line in connection['out'].splitlines()],
                         [dict(go=1), dict(refuse='driver bound passed')])
        self.assertEqual(len(entries), 1)
        self.assertEqual(len(instance.run.state.data['driver']['observed']['actions']), 2)

    def test_held_call_keeps_its_place_before_later_commands(self):
        instance, connection, _ = self.world()
        instance.broker(connection, dict(start=dict(tool='echo', args=[], stdin=False)))
        instance.run.feed('driver_event', message=dict(method='item/started', params=dict(
            item=dict(type='commandExecution', id='later'))))
        instance.run.feed('driver_inventory', inventory=dict(tools=dict(state='observed')))
        instance.admit(connection)
        self.assertEqual(json.loads(connection['out']), dict(go=1))
        self.assertEqual([a['kind'] for a in instance.run.state.data['driver']['observed']['actions']],
                         ['mcpToolCall', 'commandExecution'])

    def test_pipelined_calls_hold_and_finish_out_of_order(self):
        instance, connection, entries = self.world()
        for arg in ('first', 'second'):
            instance.broker(connection, dict(start=dict(tool='echo', args=[arg], stdin=False)))
        self.assertEqual(connection['out'], b'')
        instance.run.feed('driver_inventory', inventory=dict(tools=dict(state='observed')))
        instance.admit(connection)
        instance.admit(connection)
        self.assertEqual([json.loads(line) for line in connection['out'].splitlines()], [dict(go=1), dict(go=2)])
        instance.broker(connection, dict(end=2, status=0, signal=None))
        instance.broker(connection, dict(end=1, status=None, signal='SIGTERM'))
        self.assertEqual([(v['event'], v['n']) for _, v in entries], [('start', 1), ('start', 2), ('end', 2), ('end', 1)])
        self.assertEqual(connection['active'], set())
        self.assertEqual(instance.command_deadlines, {})

    def test_command_deadline_refuses_another_call_at_the_bound(self):
        instance, connection, entries = self.world()
        instance.run.feed('driver_inventory', inventory=dict(tools=dict(state='observed')))
        instance.broker(connection, dict(start=dict(tool='echo', args=['first'], stdin=False)))
        instance.run.ms = lambda: 3001
        instance.broker(connection, dict(start=dict(tool='echo', args=['second'], stdin=False)))
        self.assertEqual([json.loads(line) for line in connection['out'].splitlines()],
                         [dict(go=1), dict(refuse='driver ended')])
        self.assertEqual([v['event'] for _, v in entries], ['start'])
        self.assertEqual(instance.run.state.data['bounds'], [dict(name='command_seconds', ms=3001)])

    def test_broker_rejects_unadmitted_tool_and_unpaired_end(self):
        for value in (dict(start=dict(tool='other', args=[], stdin=False)), dict(end=1, status=0, signal=None),
                      dict(start=dict(tool='echo', args='string', stdin=False))):
            instance, connection, _ = self.world()
            with self.subTest(value=value), self.assertRaises(RuntimeError):
                instance.broker(connection, value)


class ProvisionSummary(unittest.TestCase):
    def test_broker_signal_stays_distinct_from_an_exit_status(self):
        import tempfile
        from pathlib import Path
        with tempfile.TemporaryDirectory() as temporary:
            observed = codex.initial()
            observed['turns'] = [dict(completed_ms=1)]
            run = SimpleNamespace(base='run', spec={}, ms=lambda: 2,
                exp=dict(driver=dict(tools=[dict(name='tool', provision=dict(args=['help'], status=0))])),
                lab=SimpleNamespace(root=Path(temporary)),
                state=SimpleNamespace(data=dict(tool_provision=dict(observed=observed, inventory={}))))
            session = driver.Driver(run, provisioning=True)
            session.started_ms, session.host_end = 0, dict(status=0)
            ref = dict(path='driver/provision/calls/1.stdout', bytes=0, sha256='sha256:' + '0' * 64)
            session.calls = {1: dict(tool='tool', args=['help'], stdin=False,
                end=dict(status=None, signal='SIGTERM', stdout=ref, stderr=ref))}
            record, errors = session.finish_provision()
            self.assertEqual(record['calls'][0]['signal'], 'SIGTERM')
            self.assertNotIn('status', record['calls'][0])
            self.assertEqual([e['what'] for e in errors], ['provision/tool/signal:SIGTERM'])
