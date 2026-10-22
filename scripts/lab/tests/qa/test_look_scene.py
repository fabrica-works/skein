"""Every scene is checked before the native browser starts (lab-browser.md, 4)."""
import json

from . import LookCase, finder, scene


class SceneRefusals(LookCase):
    def assert_refused(self, value, member, *, raw=None, extra=()):
        path = self.write_scene(value, 'candidate.json')
        if raw is not None:
            path.write_bytes(raw)
        out = self.base / ('out-' + str(len(list(self.base.glob('out-*')))))
        result = self.cli([*self.capture_args([path], out), *extra])
        self.assertEqual(result.returncode, 2, result.stderr.decode(errors='replace'))
        self.assertIn(member, result.stderr.decode(errors='replace'))
        self.assertFalse((out / 'browser.log').exists(), 'scene refusal must precede browser startup')
        self.assertFalse((out / 'capture.json').exists())

    def test_scene_schema_refusals(self):
        cases = [
            ('format', {'format': 'look-scene/9'}),
            ('surprise', {'surprise': 1}),
            ('name', {'name': '../escape'}),
            ('name', {'name': 'Uppercase'}),
            ('widths', {'widths': []}),
            ('widths', {'widths': 'small'}),
            ('page', {'page': None}),
            ('regions', {'regions': {}}),
            ('names', {'names': ['', 'Frame']}),
            ('names', {'names': ['Frame', 'Frame']}),
            ('names', {'names': [1]}),
            ('expectations', {'expectations': {}}),
            ('departures', {'departures': None}),
        ]
        for member, change in cases:
            with self.subTest(change=change):
                value = scene()
                value.update(change)
                self.assert_refused(value, member)
        for member in scene():
            with self.subTest(missing=member):
                value = scene()
                del value[member]
                self.assert_refused(value, member)

    def test_width_bounds_types_names_and_duplicates(self):
        for member, invalid in [
                ('width', 0), ('width', 4097), ('width', True), ('width', '240'),
                ('height', 0), ('height', 4097), ('height', False),
                ('mobile', 1), ('mobile', 'false'), ('name', 'bad/name'), ('extra', 0)]:
            with self.subTest(member=member, invalid=invalid):
                value = scene()
                value['widths'][0][member] = invalid
                self.assert_refused(value, member)
        value = scene()
        value['widths'] *= 2
        self.assert_refused(value, 'widths')

    def test_paths_stay_on_the_origin_and_are_printable_ascii(self):
        for path in ['relative.html', '//other.test/x', '/has space', '/tab\t', '/line\n',
                     '/café', '/delete\x7f', '/control\x1f']:
            for side in ['mockup', 'page']:
                with self.subTest(path=path, side=side):
                    value = scene()
                    value[side]['path'] = path
                    self.assert_refused(value, 'path')

    def test_finder_refusals(self):
        invalids = [
            {'selector': 'main'}, {}, finder('main', match='regex'),
            dict(finder('main'), selector='main'), finder('main', name=1),
            dict(finder('main'), extra=1), {'role': 'main', 'name': ''},
        ]
        for invalid in invalids:
            with self.subTest(ready=invalid):
                value = scene()
                value['page']['ready'] = invalid
                self.assert_refused(value, 'ready')
        value = scene()
        value['regions'] = [dict(name='target', mockup=finder('main'), page={'selector': 'main'})]
        self.assert_refused(value, 'page')
        value['regions'][0]['mockup'] = {'selector': 'main', 'role': 'main'}
        value['regions'][0]['page'] = finder('main')
        self.assert_refused(value, 'mockup')

    def test_region_names_and_mockup_pairing(self):
        for name in ['page', 'Bad', 'a/b', '']:
            with self.subTest(name=name):
                value = scene()
                value['regions'] = [dict(name=name, mockup=finder('main'), page=finder('main'))]
                self.assert_refused(value, 'name')
        value = scene()
        value['regions'] = [dict(name='part', mockup=finder('main'), page=finder('main'))] * 2
        self.assert_refused(value, 'regions')
        for paired, mockup in [(True, None), (False, finder('main'))]:
            with self.subTest(paired=paired):
                value = scene(paired=paired)
                value['regions'] = [dict(name='part', mockup=mockup, page=finder('main'))]
                self.assert_refused(value, 'mockup')

    def test_noncanonical_scene_bytes(self):
        value = scene()
        from runner.canonical import record_bytes
        canonical = record_bytes(value)
        cases = [canonical[:-1], canonical + b'\n', b' ' + canonical,
                 json.dumps(value).encode() + b'\n',
                 canonical.replace(b'"width":240', b'"width":240.0'),
                 canonical.replace(b'"width":240', b'"width":2.4e2'),
                 canonical.replace(b'"name":"sample"', b'"name":"sample","name":"sample"'),
                 canonical.replace(b'"sample"', b'"\\u0073ample"'),
                 canonical.replace(b'"width":240', b'"width":9007199254740992'),
                 canonical.replace(b'"width":240', b'"width":NaN'),
                 b'\xef\xbb\xbf' + canonical, canonical.replace(b'\n', b'\r\n'),
                 canonical.replace(b'sample', b'\xff')]
        for raw in cases:
            with self.subTest(raw=raw[:110]):
                self.assert_refused(value, '', raw=raw)

    def test_all_scenes_checked_and_duplicate_names_refused(self):
        good = self.write_scene(scene('first'))
        invalid = scene('later')
        invalid['page']['path'] = '//outside.invalid/'
        bad = self.write_scene(invalid)
        for paths in [[good, bad], [good, good]]:
            with self.subTest(paths=[p.name for p in paths]):
                out = self.base / 'batch'
                result = self.cli(self.capture_args(paths, out))
                self.assertEqual(result.returncode, 2, result.stderr.decode(errors='replace'))
                self.assertFalse((out / 'browser.log').exists())
                self.assertFalse((out / 'capture.json').exists())

    def test_another_format_is_refused_for_that_alone(self):
        value = scene()
        value['format'] = 'look-scene/0'
        value['name'] = 'BAD'
        path = self.write_scene(value, 'other.json')
        result = self.cli(self.capture_args([path]))
        self.assertEqual(result.returncode, 2, result.stderr.decode(errors='replace'))
        self.assertIn('format', result.stderr.decode(errors='replace'))

    def test_every_independent_problem_is_named(self):
        value = scene()
        value['name'] = 'BAD'
        value['widths'][0]['height'] = 0
        value['page']['path'] = '//remote.invalid/'
        path = self.write_scene(value, 'many.json')
        result = self.cli(self.capture_args([path]))
        self.assertEqual(result.returncode, 2, result.stderr.decode(errors='replace'))
        diagnostic = result.stderr.decode(errors='replace')
        for member in ['name', 'height', 'path']:
            with self.subTest(member=member):
                self.assertIn(member, diagnostic, 'lab-browser.md, 4: every problem named with its member')


