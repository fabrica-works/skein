"""The hand's commands against values and a listener held by the test."""
import io
import socket
import threading
import unittest
from unittest.mock import Mock, patch
from hand import command, words
from runner import canonical

UNREACHABLE = 'the browser is unreachable, or its answer unreadable\n'

PART = dict(role='button', name='Send', value='', states=['disabled'])
LINK = 'http://127.0.0.1:9/sign-in?secret=0123456789abcdef'


class Words(unittest.TestCase):
    def test_calls(self):
        finder = dict(role='button', name='Send', match='words', **{'in': []})
        cases = [(['outline'], {'call': 'outline', 'in': []}),
                 (['press', 'button', 'Send'], dict(call='press', **finder)),
                 (['type', 'button', 'Send'], dict(call='type', **finder)),
                 (['choose', 'button', 'Send', 'One'], dict(call='choose', option=dict(name='One', match='words'), **finder)),
                 (['go', '/a?b=c'], dict(call='go', path='/a?b=c')),
                 (['wait', 'button', 'Send', '--timeout', '3', '--gone'], dict(call='wait', gone=True, seconds=3, **finder))]
        for key in ['Enter', 'Shift+Enter', 'Escape', 'Tab', 'Shift+Tab']:
            cases.append((['key', key], dict(call='key', key=key.removeprefix('Shift+'), shift=key.startswith('Shift+'))))
        for verb in ['reload', 'back', 'forward']:
            cases.append(([verb], dict(call='history', to=verb)))
        for verb in ['address', 'shot', 'trouble']:
            cases.append(([verb], {'call': verb}))
        for argv, expected in cases:
            with self.subTest(argv=argv):
                self.assertEqual(words.call(argv), expected)

    def test_scopes_and_exact(self):
        self.assertEqual(words.call(['choose', 'select', 'Size', 'Large', '--in', 'main', '',
                                     '--in', 'region', 'Draft', '--exact']),
                         {'call': 'choose', 'role': 'select', 'name': 'Size', 'match': 'exact',
                          'option': {'name': 'Large', 'match': 'exact'}, 'in': [
                              {'role': 'main', 'name': '', 'match': 'words'},
                              {'role': 'region', 'name': 'Draft', 'match': 'words'}]})

    def test_usage(self):
        cases = [[], ['unknown'], ['press', 'button'], ['outline', 'extra'], ['key', 'Space'],
                 ['outline', '--wat'], ['outline', '--in', 'main'], ['press', 'button', 'X', '--timeout', '1'],
                 ['wait', 'button', 'X'], ['address', '--gone'], ['go', '/', '--exact'],
                 ['shot', '--in', 'main', ''], ['press', 'button', 'X', '--exact', '--exact'],
                 ['wait', 'button', 'X', '--gone', '--gone', '--timeout', '1']]
        cases += [['wait', 'button', 'X', '--timeout', v] for v in ['0', '301', 'bad', '']]
        cases += [['go', path] for path in ['//evil/', 'http://x/', '/a b', '/é', 'x']]
        for argv in cases:
            with self.subTest(argv=argv), self.assertRaises(words.Usage):
                words.call(argv)
        for endpoint in ['localhost:80', '127.0.0.2:1', '::1:1', '127.0.0.1:0', '[::1]:65536', '127.0.0.1:1/x']:
            with self.subTest(endpoint=endpoint), self.assertRaises(words.Usage):
                words.address(endpoint)
        self.assertEqual(words.address('[::1]:65535'), ('::1', 65535))
        self.assertEqual(words.address('127.0.0.1:1'), ('127.0.0.1', 1))

    def test_answers(self):
        cases = []
        for verb, key in [('press', 'pressed'), ('type', 'typed'), ('choose', 'chose'), ('wait', 'met')]:
            cases.append(({'call': verb}, {key: PART}, [key + ' button "Send" [disabled]']))
        for verb, key in [('go', 'went'), ('history', 'went'), ('sign-in', 'opened'), ('address', 'address')]:
            cases.append(({'call': verb}, {key: 'http://page/'}, [('' if verb == 'address' else key + ' ') + 'http://page/']))
        cases += [({'call': 'key', 'key': 'Enter', 'shift': True}, {'keyed': 'Shift+Enter'}, ['keyed Shift+Enter']),
                  ({'call': 'wait'}, {'gone': True}, ['gone']),
                  ({'call': 'outline'}, {'outline': 'main\n  button "X"\n', 'left': 2}, ['main', '  button "X"', '(2 entries cut)']),
                  ({'call': 'outline'}, {'outline': '', 'left': 0}, []),
                  ({'call': 'outline'}, {'outline': 'text \u2028 value\n\n', 'left': 0}, ['text \u2028 value', '']),
                  ({'call': 'shot'}, {'shot': dict(file='shots/0001.png', width=12, height=4, bytes=8, sha256='a'*64)}, ['shot shots/0001.png 12x4 8 bytes sha256 ' + 'a'*64]),
                  ({'call': 'trouble'}, {'trouble': [], 'more': 0}, ['no trouble']),
                  ({'call': 'trouble'}, {'trouble': [dict(kind='console', address='http://page/', text='bad\n"x"')], 'more': 2}, ['console http://page/ "bad\\n\\"x\\""', '(2 more)'])]
        for call, answer, lines in cases:
            with self.subTest(answer=answer):
                self.assertEqual(words.text(call, answer), (0, lines))
        for missed, lines in [([], ['missed: nothing of its role']),
                             ([PART], ['missed', '  button "Send" [disabled]'])]:
            self.assertEqual(words.text({'call': 'wait'}, {'missed': missed}), (1, lines))
        self.assertEqual(words.text({'call': 'press'}, {'refused': 'covered', 'detail': 'try later'}), (1, ['refused covered: try later']))
        self.assertEqual(words.part(dict(role='text field', name='', value='a\nb', states=['focused', 'selected'])), 'text field = "a\\nb" [focused] [selected]')

    def test_unreadable_shapes(self):
        for call, answer in [({'call': 'press'}, {'typed': PART}),
                             ({'call': 'wait'}, {'gone': False}),
                             ({'call': 'wait'}, {'missed': [PART]*9}),
                             ({'call': 'outline'}, {'outline': '', 'left': True}),
                             ({'call': 'trouble'}, {'trouble': [], 'more': -1}),
                             ({'call': 'key', 'key': 'Enter', 'shift': False}, {'keyed': 'Tab'}),
                             ({'call': 'press'}, {'pressed': dict(PART, states=['other'])}),
                             ({'call': 'address'}, {'address': 'x', 'extra': 1})]:
            with self.subTest(answer=answer), self.assertRaises(words.Unreadable):
                words.text(call, answer)


