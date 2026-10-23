"""A second host proves the tool surface before the drive's host starts."""
import json
import time
import unittest
from . import test_driver


class Provision(unittest.TestCase):
    setUpClass = classmethod(test_driver.Driver.setUpClass.__func__)
    setUp = test_driver.Driver.setUp
    tearDown = test_driver.Driver.tearDown
    cli = test_driver.Driver.cli
    records = test_driver.Driver.records
    assert_empty = test_driver.Driver.assert_empty
    story = test_driver.Driver.story

    def run(self, result=None):
        started = time.monotonic()
        try:
            return super().run(result)
        finally:
            print(f'{self.id()}: {time.monotonic() - started:.3f} s', flush=True)

    def test_provisioning_is_recorded_apart_from_drive_actions(self):
        record = self.story(dict(items=[dict(type='mcpToolCall', server='lab', tool='echo',
                                            arguments=dict(args=['drive'], stdin=None))]), 0)
        provision = record['driver']['provision']
        self.assertEqual([(c['tool'], c['args'], c['status']) for c in provision['calls']], [('echo', [], 0)])
        starts = [json.loads(line) for line in (self.root_run / 'driver/provision/calls.jsonl').read_text().splitlines()]
        self.assertEqual([(c['event'], c['n']) for c in starts], [('start', 1), ('end', 1)])
        calls = [json.loads(line) for line in (self.root_run / 'driver/calls.jsonl').read_text().splitlines()]
        self.assertEqual(calls[0]['args'], ['drive'])
        self.assertEqual(record['usage']['driver']['actions'], 1)
        self.assertEqual(record['usage']['driver']['tokens']['input'], None)
        config = (self.root_run / 'driver/config.toml').read_text().split('[model_providers.lab-openai]')[0]
        self.assertEqual(config, (self.root_run / 'driver/provision/config.toml').read_text().split('[model_providers.lab-openai]')[0])
        self.assertEqual(provision['invocation']['env']['CODEX_HOME'], str(self.root_run / 'driver/provision/home'))
        self.assertFalse((self.root_run / 'driver/provision/broker.sock').exists())

    def test_nonmatching_tool_exit_prevents_drive(self):
        self.exp['driver']['tools'][0]['argv'] = ['/usr/bin/false']
        self.exp['checks'] = [dict(name='unfinished', argv=['/usr/bin/false'], env={})]
        record = self.story({}, 4)
        self.assertIn('provision/echo/exit:1', [e['what'] for e in record['errors']])
        self.assertFalse(self.program.with_suffix('.turn').exists())
        self.assertEqual(record['usage']['driver']['actions'], 0)
        self.assertIsNone(record['driver']['invocation'])

    def test_an_expected_nonzero_status_is_accepted(self):
        self.exp['driver']['tools'][0].update(argv=['/usr/bin/false'], provision=dict(args=[], status=1))
        record = self.story({}, 0)
        self.assertEqual(record['driver']['provision']['calls'][0]['status'], 1)

    def test_unadmitted_offered_name_prevents_drive(self):
        record = self.story(dict(provision=dict(offered=['invented'])), 4)
        self.assertIn('provision/offered/invented', [e['what'] for e in record['errors']])
        self.assertIn('invented', record['driver']['provision']['offered'])
        self.assertFalse(self.program.with_suffix('.turn').exists())

    def test_provisioning_inventory_mismatch_prevents_drive(self):
        record = self.story(dict(provision=dict(inventory={'thread/start': dict(instructionSources=['invented'])})), 4)
        self.assertIn('provision/inventory', [e['what'] for e in record['errors']])
        self.assertEqual(record['driver']['provision']['inventory']['instruction_files']['state'], 'mismatch')
        self.assertFalse(self.program.with_suffix('.turn').exists())

    def test_signal_prevents_drive(self):
        self.exp['driver']['tools'][0].update(argv=['/usr/bin/python3.13', '-I', '-B', '-S', '-c',
            'import os, signal; os.kill(os.getpid(), signal.SIGTERM)'])
        record = self.story({}, 4)
        self.assertIn('provision/echo/signal:SIGTERM', [e['what'] for e in record['errors']])
        self.assertEqual(record['driver']['provision']['calls'][0]['signal'], 'SIGTERM')
        self.assertFalse(self.program.with_suffix('.turn').exists())

    def test_timeout_and_host_error_prevent_drive(self):
        self.settings['costs']['smoke'].update(command_seconds=1, stop_seconds=1)
        self.exp['driver']['tools'][0].update(argv=['/usr/bin/sleep'], provision=dict(args=['5'], status=0))
        record = self.story({}, 4)
        self.assertIn('provision/timeout', [e['what'] for e in record['errors']])
        self.assertFalse(self.program.with_suffix('.turn').exists())

    def test_host_error_retains_missing_call(self):
        record = self.story(dict(provision=dict(rpc_error='config/read')), 4)
        self.assertIn('provision/host', [e['what'] for e in record['errors']])
        self.assertIn('provision/echo/missing', [e['what'] for e in record['errors']])
        self.assertFalse(self.program.with_suffix('.turn').exists())
