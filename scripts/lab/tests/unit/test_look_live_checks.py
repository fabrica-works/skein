"""Focus and several capture records held to the check command's contract."""
import contextlib
import copy
import io
import json
from pathlib import Path
import tempfile
import unittest
from look import check, checks, record
from runner import canonical, checks as runner_checks
from .test_look_checks import fixture, tails


class Focus(unittest.TestCase):
    def test_focus_matches_on_both_sides_of_each_rule(self):
        cases = [('exact', 'Message', 'Message', True), ('exact', 'Message', 'New Message', False),
                 ('exact', '', 'Anything', True), ('words', '', 'Anything', True),
                 ('words', 'Message new', 'new draft Message', True),
                 ('words', 'message', 'Message', False), ('words', 'Message', 'Messages', False),
                 ('words', 'Message absent', 'Message', False),
                 ('words', 'one\ttwo', 'two\none', True)]
        for match, name, got, met in cases:
            with self.subTest(match=match, name=name, got=got):
                scene, width = fixture()
                scene['page'] = dict(live='/chat', ready=scene['page']['ready'],
                                      focus=dict(role='textbox', name=name, match=match))
                width['page']['focus'] = dict(role='textbox', name=got)
                result = checks.structure(scene, width, '', '')
                self.assertEqual(tails(result), [] if met else ['focus/page'])
                for focus in (None, dict(role='button', name=got)):
                    width['page']['focus'] = focus
                    self.assertEqual(tails(checks.structure(scene, width, '', '')), ['focus/page'])

    def test_null_focus_expectation_and_focus_departure(self):
        scene, width = fixture()
        scene['page'] = dict(live='/chat', ready=scene['page']['ready'], focus=None)
        width['page']['focus'] = dict(role='button', name='Send')
        self.assertEqual(checks.structure(scene, width, '', ''), checks.Result([], []))
        scene['page']['focus'] = dict(role='textbox', name='Message', match='exact')
        scene['departures'] = [dict(id='accepted', width=width['name'], signature='focus/page', cites='design 1')]
        self.assertEqual(checks.structure(scene, width, '', ''), checks.Result([], []))
        width['page']['focus'] = dict(role='textbox', name='Message')
        self.assertEqual(tails(checks.structure(scene, width, '', '')), ['departure-gone/accepted'])


class Captures(unittest.TestCase):
    def test_several_records_missing_duplicate_and_unreadable_keep_other_scenes_judged(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            paths, captures = [], []
            for name in ('first', 'second', 'missing', 'duplicate'):
                scene, width = fixture()
                scene['name'] = name
                scene['page'] = dict(live='/' + name, ready=scene['page']['ready'],
                                     focus=dict(role='textbox', name='Message', match='exact'))
                width['page']['focus'] = None if name == 'second' else dict(role='textbox', name='Message')
                scene_path = root / (name + '.json')
                data = canonical.record_bytes(scene)
                scene_path.write_bytes(data)
                paths.append(str(scene_path))
                captured = dict(name=name, scene=record.file('scene.json', data)['sha256'], widths=[width])
                captures.append(captured)
            dirs = [root / 'a', root / 'b']
            for dest in dirs:
                dest.mkdir()
                # The same relative filename differs by record: each outline must be read there.
                text = b'main\n' if dest == dirs[0] else b'banner\n'
                for side in ('mockup', 'page'):
                    (dest / (side + '.txt')).write_bytes(text)
                selected = [captures[0], captures[3]] if dest == dirs[0] else [captures[1], copy.deepcopy(captures[3])]
                for item in selected:
                    for side in ('mockup', 'page'):
                        item['widths'][0][side]['outline'] = record.file(side + '.txt', text)
                (dest / 'capture.json').write_bytes(canonical.record_bytes(record.build('Chrome', selected)))
            args = ['structure', '--capture', str(dirs[0]), '--capture', str(dirs[1])]
            def run(extra, selected):
                stdout = io.StringIO()
                with contextlib.redirect_stdout(stdout):
                    status = check.main([*args, *extra, *selected])
                data = stdout.getvalue().encode()
                self.assertEqual(runner_checks.command('look-structure', status, data)[2], [])
                return status, [json.loads(line) for line in data.splitlines()]
            status, lines = run([], paths)
            self.assertEqual(status, 1)
            self.assertEqual([line['fail'] for line in lines if 'fail' in line], ['second/phone/focus/page'])
            undecided = [line['undecided'] for line in lines if 'undecided' in line]
            self.assertEqual(len(undecided), 2)
            self.assertIn('missing', undecided[0])
            self.assertIn('duplicate', undecided[1])
            self.assertEqual(run([], paths[:1]), (0, []))
            self.assertEqual(run([], paths[2:])[0], 2)
            status, lines = run(['--capture', str(root / 'unreadable')], paths[:2])
            self.assertEqual(status, 1)
            self.assertTrue(any('unreadable' in line.get('undecided', '') for line in lines))
            self.assertTrue(any(line.get('fail') == 'second/phone/focus/page' for line in lines))
            for kind in ('style', 'structure'):
                args[0] = kind
                status, lines = run(['--capture', str(dirs[0])], paths[:1])
                self.assertEqual(status, 2)
                self.assertIn('first', lines[0]['undecided'])
