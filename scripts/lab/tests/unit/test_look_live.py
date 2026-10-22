"""Live scenes, one-call connections, and giving the hand its viewport back."""
import contextlib
import io
from pathlib import Path
import socket
import tempfile
import threading
import unittest
from unittest.mock import Mock, patch
from look import canon, capture, png, record, scene, style
from runner import canonical
from .test_look_scene import valid

FOCUS = dict(role='textbox', name='Message')
REFUSED = dict(refused='protocol', detail='read failed')
DIGEST = 'sha256:' + 'a' * 64


def live_scene():
    value = valid()
    value['page'] = dict(live='/chat', ready=value['page']['ready'],
                         focus=dict(**FOCUS, match='exact'))
    value['mockup'] = value['regions'][0]['mockup'] = None
    value['widths'] = [dict(name='small', width=4, height=3, mobile=False),
                       dict(name='phone', width=3, height=4, mobile=True)]
    return value


class Scenes(unittest.TestCase):
    def test_live_scene_and_null_focus(self):
        for focus in (None, dict(**FOCUS, match='words')):
            value = live_scene()
            value['page']['focus'] = focus
            self.assertEqual(scene.read(canonical.record_bytes(value)), value)

    def test_each_live_member_refusal(self):
        cases = [('live', v, 'page.live') for v in (None, 3, '', 'chat', '//chat', '/a b', '/é', '/a\n', '/a\x7f', '/chat?x=1')]
        cases += [('path', '/chat', 'page.path'), ('unknown', True, 'page.unknown')]
        cases += [('focus', v, 'page.focus') for v in (False, [], 'textbox')]
        cases += [('focus', v, 'page.focus.' + k) for v, k in (
            ({'selector': 'input'}, 'selector'), ({}, 'role'),
            (dict(**FOCUS, match='other'), 'match'),
            (dict(role='', name='x', match='exact'), 'role'),
            (dict(role='textbox', name=1, match='words'), 'name'),
            (dict(**FOCUS, match='exact', selector='input'), 'selector'))]
        cases += [('ready', {'selector': 'main'}, 'page.ready.selector')]
        for key, replacement, member in cases:
            with self.subTest(key=key, value=replacement):
                value = live_scene()
                value['page'][key] = replacement
                self.assertTrue(any(r.member.startswith(member) for r in scene.read(canonical.record_bytes(value))))
        for key in ('live', 'ready', 'focus'):
            value = live_scene()
            del value['page'][key]
            self.assertIn('page.' + ('path' if key == 'live' else key),
                          {r.member for r in scene.read(canonical.record_bytes(value))})
        value = valid()
        value['page']['focus'] = None
        self.assertIn('page.focus', {r.member for r in scene.read(canonical.record_bytes(value))})


