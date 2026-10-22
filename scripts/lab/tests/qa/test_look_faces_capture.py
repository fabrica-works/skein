"""Real Chromium faces of descendant text (browser.md, 5.1)."""
import shutil

from . import FONT_SOURCE, LookCase, finder, scene


class DescendantFacesCapture(LookCase):
    def test_descendant_web_faces_merge_by_family_with_all_glyphs(self):
        for source, name in [('DejaVuSans.ttf', 'sans.ttf'), ('DejaVuSerif.ttf', 'serif.ttf')]:
            shutil.copyfile(FONT_SOURCE / source, self.page / name)
        (self.page / 'fixture.html').write_text('''<!doctype html><meta charset="utf-8"><title>Descendant faces</title>
<style>@font-face{font-family:OwnSans;src:url(/sans.ttf)}
@font-face{font-family:OwnSerif;src:url(/serif.ttf)}
main{font-family:OwnSans;font-size:16px;font-variant-ligatures:none}
strong{font-family:OwnSerif;font-weight:normal}</style>
<main aria-label="Workspace"><span>AA</span><div><strong>BB</strong><span>C</span></div></main>
<script>document.fonts.ready.then(()=>{let b=document.createElement('button');
b.textContent='Fonts settled';document.body.append(b)});</script>''')
        value = scene(paired=False)
        value['page']['ready'] = finder('button', 'Fonts settled')
        value['regions'] = [dict(name='container', mockup=None, page=finder('main', 'Workspace'))]
        _, record = self.capture([value])
        width = record['scenes'][0]['widths'][0]
        self.assertEqual(width['page']['ready'], 'met')
        part = width['regions'][0]['page']
        self.assertTrue(part['found'])
        self.assertEqual(sorted((f['family'], f['custom'], f['glyphs']) for f in part['faces']),
                         [('DejaVu Sans', True, 3), ('DejaVu Serif', True, 2)])

    def test_declared_family_without_rendered_text_has_no_used_face(self):
        (self.page / 'fixture.html').write_text('''<!doctype html><title>No glyphs</title>
<main aria-label="Workspace" style="font-family:Arial;width:30px;height:30px"><div
role="img" aria-label="Picture" style="width:10px;height:10px;background:red"></div></main>''')
        value = scene(paired=False)
        value['regions'] = [dict(name='container', mockup=None, page=finder('main', 'Workspace'))]
        _, record = self.capture([value])
        part = record['scenes'][0]['widths'][0]['regions'][0]['page']
        self.assertTrue(part['found'])
        self.assertEqual(part['faces'], [])
