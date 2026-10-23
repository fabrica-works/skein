"""The real Codex command sandbox and MCP stack against scripted Responses only."""
import hashlib
import json
from pathlib import Path
import subprocess
import time
import unittest
from .test_m2_tree import LAB, ROOT, RunnerCase, canonical

CODEX = Path('/home/free/.codex/packages/standalone/releases/0.162.1-x86_64-unknown-linux-musl/bin/codex')
PROOF = ROOT / 'scripts/lab/tests/proof'


@unittest.skipUnless(CODEX.is_file(), 'Codex 0.162.1 is absent')
class RealRehearsal(RunnerCase):
    def test_correct_and_wrong_on_the_real_stack_without_a_login(self):
        started = time.monotonic()
        repository = self.base / 'ledger-repository'
        result = self.command([PROOF / 'subjects/ledger/repository', repository])
        self.assertEqual(result.returncode, 0, result.stderr)
        subject = self.command([LAB, 'subject', '--repository', repository, '--commit', result.stdout.strip().decode(),
            '--artifact', 'ledger=' + str(PROOF / 'subjects/ledger/ledger')])
        self.assertEqual(subject.returncode, 0, subject.stderr)
        settings = dict(format='lab-settings/1', costs=dict(squares=dict(ready_seconds=5, command_seconds=15,
            stop_seconds=2, driver_seconds=15, driver_actions=5, driver_input_tokens=1000)),
            driver=dict(program=dict(path=str(CODEX), sha256='sha256:' + hashlib.sha256(CODEX.read_bytes()).hexdigest(),
                                     version='0.162.1'), model='gpt-5.4', effort='low'), triager=None,
            codex_home=str(self.base / 'paid-home'))
        (self.store / 'settings.json').write_bytes(canonical(settings))
        (self.store / 'pause').write_text('paused')
        for mode, status in (('correct', 0), ('wrong', 1)):
            with self.subTest(mode=mode):
                before = time.monotonic()
                result = self.command([LAB, 'run', PROOF / 'squares', '--subject', subject.stdout.strip().decode(),
                    '--seed', '1', '--rehearse', PROOF / 'squares-rehearsal' / (mode + '.json')], timeout=40)
                run = self.store / 'rehearsals/squares' / result.stdout.strip().decode()
                self.assertEqual(result.returncode, status, result.stderr.decode(errors='replace'))
                record = json.loads((run / 'run.json').read_bytes())
                self.assertEqual(record['errors'], [])
                self.assertEqual(record['outcome'], 'pass' if mode == 'correct' else 'fail')
                checks = [f['signature'] for f in record['failures'] if f['kind'] == 'check']
                self.assertEqual(checks, [] if mode == 'correct' else ['check/answer/answer/wrong'])
                observed = json.loads((run / 'collected/observations/observations.json').read_bytes())
                commands = [a for a in record['driver']['actions'] if a['kind'] == 'commandExecution']
                self.assertEqual([a['end']['aggregatedOutput'] for a in commands], [str(observed['expected']) + '\n'])
                self.assertEqual(record['driver']['provision']['offered'], ['exec_command', 'list_mcp_resource_templates',
                    'list_mcp_resources', 'mcp__lab.ledger', 'read_mcp_resource', 'write_stdin'])
                key = self.store / 'keys/squares' / record['key']
                self.assertFalse((key / 'runs').exists())
                self.assertFalse((key / 'evidence.jsonl').exists())
                self.assertFalse((key / 'verdict.json').exists())
                self.assertFalse((self.base / 'paid-home').exists())
                self.assertFalse((run / 'live.json').exists())
                self.assertEqual(list((self.store / 'findings').glob('F-*')), [])
                print(f'real rehearsal {mode}: {time.monotonic() - before:.3f} s', flush=True)
        print(f'real rehearsal QA: {time.monotonic() - started:.3f} s', flush=True)
