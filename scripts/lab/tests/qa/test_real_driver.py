"""The scripted app-server exercises the driver's public evidence boundary."""
import json

from .test_real_inputs import RealCase


def call(tool='echo', args=None, **extra):
    return dict(type='mcpToolCall', server='lab', tool=tool,
                arguments=dict(args=args or [], stdin=None), **extra)


class DriverQA(RealCase):
    def test_probe_failure_prevents_cluster_start(self):
        # lab.md 6.2: "otherwise the run ends in error before any process starts".
        host = self.driver(dict(probe_failure='canary_read'))
        self.exp['processes'] = [self.service()]
        record, _, result = self.execute(self.subject())
        self.assertEqual(record['outcome'], 'error', record)
        self.assertEqual(result.returncode, 4, record)
        self.assertFalse(any(p.get('started_ms') is not None for p in record['processes'].values()))
        self.assertTrue(host.with_suffix('.probe').is_file())
        self.assertFalse(host.with_suffix('.turn').exists())

    def test_inventory_mismatch_blocks_turn(self):
        # lab.md 6.1: "inventory differs ends the run in error".
        host = self.driver(dict(inventory={'thread/start': dict(
            instructionSources=[dict(path='/invented/instructions', kind='user')])}))
        record, _, result = self.execute(self.subject())
        self.assertEqual(result.returncode, 4, record)
        self.assertEqual(record['outcome'], 'error', record)
        self.assertFalse(host.with_suffix('.turn').exists())

    def test_unobserved_items_and_unknown_usage_are_not_absence_or_zero(self):
        # lab.md 6.1: "unobserved, never as absent"; 6.5: "null ... never 0".
        self.driver({})
        record, _, result = self.execute(self.subject())
        self.assertEqual(result.returncode, 0, record)
        inventory = record['driver']['inventory']
        self.assertEqual(inventory['prompts']['state'], 'unobserved')
        self.assertEqual(inventory['built_in_tools']['state'], 'unobserved')
        self.assertIsNone(record['usage']['driver']['tokens']['input'])
        self.assertIsNone(record['usage']['driver']['tokens']['cached'])
        self.assertIsNone(record['usage']['driver']['tokens']['output'])
        self.assertIsNone(record['usage']['windows'])

    def test_paginated_inventory_and_early_call_count_once(self):
        # lab.md 6.1: "Calls ... held until the observation is complete";
        # 6.3: "a tool call observed in both is one action".
        host = self.driver(dict(pages=True, items=[call(args=['invented'], early=True)]))
        self.exp['driver']['tools'] = [dict(name='echo', argv=['/usr/bin/echo'], env={}, writes=[], reads=[])]
        record, run, result = self.execute(self.subject())
        self.assertEqual(result.returncode, 0, record)
        self.assertTrue(host.with_suffix('.held').is_file())
        self.assertEqual(record['usage']['driver']['actions'], 1)
        calls = [json.loads(line) for line in (run / 'driver/calls.jsonl').read_text().splitlines()]
        self.assertEqual(len(calls), 2, calls)
        self.assertEqual(calls[0]['n'], calls[1]['n'])
        self.assertEqual((run / 'driver/calls/1.stdout').read_text(), 'invented\n')
        transcript = record['driver']['files']['transcript.jsonl']
        self.assertIn('invented', (run / transcript['path']).read_text())

    def test_tool_stdin_and_outputs_have_verified_evidence(self):
        # lab.md 6.3: "Every call is recorded ... stdout's and stderr's files
        # with their sizes and digests".
        from .test_real_inputs import digest
        item = call(tool='echo')
        item['arguments']['stdin'] = 'invented-input\n'
        self.driver(dict(items=[item]))
        self.exp['driver']['tools'] = [dict(name='echo', argv=['/usr/bin/cat'], env={}, writes=[], reads=[])]
        record, run, result = self.execute(self.subject())
        self.assertEqual(result.returncode, 0, record)
        calls = [json.loads(line) for line in (run / 'driver/calls.jsonl').read_text().splitlines()]
        self.assertEqual(len(calls), 2, calls)
        for ref in (calls[1]['stdout'], calls[1]['stderr']):
            path = run / ref['path']
            self.assertEqual(ref['bytes'], path.stat().st_size)
            self.assertEqual(ref['sha256'], digest(path))
        self.assertEqual((run / 'driver/calls/1.stdin').read_text(), 'invented-input\n')
        self.assertEqual((run / 'driver/calls/1.stdout').read_text(), 'invented-input\n')

    def test_windows_last_snapshot_is_sorted_without_counting_actions(self):
        # lab.md 6.5: "the account's windows as the host last reported them".
        self.driver(dict(windows=[
            dict(primary=dict(windowDurationMins=300, usedPercent=1, resetsAt=12)),
            dict(primary=dict(windowDurationMins=10080, usedPercent=17, resetsAt=456),
                 secondary=dict(windowDurationMins=300, usedPercent=8, resetsAt=123))]))
        record, _, result = self.execute(self.subject())
        self.assertEqual(result.returncode, 0, record)
        self.assertEqual(record['usage']['windows'], [dict(minutes=300, used_percent=8, resets_at=123),
            dict(minutes=10080, used_percent=17, resets_at=456)])
        self.assertEqual(record['usage']['driver']['actions'], 0)

    def test_cached_tokens_count_and_equal_bound_passes(self):
        # lab.md 6.5: "input tokens ... cached ones included"; "pass the bound".
        self.driver(dict(usage=[dict(inputTokens=1, cachedInputTokens=1, outputTokens=0),
                                dict(inputTokens=2, cachedInputTokens=2, outputTokens=1)]))
        record, _, result = self.execute(self.subject())
        self.assertEqual(result.returncode, 0, record)
        self.assertEqual(record['usage']['driver']['tokens']['input'], 2)
        self.assertEqual(record['usage']['driver']['tokens']['cached'], 2)
        self.assertEqual(record['usage']['driver']['tokens']['output'], 1)
        self.assertNotIn('bound/driver_input_tokens', self.signatures(record))

    def test_token_overshoot_keeps_late_usage_and_releases(self):
        # lab.md 6.5: "overshoot ... is recorded"; 4.8: release "a bound".
        self.driver(dict(usage=[dict(inputTokens=3, cachedInputTokens=2, outputTokens=1)]))
        self.exp['processes'] = [self.service()]
        marker = self.base / 'released'
        self.exp['release'] = [self.command_spec('mark', '/usr/bin/touch', str(marker))]
        record, _, result = self.execute(self.subject())
        self.assertEqual(result.returncode, 1, record)
        self.assertIn('bound/driver_input_tokens', self.signatures(record))
        self.assertEqual(record['usage']['driver']['tokens']['input'], 3)
        self.assertEqual(record['usage']['driver']['input_tokens_overshoot'], 1)
        self.assertTrue(marker.is_file())

    def test_report_false_and_unseen_are_separate_use_failures(self):
        # lab.md 6.4: "done: false and each unseen are failures of kind use".
        self.driver(dict(report=dict(done=False, retried=[],
            unseen=[dict(step='first', looked_for='button'), dict(step='second', looked_for='link')],
            surprises=[])))
        record, _, result = self.execute(self.subject())
        self.assertEqual(result.returncode, 1, record)
        self.assertEqual(len([f for f in record['failures'] if f['kind'] == 'use']), 3, record)

    def test_retry_and_surprise_are_evidence(self):
        # lab.md 6.4: "retried and surprises ... not failures".
        report = dict(done=True, retried=[dict(step='first', tries=[dict(tried='again', why='missed')])],
                      unseen=[], surprises=['unexpected label'])
        self.driver(dict(report=report))
        record, run, result = self.execute(self.subject())
        self.assertEqual(result.returncode, 0, record)
        self.assertEqual(json.loads((run / 'driver/report.json').read_bytes()), report)

    def test_missing_report_is_incomplete(self):
        # lab.md 5.5: "required report is missing or malformed though its host ended well".
        self.driver(dict(report=None))
        record, _, result = self.execute(self.subject())
        self.assertEqual(result.returncode, 3, record)
        self.assertEqual(record['outcome'], 'incomplete')

    def test_malformed_report_is_incomplete(self):
        # lab.md 5.5: "required report is missing or malformed though its host ended well".
        self.driver(dict(report='not a report'))
        record, _, result = self.execute(self.subject())
        self.assertEqual(result.returncode, 3, record)
        self.assertEqual(record['outcome'], 'incomplete')

    def test_commands_and_calls_share_action_bound(self):
        # lab.md 6.5: "commands and its tool calls together"; "Past either ... stopped".
        self.driver(dict(items=[dict(type='commandExecution', argv=['/usr/bin/true']),
                                call(args=['first']), call(args=['past-bound'])]))
        self.exp['driver']['tools'] = [dict(name='echo', argv=['/usr/bin/echo'], env={}, writes=[], reads=[])]
        record, _, result = self.execute(self.subject())
        self.assertEqual(result.returncode, 1, record)
        self.assertIn('bound/driver_actions', self.signatures(record))
        self.assertEqual(record['usage']['driver']['actions'], 3)

    def test_unadmitted_item_ends_in_error(self):
        # lab.md 6.1: "An action kind outside the admitted inventory ends ... in error".
        self.driver(dict(items=[dict(type='webSearch', id='unexpected', query='invented')]))
        record, _, result = self.execute(self.subject())
        self.assertEqual(result.returncode, 4, record)
        self.assertEqual(record['outcome'], 'error')
