"""Design-only QA: awaited lines and capture scope, lab.md 4.2/4.5/5.6."""
import json

from tests.qa.test_m2_tree import RunnerCase


class AwaitQA(RunnerCase):
    def action(self, name, mode, *args, awaited=None, stdin=None):
        result = dict(name=name, argv=['/usr/bin/python3', '-I', '-B', '-S',
                      '{experiment}/participant.py', mode, *args], env={})
        if awaited is not None:
            result['await'] = awaited
        if stdin is not None:
            result['stdin'] = stdin
        return result

    @staticmethod
    def awaited(prefix='fresh '):
        return dict(process='service', stream='stdout', prefix=prefix, capture='word')

    def test_fresh_capture_reaches_next_action_and_evidence(self):
        self.exp['processes'] = [self.service()]
        self.exp['actions'] = [
            self.action('fresh', 'request-fresh', '{proc.service.ready.address}', awaited=self.awaited()),
            self.action('consume', 'capture-stdin', stdin='{action.fresh.word}\n')]
        record, directory = self.run_lab(self.subject())
        self.assertEqual(record['outcome'], 'pass', record)
        command = next(c for c in record['commands'] if c['name'] == 'fresh')
        self.assertEqual(command['awaited']['captures'], {'word': 'invented-word'})
        self.assertIsInstance(command['awaited']['ms'], int)
        events = [json.loads(line) for line in (directory / 'events.jsonl').read_bytes().splitlines()]
        matches = [e for e in events if e['event'] == 'awaited' and e['name'] == 'fresh']
        self.assertEqual(len(matches), 1)
        self.assertEqual(matches[0]['captures'], {'word': 'invented-word'})
        self.assertIn(b'fresh invented-word\n', (directory / 'proc/service/stdout').read_bytes())

    def test_old_ready_line_cannot_satisfy_await(self):
        self.exp['processes'] = [self.service()]
        self.exp['actions'] = [dict(name='old', argv=['/bin/true'], env={}, **{'await': self.awaited('ready ')}),
                               self.action('consume', 'capture-stdin', stdin='{action.old.word}\n')]
        record, _ = self.run_lab(self.subject())
        self.assertIn('bound/command_seconds/old', [f['signature'] for f in record['failures']])
        command = next(c for c in record['commands'] if c['name'] == 'old')
        self.assertIsNone(command['awaited'])
        self.assertTrue(any(x['check'] == 'actions/consume' for x in record['undecided']), record)

    def test_nonzero_action_does_not_wait_for_missing_line(self):
        self.exp['processes'] = [self.service()]
        self.exp['actions'] = [dict(name='refused', argv=['/bin/false'], env={}, **{'await': self.awaited()})]
        record, _ = self.run_lab(self.subject())
        signatures = [f['signature'] for f in record['failures']]
        self.assertIn('action/refused/exit:1', signatures)
        self.assertNotIn('bound/command_seconds/refused', signatures)
        self.assertIsNone(record['commands'][0]['awaited'])

    def test_await_fields_are_validated_independently(self):
        self.exp['processes'] = [self.service()]
        subject = self.subject()
        cases = [('process', 'absent'), ('stream', 'stdin'), ('prefix', 'fresh\n'),
                 ('prefix', 'fresh\r'), ('prefix', 'fresh\0'), ('capture', 'Bad')]
        for key, value in cases:
            with self.subTest(key=key, value=value):
                awaited = self.awaited()
                awaited[key] = value
                self.exp['actions'] = [dict(name='fresh', argv=['/bin/true'], env={}, **{'await': awaited})]
                result = self.validate(subject)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn(b'await', result.stderr.lower(), result.stderr)
        for key in self.awaited():
            with self.subTest(missing=key):
                awaited = self.awaited()
                del awaited[key]
                self.exp['actions'][0]['await'] = awaited
                result = self.validate(subject)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn(b'await', result.stderr.lower(), result.stderr)

    def test_action_capture_is_unavailable_before_declaration(self):
        self.exp['processes'] = [self.service()]
        self.exp['actions'] = [dict(name='early', argv=['/bin/true'], env={}, stdin='{action.fresh.word}'),
                               dict(name='fresh', argv=['/bin/true'], env={}, **{'await': self.awaited()})]
        result = self.validate(self.subject())
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn(b'action.fresh.word', result.stderr, result.stderr)

    def test_collection_and_check_cannot_declare_await(self):
        self.exp['processes'] = [self.service()]
        subject = self.subject()
        for category in ('collect', 'checks'):
            with self.subTest(category=category):
                self.exp['collect'], self.exp['checks'] = [], []
                command = dict(name='forbidden', argv=['/bin/true'], env={}, **{'await': self.awaited()})
                if category == 'collect':
                    command['scan'] = []
                self.exp[category] = [command]
                result = self.validate(subject)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn(b'await', result.stderr.lower(), result.stderr)

    def test_process_cannot_use_action_capture(self):
        service = self.service()
        service['env'] = {'INVENTED': '{action.fresh.word}'}
        self.exp['processes'] = [service]
        self.exp['actions'] = [dict(name='fresh', argv=['/bin/true'], env={},
                                    **{'await': self.awaited()})]
        result = self.validate(self.subject())
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn(b'action.fresh.word', result.stderr, result.stderr)

    def test_action_capture_cannot_be_executable(self):
        self.exp['processes'] = [self.service()]
        self.exp['actions'] = [dict(name='fresh', argv=['/bin/true'], env={},
                                    **{'await': self.awaited()}),
                               dict(name='execute', argv=['{action.fresh.word}'], env={})]
        result = self.validate(self.subject())
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertRegex(result.stderr.decode(), r'(?i)(executable|argv|program)')
