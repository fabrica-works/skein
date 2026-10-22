"""Command contracts from lab-browser.md 8.2 and 8.8; production stays opaque."""
import json
import os
import socket
import subprocess
import unittest

from tests.qa import LAB


def canonical(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(',', ':')).encode() + b'\n'


class HandCommands(unittest.TestCase):
    def setUp(self):
        os.umask(0o077)

    def exchange(self, operands, reply, *, stdin=b'', command='browse'):
        with socket.socket() as listener:
            listener.bind(('127.0.0.1', 0))
            listener.listen()
            listener.settimeout(5)
            address = '%s:%s' % listener.getsockname()
            process = subprocess.Popen([str(LAB / 'hand' / command), address, *operands],
                                       stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                       stderr=subprocess.PIPE)
            self.addCleanup(self.reap, process)
            process.stdin.write(stdin)
            process.stdin.close()
            process.stdin = None
            try:
                connection = listener.accept()[0]
            except TimeoutError:
                stdout, stderr = process.communicate(timeout=5)
                self.fail(f'8.8 expected a call; status={process.returncode}, stdout={stdout!r}, stderr={stderr!r}')
            with connection:
                connection.settimeout(5)
                raw = connection.makefile('rb').readline(128 * 1024 + 1)
                call = json.loads(raw)
                self.assertEqual(raw, canonical(call), '8.2 canonical request')
                connection.sendall(reply)
            stdout, stderr = process.communicate(timeout=5)
            return call, process.returncode, stdout, stderr

    @staticmethod
    def reap(process):
        if process.poll() is None:
            process.kill()
        process.communicate()

    @staticmethod
    def reply_for(call):
        part = dict(role='text field', name='Title', value='', states=[])
        return {
            'outline': dict(outline='heading 1 "Fixture"\n', left=0),
            'press': dict(pressed=part), 'type': dict(typed=part),
            'choose': dict(chose=part), 'go': dict(went='http://127.0.0.1/'),
            'history': dict(went='http://127.0.0.1/'),
            'key': dict(keyed='Enter'), 'wait': dict(gone=True),
            'address': dict(address='about:blank'),
            'shot': dict(shot=dict(file='shots/0001.png', bytes=8,
                                   sha256='0' * 64, width=320, height=240)),
            'trouble': dict(trouble=[], more=0),
        }[call]

    def test_each_verb_sends_its_call(self):
        part = dict(in_=[], match='words', name='Title', role='text field')
        part['in'] = part.pop('in_')
        cases = [
            (['outline'], dict(call='outline', **{'in': []})),
            (['press', 'text field', 'Title'], dict(call='press', **part)),
            (['type', 'text field', 'Title'], dict(call='type', text='line one\nline two\n', **part)),
            (['choose', 'text field', 'Title', 'Option'], dict(call='choose', option=dict(name='Option', match='words'), **part)),
            (['go', '/page?q=1'], dict(call='go', path='/page?q=1')),
            (['address'], dict(call='address')),
            (['shot'], dict(call='shot')),
            (['trouble'], dict(call='trouble')),
            (['wait', 'text field', 'Title', '--gone', '--timeout', '3'], dict(call='wait', gone=True, seconds=3, **part)),
        ]
        cases += [([verb], dict(call='history', to=verb)) for verb in ['reload', 'back', 'forward']]
        cases += [(['key', key], dict(call='key', key=key.removeprefix('Shift+'), shift=key.startswith('Shift+')))
                  for key in ['Enter', 'Shift+Enter', 'Escape', 'Tab', 'Shift+Tab']]
        for argv, expected in cases:
            with self.subTest(argv=argv):
                reply = self.reply_for(expected['call'])
                if expected['call'] == 'key':
                    reply = dict(keyed=argv[1])
                call, status, stdout, stderr = self.exchange(argv, canonical(reply),
                                                           stdin=b'line one\nline two\n\n')
                self.assertEqual(call, expected)
                self.assertEqual(status, 0, stderr)
                self.assertEqual(stderr, b'')

    def test_scopes_and_exact_option_match(self):
        argv = ['choose', 'select', 'Theme', 'Dark', '--in', 'main', 'Workspace',
                '--in', 'region', 'Settings', '--exact']
        call, status, _, stderr = self.exchange(argv, canonical(self.reply_for('choose')))
        self.assertEqual(call, dict(call='choose', role='select', name='Theme', match='exact',
                                    option=dict(name='Dark', match='exact'),
                                    **{'in': [dict(role='main', name='Workspace', match='words'),
                                              dict(role='region', name='Settings', match='words')]}))
        self.assertEqual(status, 0, stderr)

    def test_answer_statuses_and_reader_text(self):
        cases = [
            (['press', 'button', 'Send'], dict(pressed=dict(role='button', name='Send', value='', states=[])), 0, b'pressed button "Send"'),
            (['outline'], dict(refused='covered', detail='covered fixture'), 1, b'refused covered: covered fixture'),
            (['wait', 'button', 'Send', '--timeout', '1'], dict(missed=[]), 1, b''),
            (['outline'], dict(outline='heading 1 "Fixture"\n', left=0), 0, b'heading 1 "Fixture"'),
        ]
        for argv, reply, expected_status, fragment in cases:
            with self.subTest(reply=reply):
                _, status, stdout, stderr = self.exchange(argv, canonical(reply))
                self.assertEqual(status, expected_status, stderr)
                self.assertIn(fragment, stdout)
                self.assertEqual(stderr, b'')

    def test_unreadable_answers_return_three(self):
        for reply in [b'', b'not json\n', b'{}\n', b'{"address":false}\n', b'{"address":"unfinished"}']:
            with self.subTest(reply=reply):
                _, status, _, _ = self.exchange(['address'], reply)
                self.assertEqual(status, 3, '8.8 unreadable answer')

    def test_usage_and_cross_origin_operands_never_connect(self):
        with socket.socket() as listener:
            listener.bind(('127.0.0.1', 0))
            listener.listen()
            listener.setblocking(False)
            address = '%s:%s' % listener.getsockname()
            cases = [[], ['unknown'], ['press', 'button'], ['key', 'Delete'],
                     ['go', '//127.0.0.1/'], ['go', 'http://127.0.0.1/'], ['go', '/has space'],
                     ['wait', 'button', 'Save', '--timeout', '0'],
                     ['wait', 'button', 'Save', '--timeout', '301'], ['address', 'extra']]
            for argv in cases:
                with self.subTest(argv=argv):
                    result = subprocess.run([str(LAB / 'hand/browse'), address, *argv],
                                            input=b'', capture_output=True, timeout=5, check=False)
                    self.assertEqual(result.returncode, 2, result.stderr)
                    self.assertTrue(result.stderr)
                    with self.assertRaises(BlockingIOError):
                        listener.accept()

    def test_unreachable_returns_three(self):
        with socket.socket() as endpoint:
            endpoint.bind(('127.0.0.1', 0))
            address = '%s:%s' % endpoint.getsockname()
            result = subprocess.run([str(LAB / 'hand/browse'), address, 'address'],
                                    input=b'', capture_output=True, timeout=5, check=False)
            self.assertEqual(result.returncode, 3, result.stderr)

    def test_sign_in_more_than_one_line_is_a_usage_error_and_never_connects(self):
        # 8.8: its standard input is one line, the link; more is a usage error, and nothing is sent.
        link = 'http://127.0.0.1:12345/sign-in?invented-credential-0001'
        with socket.socket() as listener:
            listener.bind(('127.0.0.1', 0))
            listener.listen()
            listener.setblocking(False)
            address = '%s:%s' % listener.getsockname()
            run = subprocess.run([str(LAB / 'hand' / 'sign-in'), address],
                                 input=(link + '\nunused second line\n').encode(), capture_output=True, timeout=5)
            self.assertEqual(run.returncode, 2, run.stderr)
            self.assertEqual(run.stdout, b'')
            self.assertNotIn(b'invented-credential', run.stdout + run.stderr)
            with self.assertRaises(BlockingIOError):
                listener.accept()

    def test_sign_in_one_link_is_redacted(self):
        link = 'http://127.0.0.1:12345/sign-in?invented-credential-0001'
        call, status, stdout, stderr = self.exchange([], canonical(dict(opened='http://127.0.0.1:12345/sign-in?<link>')),
                                                    stdin=(link + '\n').encode(), command='sign-in')
        self.assertEqual(call, dict(call='sign-in', link=link))
        self.assertEqual(status, 0, stderr)
        self.assertNotIn(b'invented-credential', stdout + stderr)
