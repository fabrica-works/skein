"""The landed paid proof runs against the scripted host, with no model or login."""
from contextlib import redirect_stderr, redirect_stdout
import importlib.machinery
import importlib.util
import io
import json
from pathlib import Path
import shutil
import subprocess
import unittest
from runner import canonical, cli, draws
from . import test_driver, test_runtime as native

PROOF = native.PACKAGE / 'tests/proof'
SUBJECT = PROOF / 'subjects/ledger'
loader = importlib.machinery.SourceFileLoader('process_ledger', str(SUBJECT / 'ledger'))
spec = importlib.util.spec_from_loader(loader.name, loader)
ledger = importlib.util.module_from_spec(spec)
loader.exec_module(ledger)


class Squares(unittest.TestCase):
    setUpClass = classmethod(native.Runtime.setUpClass.__func__)
    tearDown = native.Runtime.tearDown
    cli = test_driver.Driver.cli

    def setUp(self):
        test_driver.Driver.setUp(self)
        shutil.rmtree(self.directory)
        shutil.copytree(PROOF / 'squares', self.directory)
        self.settings['costs'] = dict(squares=dict(ready_seconds=2, command_seconds=2, stop_seconds=2,
            driver_seconds=2, driver_actions=5, driver_input_tokens=100))
        (self.lab_root / 'settings.json').chmod(0o600)
        (self.lab_root / 'settings.json').write_bytes(canonical.record_bytes(self.settings))
        repository = self.root / 'repository'
        commit = subprocess.run([str(SUBJECT / 'repository'), str(repository)],
                                capture_output=True, text=True, check=True).stdout.strip()
        stdout, stderr = io.StringIO(), io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            status = cli.main(['subject', '--repository', str(repository), '--commit', commit,
                               '--artifact', 'ledger=' + str(SUBJECT / 'ledger')], lab_root=self.lab_root)
        self.assertEqual(status, 0, stderr.getvalue())
        self.subject = stdout.getvalue().strip()

    def story(self, offset, status):
        values = ledger.numbers(draws.draw('1', 'ledger'))
        expected = ledger.expected(values)
        program = ('def odd_squares(numbers):\n'
                   '    """Return the sum of the squares of the odd numbers."""\n'
                   '    total = 0\n'
                   '    for n in numbers:\n'
                   '        if n % 2 == 1:\n'
                   '            total += n * n\n'
                   '    return total\n'
                   f'print(odd_squares({values!r}))\n')
        def tool(args, stdin=None):
            return dict(type='mcpToolCall', server='lab', tool='ledger', arguments=dict(args=args, stdin=stdin))
        self.script.write_text(json.dumps(dict(items=[tool(['task']),
            dict(type='commandExecution', argv=['/bin/sh', '-c', "python3 - <<'EOF'\n" + program + 'EOF\n']),
            tool(['program'], program), tool(['answer', str(expected + offset)])])))
        result, run, stderr = self.cli('run', seed='1')
        root = next((self.lab_root / 'keys/squares').glob('*/runs/' + run))
        record = canonical.loads((root / 'run.json').read_bytes())
        self.assertEqual(result, status, stderr + json.dumps(record['failures']) + json.dumps(record['errors']))
        events = [json.loads(line) for line in (root / 'events.jsonl').read_text().splitlines()]
        containment = next(e for e in events if e['event'] == 'containment')
        self.assertFalse(Path(containment['cgroup']).exists())
        self.assertFalse((root / 'live.json').exists())
        calls = [dict(verb='task'), dict(verb='program', bytes=len(program.encode())),
                 dict(verb='answer', value=expected + offset, verdict='wrong' if offset else 'accepted')]
        observations = json.loads((root / 'collected/observations/observations.json').read_text())
        self.assertEqual(observations, dict(numbers=values, expected=expected, calls=calls))
        evidence = root / 'scratch/ledger/calls.jsonl'
        self.assertEqual([json.loads(line) for line in evidence.read_text().splitlines()], calls)
        command = next(a for a in record['driver']['actions'] if a['kind'] == 'commandExecution')
        self.assertEqual(command['end']['exitCode'], 0)
        self.assertEqual(command['end']['aggregatedOutput'], str(expected) + '\n')
        self.assertEqual(record['processes']['ledger']['exit']['status'], 0)
        self.assertEqual(record['errors'], [])
        return record

    def test_landed_proof_passes_on_scripted_host(self):
        record = self.story(0, 0)
        self.assertEqual(record['outcome'], 'pass')
        self.assertEqual(record['failures'], [])

    def test_wrong_answer_has_stable_check_signature(self):
        record = self.story(1, 1)
        self.assertEqual(record['outcome'], 'fail')
        self.assertEqual([f['signature'] for f in record['failures']], ['check/answer/answer/wrong'])


class Subject(unittest.TestCase):
    def test_repository_is_repeatable_and_refuses_nonempty_directory(self):
        import tempfile
        with tempfile.TemporaryDirectory(prefix='ledger-repository-') as temporary:
            root = Path(temporary)
            commits = [subprocess.run([str(SUBJECT / 'repository'), str(root / name)],
                                      capture_output=True, text=True, check=True).stdout.strip()
                       for name in ('first', 'second')]
            self.assertEqual(commits[0], commits[1])
            occupied = root / 'occupied'
            occupied.mkdir()
            (occupied / '\n').write_text('keep')
            result = subprocess.run([str(SUBJECT / 'repository'), str(occupied)], capture_output=True)
            self.assertEqual(result.returncode, 2)
            self.assertEqual((occupied / '\n').read_text(), 'keep')

    def test_program_size_bound_and_refused_requests_leave_no_calls(self):
        import select
        import tempfile
        with tempfile.TemporaryDirectory(prefix='ledger-protocol-') as temporary:
            root = Path(temporary)
            with subprocess.Popen([native.PYTHON, '-I', '-B', '-S', str(SUBJECT / 'ledger'),
                    'serve', '--seed', '0', '--state', str(root)], stdin=subprocess.DEVNULL,
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE) as process:
                try:
                    # Readiness comes from the server's line, within the process stories' two-second guide.
                    if not select.select([process.stdout], [], [], 2)[0]:
                        self.fail('ledger did not report readiness')
                    line = process.stdout.readline().decode().strip()
                    endpoint = ledger.address(line.removeprefix('ready '))
                    self.assertEqual(ledger.exchange(endpoint, 'unknown'), b'refused\n')
                    self.assertEqual(ledger.exchange(endpoint, 'answer one'), b'refused\n')
                    program = b'x' * ledger.MAX_PROGRAM
                    self.assertEqual(ledger.exchange(endpoint, 'program', program), b'received 65536 bytes\n')
                    self.assertEqual(ledger.exchange(endpoint, 'program', program + b'x'), b'too large\n')
                    self.assertEqual((root / 'program').read_bytes(), program)
                    observations = json.loads(ledger.exchange(endpoint, 'observations'))
                    calls = [dict(verb='program', bytes=65536)]
                    self.assertEqual(observations['calls'], calls)
                    self.assertEqual([json.loads(row) for row in (root / 'calls.jsonl').read_text().splitlines()], calls)
                finally:
                    process.terminate()
                    try:
                        process.wait(timeout=2)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
                self.assertEqual(process.returncode, 0)