class Live(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.out, self.live_out = self.root / 'out', self.root / 'hand'
        self.out.mkdir()
        self.live_out.mkdir()
        self.value = live_scene()
        self.calls = []
        self.overrides = {}
        self.trouble = 0

    def answer(self, call):
        self.calls.append(call)
        name = call['call']
        if name in self.overrides:
            return self.overrides[name]
        if name == 'look-viewport':
            return dict(viewport={k: call.get(k, v) for k, v in dict(width=8, height=6, mobile=False).items()})
        if name == 'address':
            return dict(address='http://127.0.0.1:9/chat?state=kept')
        if name == 'look-await':
            return dict(met=[])
        if name == 'look-tree':
            return dict(tree=[dict(node=1, depth=0, role='main', name='', level=0)])
        if name == 'look-find':
            return dict(found=[dict(node=1, role='main', name='', box=None)], more=0)
        if name == 'look-style':
            return dict(style=dict(box=dict(x=0, y=0, width=2, height=1),
                                   values={p: 'normal' for p in style.PROPERTIES}, faces=[]))
        if name == 'look-shot':
            data = png.write(png.Image(2, 1, [bytes([0, 0, 0]) * 2]))
            path = 'look/' + call['file']
            target = self.live_out / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
            entry = record.file(path, data)
            return dict(shot=dict(file=path, bytes=entry['bytes'], sha256=entry['sha256'][7:], width=2, height=1))
        if name == 'look-focus':
            return dict(focus=FOCUS.copy())
        if name == 'trouble':
            self.trouble += 1
            return dict(trouble=[dict(kind='console', text=str(self.trouble), address='http://127.0.0.1:9/chat')],
                        more=1 if self.trouble == 1 else 0)
        raise AssertionError(name)

    @contextlib.contextmanager
    def listener(self, reply=None):
        errors = []
        with socket.socket() as listener:
            listener.bind(('127.0.0.1', 0))
            listener.listen()
            endpoint = listener.getsockname()
            def serve():
                try:
                    while True:
                        with listener.accept()[0] as peer:
                            peer.settimeout(2)
                            data = bytearray()
                            while b'\n' not in data:
                                chunk = peer.recv(65536)
                                if not chunk:
                                    break
                                data.extend(chunk)
                            if not data:
                                return
                            call = canon.loads(bytes(data).removesuffix(b'\n'), None)
                            answer = self.answer(call) if reply is None else reply
                            if answer == b'':
                                continue
                            peer.sendall(answer if type(answer) is bytes else canonical.dumps(answer) + b'\n')
                            self.assertEqual(peer.recv(1), b'', 'one connection carried several calls')
                except Exception as error:
                    errors.append(error)
            thread = threading.Thread(target=serve)
            thread.start()
            try:
                yield capture.Hand(endpoint, self.live_out, 2)
            finally:
                with socket.create_connection(endpoint, timeout=4):
                    pass
                thread.join(timeout=6)
                self.assertFalse(thread.is_alive())
                self.assertEqual(errors, [])

    def engine(self, hand):
        return capture.Capture(Mock(), self.out, 1, {}, hand)

    def test_calls_files_focus_trouble_and_restore_after_two_widths(self):
        with self.listener() as hand:
            value = self.engine(hand).scene(self.value, DIGEST)
        record.build('Chrome', [value])
        names = [c['call'] for c in self.calls]
        one = ['look-viewport', 'address', 'look-await', 'look-tree', 'look-find', 'look-style',
               'look-shot', 'look-focus', 'look-shot', 'trouble']
        self.assertEqual(names, one + ['trouble'] + one + ['look-viewport'])
        self.assertEqual(self.calls[-1], {'call': 'look-viewport'})
        self.assertEqual(self.calls[0], dict(call='look-viewport', width=4, height=3, mobile=False))
        self.assertEqual(self.calls[2]['seconds'], 1)
        self.assertEqual(self.calls[5]['properties'], style.PROPERTIES)
        self.assertEqual(self.calls[6]['scale'], 2)
        self.assertEqual(self.calls[8]['scale'], 1)
        for width in value['widths']:
            self.assertEqual(width['page']['focus'], FOCUS)
            self.assertEqual(width['page']['ready'], 'met')
            self.assertEqual(width['page']['address'], 'http://127.0.0.1:9/chat?state=kept')
            path = width['page']['shot']['path']
            self.assertEqual((self.out / path).read_bytes(), (self.live_out / 'look' / path).read_bytes())
        self.assertEqual(value['widths'][0]['page']['trouble'],
                         [dict(kind='console', text='1'), dict(kind='console', text='2')])
        self.assertEqual(value['widths'][1]['page']['trouble'], [dict(kind='console', text='3')])

    def test_address_path_and_await_must_both_match(self):
        for address, awaited, expected in (
                ('/chat', dict(met=[]), 'met'), ('/chat/other', dict(met=[]), 'missed'),
                ('/chat', dict(missed=[]), 'missed')):
            with self.subTest(address=address, awaited=awaited):
                self.overrides.update(address=dict(address='http://127.0.0.1:9' + address), **{'look-await': awaited})
                hand = Mock(out=self.live_out)
                hand.call.side_effect = lambda call, **kw: self.answer(dict(call=call, **kw))
                result = self.engine(hand).side(self.value, self.value['widths'][0], 'page', address.strip('/').replace('/', '-') + expected)
                self.assertEqual(result[0]['ready'], expected)

    def test_shot_digest_mismatch_fails_and_restores(self):
        original = self.answer
        def wrong(call):
            answer = original(call)
            if call['call'] == 'look-shot':
                answer['shot']['sha256'] = '0' * 64
            return answer
        path = self.root / 'scene.json'
        path.write_bytes(canonical.record_bytes(self.value))
        scratch = self.root / 'scratch'
        scratch.mkdir()
        out = self.root / 'failed'
        program = Mock()
        program.answer.return_value = dict(ready=dict(browser='Chrome'))
        with (self.listener() as hand, patch.object(self, 'answer', side_effect=wrong),
              patch('look.capture.Program', return_value=program),
              patch('look.capture.tempfile.mkdtemp', return_value=str(scratch)),
              contextlib.redirect_stderr(io.StringIO()) as stderr):
            endpoint = f'{hand.endpoint[0]}:{hand.endpoint[1]}'
            self.assertEqual(capture.main([
                '--browser', '/bin/true', '--chromium', '/bin/true', '--fonts', str(self.root),
                '--live', endpoint, '--live-out', str(self.live_out), '--ready-seconds', '1',
                '--out', str(out), str(path)]), 1)
        self.assertIn('metadata differs', stderr.getvalue())
        self.assertEqual(self.calls[-1], dict(call='look-viewport'))
        self.assertFalse((out / 'capture.json').exists())
        self.assertFalse(scratch.exists())
        program.dispose.assert_called_once()

    def test_region_refusal_is_recorded(self):
        self.overrides['look-find'] = REFUSED
        with self.listener() as hand:
            value = self.engine(hand).scene(self.value, DIGEST)
        self.assertEqual(value['widths'][0]['regions'][0]['page'], REFUSED)
        self.assertNotIn('look-style', [c['call'] for c in self.calls])

    def test_critical_refusals_fail_and_restore(self):
        for call in ('look-viewport', 'address', 'trouble'):
            with self.subTest(call=call):
                self.calls.clear()
                self.overrides = {call: REFUSED}
                with self.listener() as hand, self.assertRaises(capture.ProgramFailure):
                    self.engine(hand).scene(self.value, DIGEST)
                self.assertEqual(self.calls[-1], dict(call='look-viewport'))

    def test_closed_unreadable_unreachable_and_deadline_fail(self):
        for reply in (b'', b'garbage\n', b'{"other":true}\n'):
            with self.subTest(reply=reply), self.listener(reply) as hand:
                with self.assertRaises(capture.ProgramFailure):
                    hand.call('address')
        with socket.socket() as bound:
            bound.bind(('127.0.0.1', 0))
            with self.assertRaises(capture.ProgramFailure):
                capture.Hand(bound.getsockname(), self.live_out, 1).call('address')
        with patch('look.capture.time.monotonic', side_effect=[0, 3]):
            with self.assertRaisesRegex(capture.ProgramFailure, 'deadline'):
                capture.Hand(('127.0.0.1', 1), self.live_out, 2).call('address')

    def test_unreadable_critical_answers_fail(self):
        cases = [('look-viewport', dict(viewport=None)),
                 ('look-viewport', dict(viewport=dict(width=True, height=1, mobile=False))),
                 ('address', dict(address=3)), ('look-focus', dict(focus=[])),
                 ('trouble', dict(trouble=[], more=-1)),
                 ('trouble', dict(trouble=[dict(kind='console', text='missing address')], more=0))]
        for call, answer in cases:
            with self.subTest(call=call, answer=answer), self.listener(answer) as hand:
                with self.assertRaises(capture.ProgramFailure):
                    hand.call(call)

    def test_scene_kind_and_live_arguments_refused_before_start(self):
        path = self.root / 'scene.json'
        base = ['--browser', '/bin/true', '--chromium', '/bin/true', '--fonts', str(self.root),
                '--mockups', str(self.root), '--ready-seconds', '1', '--out', str(self.root / 'new')]
        cases = [(live_scene(), ['--page', str(self.root)]),
                 (valid(), ['--live', '127.0.0.1:9', '--live-out', str(self.root)]),
                 (live_scene(), ['--live', 'localhost:9', '--live-out', str(self.root)]),
                 (live_scene(), ['--live', '127.0.0.1:9']),
                 (live_scene(), ['--live', '127.0.0.1:9', '--live-out', 'relative']),
                 (valid(), ['--page', str(self.root), '--live-out', str(self.root)])]
        for value, args in cases:
            with self.subTest(args=args):
                path.write_bytes(canonical.record_bytes(value))
                with patch('look.capture.Program') as program, contextlib.redirect_stderr(io.StringIO()):
                    self.assertEqual(capture.main([*base, *args, str(path)]), 2)
                program.assert_not_called()
                self.assertFalse((self.root / 'new').exists())