class Command(unittest.TestCase):
    def invoke(self, argv, value='', **kwargs):
        out, err = io.StringIO(), io.StringIO()
        status = command.main(argv, io.StringIO(value), out, err, **kwargs)
        return status, out.getvalue(), err.getvalue()

    def listen(self, args, reply, value='', **kwargs):
        received, errors = [], []
        with socket.socket() as listener:
            listener.bind(('127.0.0.1', 0))
            listener.listen()
            listener.settimeout(1)
            def serve():
                try:
                    with listener.accept()[0] as peer:
                        peer.settimeout(1)
                        data = bytearray()
                        while not data.endswith(b'\n'):
                            chunk = peer.recv(65536)
                            if not chunk:
                                break
                            data.extend(chunk)
                        received.append(bytes(data))
                        if reply:
                            peer.sendall(reply)
                except Exception as error:
                    errors.append(error)
            thread = threading.Thread(target=serve)
            thread.start()
            try:
                result = self.invoke(['127.0.0.1:' + str(listener.getsockname()[1])] + args, value, **kwargs)
            finally:
                thread.join(2)
            self.assertFalse(thread.is_alive())
            self.assertEqual(errors, [])
        return result, received

    def test_wire_and_statuses(self):
        cases = [(b'{"address":"http://page/"}\n', (0, 'http://page/\n', '')),
                 (b'{"detail":"blocked","refused":"origin"}\n', (1, 'refused origin: blocked\n', '')),
                 (b'{"address": "x"}\n', (3, '', UNREACHABLE)), (b'garbage\n', (3, '', UNREACHABLE)),
                 (b'{"address":"x","address":"y"}\n', (3, '', UNREACHABLE)),
                 (b'{"address":true}\n', (3, '', UNREACHABLE)), (b'{"address":"x"}', (3, '', UNREACHABLE)),
                 (b'', (3, '', UNREACHABLE))]
        for reply, expected in cases:
            with self.subTest(reply=reply):
                result, received = self.listen(['address'], reply)
                self.assertEqual(result, expected)
                self.assertEqual(received, [canonical.dumps({'call': 'address'}) + b'\n'])

    def test_type_input(self):
        result, received = self.listen(['type', 'text area', 'Message'], canonical.dumps({'typed': PART}) + b'\n', 'one\ntwo\n\n')
        self.assertEqual(result[0], 0)
        self.assertEqual(received, [canonical.dumps(dict(call='type', role='text area', name='Message', match='words', text='one\ntwo\n', **{'in': []})) + b'\n'])

    def test_closed_port(self):
        with socket.socket() as held:
            held.bind(('127.0.0.1', 0))
            port = held.getsockname()[1]
        self.assertEqual(self.invoke([f'127.0.0.1:{port}', 'address']), (3, '', UNREACHABLE))

    def test_invalid_path_or_address_never_connects(self):
        with socket.socket() as listener:
            listener.bind(('127.0.0.1', 0))
            listener.listen()
            endpoint = f'127.0.0.1:{listener.getsockname()[1]}'
            with patch('hand.command.socket.socket', side_effect=AssertionError('must not connect')):
                for path in ['//evil/', 'http://x/', '/a b', '/é', 'x']:
                    self.assertEqual(self.invoke([endpoint, 'go', path])[0], 2)
                self.assertEqual(self.invoke(['localhost:80', 'address'])[0], 2)
            listener.setblocking(False)
            with self.assertRaises(BlockingIOError):
                listener.accept()

    def test_sign_in_link_only_on_wire(self):
        for answer, status in [({'opened': 'http://127.0.0.1:9/sign-in?<link>'}, 0),
                               ({'refused': 'origin', 'detail': 'not on origin'}, 1)]:
            result, received = self.listen([], canonical.dumps(answer) + b'\n', LINK + '\n', sign_in=True)
            self.assertEqual(result[0], status)
            self.assertEqual(received, [canonical.dumps({'call': 'sign-in', 'link': LINK}) + b'\n'])
            for output in result[1:]:
                self.assertNotIn(LINK, output)
                self.assertNotIn('0123456789abcdef', output)
        with patch('hand.command.socket.socket', side_effect=AssertionError('must not connect')):
            for argv, value in [(['127.0.0.1:9'], LINK + '\nextra\n'),
                                (['127.0.0.1:9'], ''), ([LINK], LINK),
                                (['127.0.0.1:9', LINK], LINK)]:
                result = self.invoke(argv, value, sign_in=True)
                self.assertEqual(result[0], 2)
                self.assertNotIn(LINK, ''.join(result[1:]))
                self.assertNotIn('0123456789abcdef', ''.join(result[1:]))

    def test_deadline_and_size(self):
        with patch('hand.command.exchange', side_effect=TimeoutError):
            self.assertEqual(self.invoke(['127.0.0.1:9', 'address']), (3, '', UNREACHABLE))
        with patch.object(command, 'LIMIT', 32):
            result, _ = self.listen(['address'], b'x'*33)
            self.assertEqual(result, (3, '', UNREACHABLE))

    def test_read_deadline_is_total_and_wait_adds_thirty(self):
        for call, seconds in [({'call': 'address'}, 60),
                              ({'call': 'wait', 'seconds': 300}, 330)]:
            peer = Mock()
            peer.__enter__ = Mock(return_value=peer)
            peer.__exit__ = Mock(return_value=False)
            peer.recv.side_effect = [b'{', b'"address":"x"}\n']
            with patch('hand.command.socket.socket', return_value=peer), \
                    patch('hand.command.time.monotonic', side_effect=[0, 0, 0, seconds - 1, seconds + 1]):
                with self.assertRaises(words.Unreadable):
                    command.exchange(('127.0.0.1', 9), call)
            self.assertEqual(peer.recv.call_count, 1)
            self.assertEqual(peer.settimeout.call_args_list[0].args, (seconds,))

    def test_input_is_strict_utf8_and_preserves_carriage_returns(self):
        with patch('hand.command.exchange', return_value={'typed': PART}) as exchange:
            output = io.StringIO()
            source = io.TextIOWrapper(io.BytesIO(b'one\r\ntwo\n'), encoding='ascii')
            self.assertEqual(command.main(['127.0.0.1:9', 'type', 'text area', 'X'], source, output, io.StringIO()), 0)
            self.assertEqual(exchange.call_args.args[1]['text'], 'one\r\ntwo')
        for sign_in, args in [(False, ['type', 'text field', 'X']), (True, [])]:
            with patch('hand.command.socket.socket', side_effect=AssertionError('must not connect')):
                source = io.TextIOWrapper(io.BytesIO(b'\xff\n'), encoding='ascii')
                out, err = io.StringIO(), io.StringIO()
                self.assertEqual(command.main(['127.0.0.1:9'] + args, source, out, err, sign_in=sign_in), 2)
