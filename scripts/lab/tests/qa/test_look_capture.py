"""Real program and Chromium: records, continued reads and three views."""
from . import LookCase, finder, scene, sha


class CaptureEvidence(LookCase):
    def test_record_orders_scenes_widths_and_files(self):
        first = scene('z-last')
        first['regions'] = [dict(name='equal', mockup=finder('image', 'Equal'), page=finder('image', 'Equal'))]
        first['widths'].append(dict(name='phone', width=180, height=200, mobile=True))
        second = scene('a-first', paired=False)
        out, record = self.capture([first, second])
        self.assertTrue(record['browser'])
        self.assertEqual([s['name'] for s in record['scenes']], ['z-last', 'a-first'])
        self.assertEqual([w['name'] for w in record['scenes'][0]['widths']], ['small', 'phone'])
        for source, actual in zip([first, second], record['scenes']):
            self.assertEqual(actual['scene'], sha((self.base / (source['name'] + '.json')).read_bytes()))
        def check(value):
            if isinstance(value, dict):
                if 'path' in value:
                    self.assertEqual(set(value), {'path', 'bytes', 'sha256'})
                    path = value['path']
                    self.assertFalse(path.startswith('/'))
                    self.assertNotIn('..', path.split('/'))
                    self.assertNotIn('\\', path)
                    data = (out / path).read_bytes()
                    self.assertEqual(value['bytes'], len(data))
                    self.assertEqual(value['sha256'], sha(data))
                for item in value.values():
                    check(item)
            elif isinstance(value, list):
                for item in value:
                    check(item)
        check(record)
        self.assertTrue((out / 'browser.log').is_file())
        width = record['scenes'][1]['widths'][0]
        self.assertIsNone(width['mockup'])
        self.assertIsNone(width['structure'])
        self.assertIsNone(width['pair'])

    def test_all_declared_views_have_the_documented_paths(self):
        value = scene()
        value['regions'] = [dict(name='equal', mockup=finder('image', 'Equal'), page=finder('image', 'Equal'))]
        out, record = self.capture([value])
        width = record['scenes'][0]['widths'][0]
        for side in ['mockup', 'page']:
            for field, filename in [('tree', 'tree.json'), ('outline', 'outline.txt'), ('shot', 'page.png')]:
                self.assertEqual(width[side][field]['path'], f'sample/small/{side}/{filename}')
            self.assertTrue(width['regions'][0][side]['found'])
            self.assertEqual(width['regions'][0][side]['shot']['path'], f'sample/small/{side}/equal.png')
        for field, filename in [('structure', 'structure.diff'), ('style', 'style.md'), ('pair', 'pair.png')]:
            self.assertEqual(width[field]['path'], f'sample/small/{filename}')
        for field, filename in [('pair', 'pairs/equal.png'), ('diff', 'diff/equal.png')]:
            self.assertEqual(width['regions'][0][field]['path'], f'sample/small/{filename}')

    def test_missed_readiness_and_missing_regions_do_not_cut_later_reads(self):
        value = scene()
        value['mockup']['ready'] = finder('button', 'Never present')
        value['regions'] = [
            dict(name='absent', mockup=finder('switch', 'Missing'), page=finder('switch', 'Missing')),
            dict(name='duplicates', mockup=finder('button', 'Send'), page=finder('button', 'Send')),
            dict(name='later', mockup=finder('image', 'Equal'), page=finder('image', 'Equal')),
        ]
        out, record = self.capture([value])
        width = record['scenes'][0]['widths'][0]
        self.assertEqual(width['mockup']['ready'], 'missed')
        self.assertEqual(width['page']['ready'], 'met')
        absent, duplicate, later = width['regions']
        self.assertEqual(absent['page'], {'found': False})
        self.assertTrue(absent['mockup']['found'])
        self.assertEqual(absent['content'], 'unknown')
        for field in ['diff', 'psnr', 'differing', 'pixels']:
            self.assertIsNone(absent[field], field)
        for side in ['mockup', 'page']:
            self.assertTrue(duplicate[side]['found'])
            self.assertEqual(duplicate[side]['more'], 1)
            self.assertTrue(later[side]['found'])
            self.assertTrue((out / later[side]['shot']['path']).is_file())
            self.assertTrue((out / width[side]['shot']['path']).is_file())

    def test_content_differs_and_ignored_selector_content_is_unknown(self):
        value = scene()
        value['regions'] = [
            dict(name='content', mockup=finder('region', 'Contents'), page=finder('region', 'Contents')),
            dict(name='ignored', mockup={'selector': '#unknown'}, page=finder('image', 'Ignored')),
        ]
        _, record = self.capture([value])
        regions = record['scenes'][0]['widths'][0]['regions']
        self.assertEqual([r['content'] for r in regions], ['differs', 'unknown'])
        for region in regions:
            self.assertTrue(region['mockup']['found'])
            self.assertTrue(region['page']['found'])
            self.assertIsNotNone(region['pair'])
            for field in ['diff', 'psnr', 'differing', 'pixels']:
                self.assertIsNone(region[field], field)

    def test_structure_comes_from_tree_and_only_discloses_allowlisted_names(self):
        import difflib
        import json
        from runner.canonical import record_bytes
        landmarks = {'banner', 'navigation', 'main', 'complementary', 'contentinfo', 'region', 'form', 'search'}
        controls = {'button', 'link', 'textbox', 'searchbox', 'checkbox', 'radio', 'combobox', 'tab', 'menuitem', 'switch'}
        value = scene()
        out, record = self.capture([value])
        width = record['scenes'][0]['widths'][0]
        outlines = []
        for side in ['mockup', 'page']:
            raw = (out / width[side]['tree']['path']).read_bytes()
            answer = json.loads(raw)
            self.assertEqual(raw, record_bytes(answer))
            entries = answer['tree']
            lines, ancestors = [], []
            for index, entry in enumerate(entries):
                ancestors = [depth for depth in ancestors if depth < entry['depth']]
                role, name = entry['role'], entry['name']
                rendered = role
                if role == 'heading':
                    rendered += ' ' + str(entry['level'])
                elif role == 'list':
                    children = []
                    for child in entries[index + 1:]:
                        if child['depth'] <= entry['depth']:
                            break
                        if child['depth'] == entry['depth'] + 1 and child['role'] == 'listitem':
                            children.append(child)
                    rendered += f' ({len(children)})'
                elif role not in landmarks | controls:
                    continue
                if name in value['names']:
                    rendered += ' ' + json.dumps(name, ensure_ascii=False)
                lines.append('  ' * len(ancestors) + rendered + '\n')
                if role in landmarks:
                    ancestors.append(entry['depth'])
            expected = ''.join(lines)
            actual = (out / width[side]['outline']['path']).read_text()
            self.assertEqual(actual, expected)
            self.assertNotIn('Private', actual)
            self.assertNotIn('Secret', actual)
            self.assertIn('list (2)', actual)
            self.assertIn('heading 2', actual)
            outlines.append(actual.splitlines(keepends=True))
        expected_diff = ''.join(difflib.unified_diff(*outlines, fromfile='mockup/outline.txt',
                                                    tofile='page/outline.txt', n=3))
        self.assertEqual((out / width['structure']['path']).read_text(), expected_diff)

    def test_style_tolerance_marks_colours_exactly_and_box_lengths_within_one(self):
        value = scene()
        value['regions'] = [dict(name='resize', mockup=finder('image', 'Resize'), page=finder('image', 'Resize')),
                            dict(name='color', mockup=finder('image', 'Color'), page=finder('image', 'Color')),
                            dict(name='missing', mockup=finder('switch', 'Missing'), page=finder('switch', 'Missing'))]
        out, record = self.capture([value])
        width = record['scenes'][0]['widths'][0]
        text = (out / width['style']['path']).read_text()
        def section(name):
            start = text.index('## ' + name)
            end = text.find('\n## ', start + 1)
            return text[start:end if end >= 0 else len(text)]
        def rows(name):
            return [[cell.strip() for cell in line.strip().strip('|').split('|')]
                    for line in section(name).splitlines() if line.startswith('|')]
        resize = rows('resize')
        width_rows = [row for row in resize if row[0] in ['width', 'box.width']]
        self.assertTrue(width_rows, text)
        self.assertTrue(all('≠' not in row for row in width_rows), text)
        color_rows = [row for row in rows('color') if row[0] == 'background-color']
        self.assertEqual(len(color_rows), 1, text)
        self.assertIn('≠', color_rows[0])
        missing = rows('missing')
        self.assertTrue(any('—' in row for row in missing), text)
        self.assertTrue(all('≠' not in row for row in missing), text)
        required = ['font-family', 'font-size', 'font-weight', 'line-height', 'letter-spacing',
                    'color', 'background-color', 'box-shadow', 'row-gap', 'column-gap']
        required += [f'border-{side}-{part}' for side in ['top', 'right', 'bottom', 'left']
                     for part in ['width', 'style', 'color']]
        required += [f'border-{corner}-radius' for corner in ['top-left', 'top-right', 'bottom-left', 'bottom-right']]
        required += [f'padding-{side}' for side in ['top', 'right', 'bottom', 'left']]
        for region in width['regions'][:2]:
            for side in ['mockup', 'page']:
                self.assertEqual(set(region[side]['values']), set(required))
                self.assertTrue(all(isinstance(v, str) for v in region[side]['values'].values()))
                self.assertEqual(set(region[side]['box']), {'x', 'y', 'width', 'height'})

    def test_difference_pixels_psnr_and_pair_geometry(self):
        import math
        from . import png
        value = scene()
        value['regions'] = [dict(name=name.lower(), mockup=finder('image', name), page=finder('image', name))
                            for name in ['Equal', 'Color', 'Resize']]
        out, record = self.capture([value])
        width = record['scenes'][0]['widths'][0]
        for region in width['regions']:
            with self.subTest(region=region['name']):
                self.assertEqual(region['content'], 'same')
                left = png(out / region['mockup']['shot']['path'])
                right = png(out / region['page']['shot']['path'])
                for part, image in zip([region['mockup'], region['page']], [left, right]):
                    self.assertEqual(image[:2], (2 * part['box']['width'], 2 * part['box']['height']))
                canvas_w, canvas_h = max(left[0], right[0]), max(left[1], right[1])
                expected_different, squared_error = 0, 0
                difference = png(out / region['diff']['path'])
                self.assertEqual(difference[:2], (canvas_w, canvas_h))
                self.assertEqual(difference[3], 2, 'views are written as 8-bit RGB')
                for y in range(canvas_h):
                    for x in range(canvas_w):
                        covered = x < left[0] and x < right[0] and y < left[1] and y < right[1]
                        lp = left[2][(y * left[0] + x) * 3:(y * left[0] + x) * 3 + 3] if x < left[0] and y < left[1] else None
                        rp = right[2][(y * right[0] + x) * 3:(y * right[0] + x) * 3 + 3] if x < right[0] and y < right[1] else None
                        different = not covered or lp != rp
                        expected_different += different
                        squared_error += sum((a - b) ** 2 for a, b in zip(lp, rp)) if covered else 3 * 255 ** 2
                        pixel = difference[2][(y * canvas_w + x) * 3:(y * canvas_w + x) * 3 + 3]
                        if different:
                            self.assertEqual(pixel, b'\xff\x00\xff')
                        else:
                            self.assertEqual(len(set(pixel)), 1, 'equal pixels are grey')
                self.assertEqual(region['differing'], expected_different)
                self.assertEqual(region['pixels'], canvas_w * canvas_h)
                expected_psnr = round(1000 * math.log10(255 ** 2 * 3 * canvas_w * canvas_h / squared_error)) if squared_error else None
                self.assertEqual(region['psnr'], expected_psnr)
                self.check_pair(png(out / region['pair']['path']), left, right)
        self.check_pair(png(out / width['pair']['path']), png(out / width['mockup']['shot']['path']),
                        png(out / width['page']['shot']['path']))

    def check_pair(self, pair, left, right):
        self.assertEqual(pair[:2], (left[0] + right[0] + 16, max(left[1], right[1])))
        self.assertEqual(pair[3], 2)
        for y in range(pair[1]):
            row = bytearray(b'\xff' * pair[0] * 3)
            if y < left[1]:
                row[:left[0] * 3] = left[2][y * left[0] * 3:(y + 1) * left[0] * 3]
            if y < right[1]:
                start = (left[0] + 16) * 3
                row[start:start + right[0] * 3] = right[2][y * right[0] * 3:(y + 1) * right[0] * 3]
            self.assertEqual(pair[2][y * pair[0] * 3:(y + 1) * pair[0] * 3], row)

    def test_selected_part_without_a_box_is_recorded_and_later_regions_continue(self):
        value = scene()
        value['regions'] = [dict(name='boxless', mockup={'selector': '#boxless'}, page=finder('button', 'Send')),
                            dict(name='later', mockup=finder('button', 'Send'), page=finder('button', 'Send'))]
        out, record = self.capture([value])
        boxless, later = record['scenes'][0]['widths'][0]['regions']
        self.assertTrue(boxless['mockup']['found'])
        self.assertIsNone(boxless['mockup']['box'])
        self.assertIsNone(boxless['mockup']['shot'])
        for side in ['mockup', 'page']:
            self.assertTrue(later[side]['found'])
            self.assertTrue((out / later[side]['shot']['path']).is_file())

    def test_single_side_has_no_pairs_diffs_or_mockup_origin_requirement(self):
        self.require_native()
        value = scene(paired=False)
        value['regions'] = [dict(name='button', mockup=None, page=finder('button', 'Send'))]
        path = self.write_scene(value)
        args = self.capture_args([path])
        index = args.index('--mockups')
        del args[index:index + 2]
        result = self.cli(args)
        self.assertEqual(result.returncode, 0, result.stderr.decode(errors='replace'))
        import json
        from runner.canonical import record_bytes
        raw = (self.base / 'out/capture.json').read_bytes()
        record = json.loads(raw)
        self.assertEqual(raw, record_bytes(record))
        width = record['scenes'][0]['widths'][0]
        self.assertIsNone(width['mockup'])
        self.assertIsNone(width['structure'])
        self.assertIsNone(width['pair'])
        region = width['regions'][0]
        self.assertTrue(region['page']['found'])
        self.assertIsNone(region['mockup'])
        self.assertIsNone(region['pair'])
        self.assertEqual(region['content'], 'unknown')
        for field in ['diff', 'psnr', 'differing', 'pixels']:
            self.assertIsNone(region[field])

    def test_style_one_pixel_bound_and_glyph_counts(self):
        common = '''<!doctype html><meta charset="utf-8"><title>Style bounds</title>
<style>body{margin:0}button{display:block;font-family:"DejaVu Sans";font-size:12px;
line-height:14px;letter-spacing:0px;padding:0;border:0;box-shadow:1px 2px 3px rgb(0,0,0)}%s</style>
<main aria-label="Workspace"><button aria-label="Near">Near</button>
<button aria-label="Far">Far</button><button aria-label="Faces">%s</button></main>'''
        (self.mockups / 'fixture.html').write_text(common % ('', 'AAAA'))
        (self.page / 'fixture.html').write_text(common % (
            '[aria-label="Near"]{font-size:13px;line-height:15px;padding-top:1px;box-shadow:2px 3px 4px rgb(0,0,0)}'
            '[aria-label="Far"]{font-size:14px;line-height:16px;padding-top:2px;box-shadow:3px 4px 5px rgb(0,0,0)}', 'A'))
        value = scene()
        value['regions'] = [dict(name=name.lower(), mockup=finder('button', name), page=finder('button', name))
                            for name in ['Near', 'Far', 'Faces']]
        out, record = self.capture([value])
        width = record['scenes'][0]['widths'][0]
        text = (out / width['style']['path']).read_text()
        for name, mark in [('near', False), ('far', True)]:
            section = text.split('## ' + name + '\n', 1)[1].split('\n## ', 1)[0]
            rows = [[v.strip() for v in line.strip('|').split('|')]
                    for line in section.splitlines() if line.startswith('|')]
            for prop in ['font-size', 'line-height', 'padding-top', 'box-shadow']:
                row = next(row for row in rows if row[0] == prop)
                self.assertEqual('≠' in row, mark, f'{name}: {row}')
        faces = width['regions'][2]
        self.assertNotEqual([f['glyphs'] for f in faces['mockup']['faces']],
                            [f['glyphs'] for f in faces['page']['faces']])
        section = text.split('## faces\n', 1)[1]
        faces_rows = [line for line in section.splitlines() if line.startswith('| faces |')]
        self.assertEqual(len(faces_rows), 1, section)
        self.assertNotIn('≠', faces_rows[0])

    def test_expectations_and_departures_are_opaque_lists_to_capture(self):
        value = scene(paired=False)
        value['expectations'] = [None, False, 7, {'checks-own-this': ['arbitrary', 'shape']}]
        value['departures'] = [{'unknown-to-capture': {'member': 'value'}}]
        _, record = self.capture([value])
        self.assertEqual(record['scenes'][0]['widths'][0]['page']['ready'], 'met')

    def test_smallest_viewport_and_largest_width_are_accepted(self):
        value = scene(paired=False)
        value['widths'] = [dict(name='one', width=1, height=1, mobile=False),
                           dict(name='wide', width=4096, height=1, mobile=False)]
        out, record = self.capture([value])
        from . import png
        widths = record['scenes'][0]['widths']
        for width, expected in zip(widths, [(1, 1), (4096, 1)]):
            with self.subTest(viewport=expected):
                self.assertEqual(width['page']['ready'], 'met')
                self.assertIn('path', width['page']['shot'], width['page']['shot'])
                self.assertEqual(png(out / width['page']['shot']['path'])[:2], expected)

    def test_refused_region_call_does_not_cut_other_regions_or_page(self):
        value = scene()
        value['regions'] = [dict(name='invalid-css', mockup={'selector': '['}, page=finder('button', 'Send')),
                            dict(name='later', mockup=finder('button', 'Send'), page=finder('button', 'Send'))]
        _, record = self.capture([value])
        width = record['scenes'][0]['widths'][0]
        refused, later = width['regions']
        self.assertIn('refused', refused['mockup'])
        self.assertTrue(refused['mockup']['detail'])
        self.assertTrue(refused['page']['found'])
        self.assertEqual(refused['content'], 'unknown')
        for side in ['mockup', 'page']:
            self.assertTrue(later[side]['found'])
            self.assertIn('path', width[side]['shot'])

    def test_equal_outlines_have_an_empty_structure_diff(self):
        import shutil
        shutil.copyfile(self.mockups / 'fixture.html', self.page / 'fixture.html')
        out, record = self.capture([scene()])
        width = record['scenes'][0]['widths'][0]
        self.assertEqual((out / width['mockup']['outline']['path']).read_bytes(),
                         (out / width['page']['outline']['path']).read_bytes())
        self.assertEqual((out / width['structure']['path']).read_bytes(), b'')
