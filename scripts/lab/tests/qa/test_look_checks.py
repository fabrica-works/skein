"""Design-derived records through the public check CLI (lab-browser.md, 5.4)."""
import copy
import json
from urllib.parse import quote

from runner.canonical import record_bytes
from . import LAB, LookCase, finder, scene, sha


class LookChecks(LookCase):
    def setUp(self):
        super().setUp()
        self.value = scene('checks')
        self.out = self.base / 'capture'
        self.out.mkdir()
        self.width = dict(name='small', width=240, height=240, mobile=False,
                          mockup=self.side('mockup'), page=self.side('page'),
                          structure=None, style=self.file('style.md', b''), pair=None, regions=[])

    def file(self, name, data):
        path = self.out / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        return dict(path=name, bytes=len(data), sha256=sha(data))

    def side(self, name):
        return dict(address='http://127.0.0.1:7400/fixture.html', ready='met', trouble=[],
                    tree=self.file(name + '/tree.json', record_bytes({'tree': []})),
                    outline=self.file(name + '/outline.txt', b'main\n'),
                    shot=self.file(name + '/page.png', b'not read by checks'))

    def outline(self, side, text):
        self.width[side]['outline'] = self.file(side + '/outline.txt', text.encode())

    def region(self, name='target', left=None, right=None):
        def part(box):
            values = dict.fromkeys(['font-family', 'font-size', 'font-weight', 'line-height',
                                    'letter-spacing', 'color', 'background-color', 'box-shadow',
                                    'row-gap', 'column-gap'], '')
            values.update({f'border-{side}-{prop}': '' for side in ['top', 'right', 'bottom', 'left']
                           for prop in ['width', 'style', 'color']})
            values.update({f'border-{corner}-radius': '' for corner in
                           ['top-left', 'top-right', 'bottom-left', 'bottom-right']})
            values.update({f'padding-{side}': '' for side in ['top', 'right', 'bottom', 'left']})
            values['color'] = 'rgb(0, 0, 0)'
            return dict(found=True, more=0, box=box, values=values,
                        faces=[dict(family='DejaVu Sans', custom=False, glyphs=3)], shot=None)
        left = left or dict(x=0, y=0, width=20, height=10)
        right = right or dict(left)
        self.value['regions'].append(dict(name=name, mockup=finder('button', name), page=finder('button', name)))
        region = dict(name=name, mockup=part(left), page=part(right), pair=None,
                      content='same', diff=None, psnr=None, differing=None, pixels=None)
        self.width['regions'].append(region)
        return region

    def expectation(self, **fields):
        self.value['expectations'].append(dict(id='expect-' + str(len(self.value['expectations'])),
                                                width='small', cites='QA contract', **fields))

    def departure(self, **fields):
        self.value['departures'].append(dict(id='accept-' + str(len(self.value['departures'])),
                                              width='small', cites='QA contract', **fields))

    def check(self, kind):
        path = self.write_scene(self.value)
        record = dict(format='look-capture/1', browser='QA recorded browser', scenes=[
            dict(name='checks', scene=sha(path.read_bytes()), widths=[self.width])])
        (self.out / 'capture.json').write_bytes(record_bytes(record))
        return self.run_check(kind, path)

    def run_check(self, kind, path):
        result = self.cli([LAB / 'look/check', kind, '--capture', self.out, path])
        lines = [json.loads(line) for line in result.stdout.splitlines()]
        diagnostic = result.stderr.decode(errors='replace')
        self.assertIn(result.returncode, [0, 1, 2], diagnostic)
        if result.returncode == 0:
            self.assertEqual(lines, [], 'lab.md 7.2: no lines on pass')
        elif result.returncode == 1:
            self.assertTrue(any('fail' in line for line in lines), (lines, diagnostic))
        else:
            self.assertTrue(any('undecided' in line for line in lines), (lines, diagnostic))
            self.assertFalse(any('fail' in line for line in lines), 'lab.md 7.2: status 2 has no fail')
        for line in lines:
            self.assertIn(set(line), [{'fail', 'summary'}, {'undecided'}])
        return result.returncode, lines

    def failures(self, kind, expected):
        status, lines = self.check(kind)
        self.assertEqual(status, 1 if expected else 0, lines)
        failures = [line for line in lines if 'fail' in line]
        self.assertEqual({line['fail'] for line in failures}, {'checks/small/' + s for s in expected})
        self.assertEqual(len(failures), len(expected), 'one line per signature')

    def test_identical_capture_passes_both_checks(self):
        self.region()
        for kind in ['structure', 'style']:
            with self.subTest(kind=kind):
                self.failures(kind, [])

    def test_outline_counts_names_percent_encoding_and_landmark_coalescing(self):
        name = 'Go / café %?"'
        entry = 'link ' + json.dumps(name, ensure_ascii=False)
        self.value['names'].append(name)
        self.outline('mockup', 'banner\n  ' + entry + '\nmain\n  ' + entry + '\n')
        self.outline('page', 'banner\nmain\n')
        status, lines = self.check('structure')
        self.assertEqual(status, 1, lines)
        self.assertEqual(len(lines), 1, lines)
        self.assertEqual(lines[0]['fail'], 'checks/small/missing/link/' + quote(name, safe='-._~'))
        self.assertIn('banner', lines[0]['summary'])
        self.assertIn('main', lines[0]['summary'])

    def test_count_expectation_overrides_base_in_one_landmark_only(self):
        self.outline('mockup', 'banner\n  link "Home"\nmain\n  link "Home"\n')
        self.outline('page', 'banner\nmain\n  link "Home"\n')
        self.expectation(entry='banner > link "Home"', count=0)
        self.failures('structure', [])

    def test_count_multiplicity_heading_and_list_signatures(self):
        self.outline('mockup', 'main\n  heading 2\n  list (3)\n  button "Send"\n  button "Send"\n')
        self.outline('page', 'main\n  heading 3\n  list (2)\n  button "Send"\n')
        self.failures('structure', ['missing/heading-2', 'extra/heading-3', 'missing/list-3',
                                    'extra/list-2', 'missing/button/Send'])

    def test_no_mockup_is_held_to_expectations_alone(self):
        # No base: an entry no expectation names is expected none times.
        self.value['mockup'] = None
        self.width['mockup'] = None
        self.outline('page', 'main\n  button "Send"\n  textbox\n')
        self.failures('structure', ['extra/main', 'extra/button/Send', 'extra/textbox'])
        self.expectation(entry='main', count=1)
        self.expectation(entry='main > button "Send"', count=1)
        self.expectation(entry='main > textbox', count=1)
        self.failures('structure', [])
        self.value['expectations'][1]['count'] = 2
        self.failures('structure', ['missing/button/Send'])

    def test_property_expectation_overrides_mockup_only_for_named_property(self):
        region = self.region()
        region['page']['values']['color'] = 'rgb(1, 2, 3)'
        self.expectation(region='target', property='color', value='rgb(1, 2, 3)')
        self.failures('style', [])
        region['page']['box']['width'] += 2
        self.failures('style', ['style/target/width'])

    def test_px_tolerance_exact_boundary_and_non_px_tokens(self):
        region = self.region()
        cases = [
            ('padding-top', '4px', '5px', False),
            ('padding-top', '-4px', '-5px', False),
            ('padding-top', '4px', '6px', True),
            ('box-shadow', '1px 2px 3px rgb(0, 0, 0)', '2px 3px 4px rgb(0,0,0)', False),
            ('box-shadow', '1px 2px 3px rgb(0,0,0)', '2px 3px 4px rgb(0,0,1)', True),
            ('font-weight', '400', '401', True),
            ('line-height', 'normal', 'normal', False),
        ]
        for prop, left, right, fails in cases:
            with self.subTest(prop=prop, left=left, right=right):
                region['mockup']['values'][prop] = left
                region['page']['values'][prop] = right
                self.failures('style', ['style/target/' + prop] if fails else [])
                region['page']['values'][prop] = left

    def test_faces_ignore_order_and_glyphs_but_compare_custom_flag(self):
        region = self.region()
        region['mockup']['faces'] = [dict(family='Second', custom=True, glyphs=2),
                                      dict(family='First', custom=False, glyphs=5)]
        region['page']['faces'] = [dict(family='First', custom=False, glyphs=1),
                                    dict(family='Second', custom=True, glyphs=99)]
        self.failures('style', [])
        region['page']['faces'][1]['custom'] = False
        self.failures('style', ['style/target/faces'])

    def test_faces_expectation_uses_sorted_families_and_web_marker(self):
        region = self.region()
        region['page']['faces'] = [dict(family='Zulu', custom=True, glyphs=1),
                                    dict(family='Alpha', custom=False, glyphs=5)]
        self.expectation(region='target', property='faces', value='Alpha, Zulu (web)')
        self.failures('style', [])

    def test_shown_difference_suppresses_box_comparisons(self):
        region = self.region()
        region['page']['box'] = None
        self.failures('style', ['style/target/shown'])
        self.expectation(region='target', property='shown', value='no')
        self.failures('style', [])

    def test_missing_page_region_uses_its_finder_signature(self):
        region = self.region()
        self.value['regions'][0]['page'] = finder('button', 'Do / it')
        region['page'] = {'found': False}
        self.failures('structure', ['missing/button/Do%20%2F%20it'])

    def test_missing_mockup_region_is_undecided_for_that_region(self):
        region = self.region()
        region['mockup'] = {'found': False}
        for kind in ['structure', 'style']:
            with self.subTest(kind=kind):
                status, lines = self.check(kind)
                self.assertEqual(status, 2, lines)
                self.assertIn('target', str(lines))

    def test_refused_part_is_undecided(self):
        region = self.region()
        region['page'] = {'refused': 'limit', 'detail': 'bounded read'}
        for kind in ['structure', 'style']:
            with self.subTest(kind=kind):
                status, lines = self.check(kind)
                self.assertEqual(status, 2, lines)

    def test_signature_departure_suppresses_failure_then_goes_stale(self):
        region = self.region()
        region['page']['values']['color'] = 'rgb(1, 0, 0)'
        self.departure(signature='style/target/color')
        self.failures('style', [])
        self.failures('structure', [])
        region['page']['values']['color'] = 'rgb(0, 0, 0)'
        self.failures('style', ['departure-gone/accept-0'])
        self.failures('structure', [])

    def test_entry_departure_accepts_count_difference_and_goes_stale(self):
        self.outline('mockup', 'main\n  button "Send"\n')
        self.outline('page', 'main\n')
        self.departure(entry='main > button "Send"')
        self.failures('structure', [])
        self.failures('style', [])
        self.outline('page', 'main\n  button "Send"\n')
        self.failures('structure', ['departure-gone/accept-0'])

    def test_expectation_can_make_an_accepted_difference_stale(self):
        region = self.region()
        region['page']['values']['color'] = 'rgb(9, 0, 0)'
        self.departure(signature='style/target/color')
        self.expectation(region='target', property='color', value='rgb(9, 0, 0)')
        self.failures('style', ['departure-gone/accept-0'])

    def test_departure_width_does_not_accept_other_widths(self):
        region = self.region()
        region['page']['values']['color'] = 'red'
        self.value['widths'].append(dict(name='other', width=100, height=100, mobile=False))
        self.value['departures'].append(dict(id='other-only', width='other', cites='QA', signature='style/target/color'))
        path = self.write_scene(self.value)
        other = copy.deepcopy(self.width)
        other.update(name='other', width=100, height=100)
        record = dict(format='look-capture/1', browser='QA', scenes=[dict(
            name='checks', scene=sha(path.read_bytes()), widths=[self.width, other])])
        (self.out / 'capture.json').write_bytes(record_bytes(record))
        status, lines = self.run_check('style', path)
        self.assertEqual(status, 1, lines)
        self.assertEqual([line['fail'] for line in lines], ['checks/small/style/target/color'])

    def test_trouble_deduplicates_network_hosts_and_keeps_kinds_and_sides(self):
        self.width['mockup']['ready'] = 'missed'
        self.width['page']['ready'] = {'refused': 'timeout', 'detail': 'readiness'}
        self.width['mockup']['trouble'] = [dict(kind='console', text='mockup error')]
        self.width['page']['trouble'] = [dict(kind='network', text='fonts.invalid:443'),
                                         dict(kind='network', text='fonts.invalid:443'),
                                         dict(kind='network', text='cdn.invalid:80'),
                                         dict(kind='log', text='failed request')]
        self.failures('structure', ['ready/mockup', 'ready/page', 'trouble/mockup/console',
                                    'trouble/page/network/fonts.invalid:443',
                                    'trouble/page/network/cdn.invalid:80', 'trouble/page/log'])

    def test_readiness_and_network_departures_go_stale_in_structure_only(self):
        self.width['mockup']['ready'] = 'missed'
        self.width['mockup']['trouble'] = [dict(kind='network', text='fonts.invalid:443')]
        self.departure(signature='ready/mockup')
        self.departure(signature='trouble/mockup/network/fonts.invalid:443')
        self.failures('structure', [])
        self.width['mockup']['ready'] = 'met'
        self.width['mockup']['trouble'] = []
        self.failures('structure', ['departure-gone/accept-0', 'departure-gone/accept-1'])
        self.failures('style', [])

    def test_reversed_three_regions_names_the_later_tie_then_remaining_pair(self):
        for name, left, right in [('a', 0, 40), ('b', 20, 20), ('c', 40, 0)]:
            self.region(name, dict(x=0, y=left, width=10, height=10),
                        dict(x=0, y=right, width=10, height=10))
        self.failures('structure', ['order/c', 'order/b'])

    def test_moved_composer_is_named_instead_of_regions_it_passed(self):
        for name, left, right in [('composer', 60, 0), ('one', 0, 20), ('two', 20, 40), ('three', 40, 60)]:
            self.region(name, dict(x=0, y=left, width=10, height=10),
                        dict(x=0, y=right, width=10, height=10))
        self.failures('structure', ['order/composer'])

    def test_horizontal_order_is_used_for_vertical_overlap(self):
        self.region('a', dict(x=0, y=0, width=10, height=20), dict(x=20, y=0, width=10, height=20))
        self.region('b', dict(x=20, y=0, width=10, height=20), dict(x=0, y=0, width=10, height=20))
        self.failures('structure', ['order/b'])

    def test_nested_or_crossing_pair_is_never_inverted(self):
        for box in [dict(x=2, y=2, width=5, height=5), dict(x=5, y=-5, width=20, height=20)]:
            with self.subTest(box=box):
                self.width['regions'] = []
                self.value['regions'] = []
                self.region('outer', dict(x=0, y=0, width=20, height=20), dict(x=0, y=30, width=20, height=20))
                self.region('inner', box, dict(x=0, y=0, width=20, height=20))
                self.failures('structure', [])

    def test_scene_digest_and_outline_file_integrity_are_required(self):
        self.check('structure')
        path = self.base / 'checks.json'
        original = path.read_bytes()
        self.value['names'].append('Changed')
        path.write_bytes(record_bytes(self.value))
        status, lines = self.run_check('structure', path)
        self.assertEqual(status, 2, lines)
        path.write_bytes(original)
        outline = self.out / self.width['page']['outline']['path']
        for changed in [b'x' * outline.stat().st_size, b'longer than the recorded file\n', None]:
            with self.subTest(changed=changed):
                if changed is None:
                    outline.unlink()
                else:
                    outline.write_bytes(changed)
                status, lines = self.run_check('structure', path)
                self.assertEqual(status, 2, lines)

    def test_check_uses_record_properties_not_style_markdown_or_picture(self):
        region = self.region()
        region['page']['values']['color'] = 'rgb(1, 0, 0)'
        self.width['style'] = self.file('style.md', b'All properties agree.\n')
        self.failures('style', ['style/target/color'])

    def test_record_missing_noncanonical_or_another_format_is_undecided(self):
        path = self.write_scene(self.value)
        record = self.out / 'capture.json'
        for raw in [None, b'not JSON\n', b'{"format":"look-capture/9"}\n',
                    b' {"format":"look-capture/1"}\n']:
            with self.subTest(raw=raw):
                if raw is None:
                    if record.exists():
                        record.unlink()
                else:
                    record.write_bytes(raw)
                for kind in ['structure', 'style']:
                    status, lines = self.run_check(kind, path)
                    self.assertEqual(status, 2, lines)

    def test_scene_absent_from_record_is_undecided(self):
        path = self.write_scene(self.value)
        (self.out / 'capture.json').write_bytes(record_bytes(dict(
            format='look-capture/1', browser='QA', scenes=[])))
        for kind in ['structure', 'style']:
            with self.subTest(kind=kind):
                status, lines = self.run_check(kind, path)
                self.assertEqual(status, 2, lines)
                self.assertIn('checks', str(lines))

    def test_unread_picture_does_not_prevent_structure_or_style_judgement(self):
        self.region()
        for side in ['mockup', 'page']:
            (self.out / self.width[side]['shot']['path']).unlink()
        self.failures('structure', [])
        self.failures('style', [])

