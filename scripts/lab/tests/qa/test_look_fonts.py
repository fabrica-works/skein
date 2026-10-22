"""The fonts command's private copy and path-independent tree identity."""
import json
import shutil

from . import FONT_SOURCE, LAB, LookCase, tree
from runner.canonical import record_bytes


class FontCopy(LookCase):
    def make_fonts(self, source, target):
        return self.cli([LAB / 'look/fonts', source, target])

    def test_output_modes_required_faces_aliases_and_stable_tree(self):
        self.assertTrue(FONT_SOURCE.is_dir(), f'missing fonts-dejavu-core: {FONT_SOURCE}')
        copies = []
        for name in ['one', 'two']:
            target = self.base / name
            result = self.make_fonts(FONT_SOURCE, target)
            self.assertEqual(result.returncode, 0, result.stderr.decode(errors='replace'))
            printed = json.loads(result.stdout)
            self.assertEqual(result.stdout, record_bytes(printed))
            self.assertEqual(set(printed), {'path', 'tree'})
            self.assertEqual(printed['path'], str(target))
            self.assertEqual(printed['tree'], tree(target))
            self.assertEqual(target.stat().st_mode & 0o777, 0o500)
            for item in target.iterdir():
                self.assertTrue(item.is_file() and not item.is_symlink(), str(item))
                self.assertEqual(item.stat().st_mode & 0o777, 0o400)
            for source in FONT_SOURCE.glob('*.ttf'):
                self.assertEqual((target / source.name).read_bytes(), source.read_bytes())
            aliases = (target / 'aliases.conf').read_text()
            for family in ['Arial', 'Helvetica', 'Times New Roman', 'Georgia', 'Courier New']:
                self.assertIn(family, aliases)
            copies.append(printed)
        self.assertEqual(copies[0]['tree'], copies[1]['tree'])

    def test_refuses_each_missing_required_regular_face(self):
        for absent in ['DejaVuSans.ttf', 'DejaVuSansMono.ttf', 'DejaVuSerif.ttf']:
            with self.subTest(absent=absent):
                source = self.base / ('source-' + absent)
                source.mkdir()
                for file in FONT_SOURCE.glob('*.ttf'):
                    if file.name != absent:
                        shutil.copyfile(file, source / file.name)
                result = self.make_fonts(source, self.base / ('fonts-' + absent))
                self.assertEqual(result.returncode, 2, result.stderr.decode(errors='replace'))
                self.assertIn(absent, result.stderr.decode(errors='replace'))

    def test_refuses_existing_destination_without_changing_it(self):
        target = self.base / 'fonts'
        target.mkdir()
        keep = target / 'keep'
        keep.write_bytes(b'existing')
        result = self.make_fonts(FONT_SOURCE, target)
        self.assertEqual(result.returncode, 2, result.stderr.decode(errors='replace'))
        self.assertEqual(list(target.iterdir()), [keep])
        self.assertEqual(keep.read_bytes(), b'existing')

    def test_required_face_symlink_is_not_a_regular_font(self):
        source = self.base / 'source'
        source.mkdir()
        for name in ['DejaVuSans.ttf', 'DejaVuSansMono.ttf', 'DejaVuSerif.ttf']:
            if name == 'DejaVuSans.ttf':
                (source / name).symlink_to(FONT_SOURCE / name)
            else:
                shutil.copyfile(FONT_SOURCE / name, source / name)
        result = self.make_fonts(source, self.base / 'fonts')
        self.assertEqual(result.returncode, 2, result.stderr.decode(errors='replace'))
        self.assertIn('DejaVuSans.ttf', result.stderr.decode(errors='replace'))