class CaptureArguments(LookCase):
    def test_existing_out_is_preserved(self):
        path = self.write_scene(scene())
        out = self.base / 'out'
        out.mkdir()
        marker = out / 'keep'
        marker.write_bytes(b'private existing evidence')
        result = self.cli(self.capture_args([path]))
        self.assertEqual(result.returncode, 2, result.stderr.decode(errors='replace'))
        self.assertEqual(list(out.iterdir()), [marker])
        self.assertEqual(marker.read_bytes(), b'private existing evidence')

    def test_argument_paths_must_be_absolute_and_have_the_promised_kind(self):
        path = self.write_scene(scene())
        for flag, replacement in [('--browser', 'relative'), ('--chromium', 'relative'),
                                  ('--fonts', 'relative'), ('--page', 'relative'),
                                  ('--mockups', 'relative'), ('--out', 'relative'),
                                  ('--browser', self.base), ('--chromium', self.base),
                                  ('--fonts', path), ('--page', path),
                                  ('--ready-seconds', '0'), ('--ready-seconds', '-1'),
                                  ('--ready-seconds', '1.5')]:
            with self.subTest(flag=flag, replacement=str(replacement)):
                args = self.capture_args([path])
                args[args.index(flag) + 1] = replacement
                result = self.cli(args)
                self.assertEqual(result.returncode, 2, result.stderr.decode(errors='replace'))
                self.assertFalse((self.base / 'out/browser.log').exists())

    def test_mockups_required_only_when_a_scene_uses_them(self):
        path = self.write_scene(scene())
        args = self.capture_args([path])
        index = args.index('--mockups')
        del args[index:index + 2]
        result = self.cli(args)
        self.assertEqual(result.returncode, 2, result.stderr.decode(errors='replace'))
        self.assertIn('mockups', result.stderr.decode(errors='replace'))
        self.assertFalse((self.base / 'out/browser.log').exists())

    def test_scene_paths_and_nonexecutable_program_are_refused(self):
        path = self.write_scene(scene())
        nonexecutable = self.base / 'not-executable'
        nonexecutable.write_text('not a program\n')
        cases = []
        relative_scene = self.capture_args([path.name])
        cases.append(relative_scene)
        args = self.capture_args([path])
        args[args.index('--browser') + 1] = nonexecutable
        cases.append(args)
        for args in cases:
            with self.subTest(argv=[str(x) for x in args]):
                result = self.cli(args, cwd=self.base)
                self.assertEqual(result.returncode, 2, result.stderr.decode(errors='replace'))
                self.assertFalse((self.base / 'out/browser.log').exists())

    def test_symlink_out_is_refused_without_touching_its_target(self):
        target = self.base / 'target'
        target.mkdir()
        marker = target / 'keep'
        marker.write_bytes(b'preserve')
        (self.base / 'out').symlink_to(target, target_is_directory=True)
        result = self.cli(self.capture_args([self.write_scene(scene())]))
        self.assertEqual(result.returncode, 2, result.stderr.decode(errors='replace'))
        self.assertEqual(list(target.iterdir()), [marker])
        self.assertEqual(marker.read_bytes(), b'preserve')
