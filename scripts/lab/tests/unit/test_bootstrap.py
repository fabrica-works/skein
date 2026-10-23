"""Private framing vectors expose no input bytes in diagnostics."""
from dataclasses import FrozenInstanceError
import unittest
from runner import bootstrap


class Framing(unittest.TestCase):
    def assert_private(self, state, event):
        self.assertTrue('canary' not in repr(state), 'parser representation contains no input')
        self.assertTrue(event is None or event in ('received', 'partial', 'malformed', 'long', 'extra'),
                        'bootstrap event contains only a fixed framing fact')
        self.assertLessEqual(len(state.line), 4096)

    def test_every_split_of_utf8_and_maximum_line(self):
        for line in ('private-canary-\u03bb\U0001f642\n'.encode(), b'private-canary-' + b'x' * (4096 - 16) + b'\n'):
            self.assertTrue(len(line) <= 4096)
            for split in range(len(line) + 1):
                state = bootstrap.State()
                for chunk in (line[:split], line[split:]):
                    if chunk:
                        state, event = bootstrap.feed(state, chunk)
                        self.assert_private(state, event)
                self.assertEqual(state.status, 'received')
                state, payload = bootstrap.take(state)
                self.assertTrue(payload == line, 'recipient receives the exact framed line')
                self.assert_private(state, None)
                self.assertTrue(bootstrap.take(state) == (state, None), 'input is available at most once')
                self.assertTrue(bootstrap.feed(state, b'') == (state, None), 'EOF emits no duplicate event')
                state, event = bootstrap.feed(state, b'after')
                self.assertTrue(event == 'extra', 'extra private output is refused')
                self.assert_private(state, event)

    def test_single_byte_chunks_include_multibyte_utf8(self):
        state = bootstrap.State()
        for value in 'private-canary-\u03bb\U0001f642\n'.encode():
            state, event = bootstrap.feed(state, bytes([value]))
            self.assert_private(state, event)
        self.assertTrue(event == 'received', 'one complete private line is received')

    def test_all_failures_drop_input_and_invalid_state_drains_once(self):
        cases = [(b'\n', 'malformed'), (b'private-canary\r\n', 'malformed'),
                 (b'private-canary\0\n', 'malformed'), (b'private-canary\xff\n', 'malformed'),
                 (b'private-canary\xc0\x80\n', 'malformed'), (b'private-canary\xed\xa0\x80\n', 'malformed'),
                 (b'x' * 4096 + b'\n', 'long'), (b'x' * 4097, 'long'),
                 (b'private-canary\nextra', 'extra'), (b'private-canary\n\n', 'extra')]
        for data, expected in cases:
            state, event = bootstrap.feed(bootstrap.State(), data)
            self.assertTrue(event == expected, 'private framing failure has the expected fixed reason')
            self.assert_private(state, event)
            self.assertTrue(not state.line, 'invalid parser state retains no input')
            for chunk in (b'private-canary\n', b'\xff', b''):
                state, event = bootstrap.feed(state, chunk)
                self.assertTrue(event is None, 'draining emits no duplicate event')
                self.assert_private(state, event)
                self.assertTrue(bootstrap.take(state) == (state, None), 'input is available at most once')

    def test_4096_bytes_without_lf_then_4097th_byte_is_long(self):
        state, event = bootstrap.feed(bootstrap.State(), b'x' * 4096)
        self.assertTrue(event is None, 'draining emits no duplicate event')
        self.assertEqual(len(state.line), 4096)
        state, event = bootstrap.feed(state, b'\n')
        self.assertTrue(event == 'long', 'overlong private input is refused')
        self.assertTrue(not state.line, 'parser retains no input')

    def test_eof_before_during_and_after_a_line(self):
        state = bootstrap.State()
        self.assertTrue(bootstrap.feed(state, b'') == (state, None), 'EOF emits no duplicate event')
        state, _ = bootstrap.feed(state, b'private-canary')
        self.assertTrue(bootstrap.take(state) == (state, None), 'input is available at most once')
        state, event = bootstrap.feed(state, b'')
        self.assertTrue(event == 'partial', 'EOF refuses a partial private line')
        self.assert_private(state, event)
        state, _ = bootstrap.feed(bootstrap.State(), b'private-canary\n')
        self.assertTrue(bootstrap.feed(state, b'') == (state, None), 'EOF emits no duplicate event')
        self.assertTrue(bootstrap.take(state)[1] is not None, 'EOF preserves one-time delivery')

    def test_extra_before_take_and_after_take_or_drop(self):
        for operation in ('waiting', 'take', 'drop'):
            state, _ = bootstrap.feed(bootstrap.State(), b'private-canary\n')
            if operation == 'take':
                state, _ = bootstrap.take(state)
            if operation == 'drop':
                state = bootstrap.drop(state)
            state, event = bootstrap.feed(state, b'more')
            self.assertTrue(event == 'extra', 'extra private output is refused')
            self.assertTrue(not state.line, 'parser retains no input')
            self.assertTrue(bootstrap.take(state) == (state, None), 'input is available at most once')

    def test_drop_partial_drains_and_preserves_terminator_without_receipt(self):
        state, _ = bootstrap.feed(bootstrap.State(), b'private-canary')
        state = bootstrap.drop(state)
        self.assertTrue(not state.line, 'parser retains no input')
        state, event = bootstrap.feed(state, b' remainder')
        self.assertIsNone(event)
        self.assertTrue(not state.line, 'parser retains no input')
        state, event = bootstrap.feed(state, b'\n')
        self.assertIsNone(event)
        self.assertTrue(bootstrap.take(state) == (state, None), 'input is available at most once')
        state, event = bootstrap.feed(state, b'more')
        self.assertTrue(event == 'extra', 'extra private output is refused')
        state = bootstrap.drop(bootstrap.State())
        state, event = bootstrap.feed(state, b'private-canary\nmore')
        self.assertTrue(event == 'extra', 'extra private output is refused')
        self.assertTrue(not state.line, 'parser retains no input')

    def test_state_is_frozen_and_representation_never_contains_input(self):
        state, event = bootstrap.feed(bootstrap.State(), b'private-canary')
        self.assert_private(state, event)
        with self.assertRaises(FrozenInstanceError):
            state.status = 'taken'
        state = bootstrap.drop(state)
        self.assert_private(state, None)
