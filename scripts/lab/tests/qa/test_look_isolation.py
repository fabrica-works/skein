"""Real network namespace and fatal-origin evidence, with no substituted browser."""
import select
import socket

from . import LookCase, finder, scene


class CaptureIsolation(LookCase):
    def test_other_host_loopback_listener_cannot_be_reached(self):
        self.require_native()
        with socket.socket() as listener:
            listener.bind(('127.0.0.1', 0))
            self.assertGreaterEqual(listener.getsockname()[1], 7400)
            listener.listen()
            port = listener.getsockname()[1]
            html = f'''<!doctype html><meta charset="utf-8"><title>Network QA</title>
<main aria-label="Workspace"></main><script>
Promise.all([
 fetch('http://127.0.0.1:{port}/private-host-canary', {{mode:'no-cors'}}).catch(()=>null),
 fetch('http://qa-closed-network.invalid/private-network-canary', {{mode:'no-cors'}}).catch(()=>null)
]).then(()=>{{let b=document.createElement('button');b.textContent='Network settled';document.body.append(b)}});
</script>'''
            (self.page / 'fixture.html').write_text(html)
            value = scene(paired=False)
            value['page']['ready'] = finder('button', 'Network settled')
            out, record = self.capture([value])
            side = record['scenes'][0]['widths'][0]['page']
            self.assertFalse(select.select([listener], [], [], 0)[0],
                             'lab-browser.md, 2.4 and 5.1: another host loopback listener was reached')
            self.assertEqual(side['ready'], 'met')
            self.assertTrue(any(t['kind'] == 'network' and 'qa-closed-network.invalid' in t['text']
                                for t in side['trouble']), side['trouble'])
            self.assertTrue((out / side['shot']['path']).is_file())

    def test_network_refusals_do_not_bleed_from_mockup_to_page(self):
        for directory, host in [(self.mockups, 'mockup-only.invalid'), (self.page, 'page-only.invalid')]:
            (directory / 'fixture.html').write_text(f'''<!doctype html><title>Network attribution</title>
<main aria-label="Workspace"></main><script>
fetch('http://{host}/qa',{{mode:'no-cors'}}).catch(()=>null).then(()=>{{
let b=document.createElement('button');b.textContent='Settled';document.body.append(b)}});
</script>''')
        value = scene()
        for side in ['mockup', 'page']:
            value[side]['ready'] = finder('button', 'Settled')
        _, record = self.capture([value])
        width = record['scenes'][0]['widths'][0]
        for side, own, other in [('mockup', 'mockup-only.invalid', 'page-only.invalid'),
                                 ('page', 'page-only.invalid', 'mockup-only.invalid')]:
            with self.subTest(side=side):
                self.assertEqual(width[side]['ready'], 'met')
                network = [t['text'] for t in width[side]['trouble'] if t['kind'] == 'network']
                self.assertTrue(any(own in text for text in network), network)
                self.assertFalse(any(other in text for text in network), network)

    def assert_origin_failure(self):
        self.require_native()
        path = self.write_scene(scene(paired=False))
        result = self.cli(self.capture_args([path]))
        self.assertEqual(result.returncode, 1, result.stderr.decode(errors='replace'))
        out = self.base / 'out'
        self.assertTrue(out.is_dir(), 'failed program keeps --out')
        self.assertTrue((out / 'browser.log').is_file(), 'failed program keeps browser.log')
        self.assertFalse((out / 'capture.json').exists(), 'failed program publishes no capture.json')
        self.assertTrue(result.stderr, 'program failure is diagnosed')
        self.assertEqual(list(self.scratch_space.iterdir()), [], 'capture removes its scratch after program exit')

    def test_symlink_in_served_origin_fails_capture_and_retains_log(self):
        (self.page / 'linked.html').symlink_to(self.mockups / 'fixture.html')
        self.assert_origin_failure()

    def test_file_past_origin_limit_fails_capture_and_retains_log(self):
        with (self.page / 'oversize.bin').open('wb') as file:
            file.truncate(8 * 1024 * 1024 + 1)
        self.assert_origin_failure()

    def test_origin_special_file_is_refused_before_page_reads(self):
        import os
        os.mkfifo(self.page / 'fifo')
        self.assert_origin_failure()

    def test_query_is_left_to_the_page_and_side_origins_are_distinct(self):
        html = '''<!doctype html><title>Query QA</title><main aria-label="Workspace"></main>
<script>let b=document.createElement('button');b.textContent=new URLSearchParams(location.search).get('ready');
document.body.append(b)</script>'''
        for directory in [self.page, self.mockups]:
            (directory / 'fixture.html').write_text(html)
        value = scene()
        value['page'] = dict(path='/fixture.html?ready=Query%20page&bare', ready=finder('button', 'Query page'))
        value['mockup'] = dict(path='/fixture.html?ready=Query%20mockup&bare', ready=finder('button', 'Query mockup'))
        _, record = self.capture([value])
        width = record['scenes'][0]['widths'][0]
        from urllib.parse import urlsplit
        self.assertNotEqual(urlsplit(width['mockup']['address']).netloc, urlsplit(width['page']['address']).netloc)
        for side in ['mockup', 'page']:
            self.assertEqual(width[side]['ready'], 'met')
            self.assertTrue(width[side]['address'].endswith(value[side]['path']))

    def test_file_at_eight_mib_is_accepted(self):
        with (self.page / 'at-limit.bin').open('wb') as file:
            file.truncate(8 * 1024 * 1024)
        _, record = self.capture([scene(paired=False)])
        self.assertEqual(record['scenes'][0]['widths'][0]['page']['ready'], 'met')

    def test_origin_file_count_limit_on_both_sides(self):
        for index in range(4095):
            (self.page / f'empty-{index}').touch()
        value = scene(paired=False)
        _, record = self.capture([value])
        self.assertEqual(record['scenes'][0]['widths'][0]['page']['ready'], 'met')
        (self.page / 'one-too-many').touch()
        path = self.write_scene(value)
        result = self.cli(self.capture_args([path], self.base / 'over-count'))
        self.assertEqual(result.returncode, 1, result.stderr.decode(errors='replace'))
        self.assertFalse((self.base / 'over-count/capture.json').exists())
        self.assertIn('limit', result.stderr.decode(errors='replace'))

    def test_origin_total_bytes_limit_on_both_sides(self):
        fixture_bytes = (self.page / 'fixture.html').stat().st_size
        for index in range(8):
            size = 8 * 1024 * 1024 - (fixture_bytes if index == 7 else 0)
            with (self.page / f'data-{index}.bin').open('wb') as file:
                file.truncate(size)
        value = scene(paired=False)
        _, record = self.capture([value])
        self.assertEqual(record['scenes'][0]['widths'][0]['page']['ready'], 'met')
        (self.page / 'one-byte-more').write_bytes(b'x')
        path = self.write_scene(value)
        result = self.cli(self.capture_args([path], self.base / 'over-bytes'))
        self.assertEqual(result.returncode, 1, result.stderr.decode(errors='replace'))
        self.assertFalse((self.base / 'over-bytes/capture.json').exists())
        self.assertIn('limit', result.stderr.decode(errors='replace'))

    def test_external_font_is_refused_and_lab_font_draws_instead(self):
        html = '''<!doctype html><meta charset="utf-8"><title>Font QA</title>
<style>@font-face{font-family:Unreachable;src:url(http://qa-font.invalid/missing.woff2)}
button{font-family:Unreachable,Arial;font-size:16px}</style>
<main aria-label="Workspace"><button>Font sample</button></main>
<script>document.fonts.ready.then(()=>{let b=document.createElement('button');
b.textContent='Fonts settled';document.body.append(b)});</script>'''
        (self.page / 'fixture.html').write_text(html)
        value = scene(paired=False)
        value['page']['ready'] = finder('button', 'Fonts settled')
        value['regions'] = [dict(name='font', mockup=None, page=finder('button', 'Font sample'))]
        _, record = self.capture([value])
        width = record['scenes'][0]['widths'][0]
        self.assertEqual(width['page']['ready'], 'met')
        self.assertTrue(any(t['kind'] == 'network' and 'qa-font.invalid' in t['text']
                            for t in width['page']['trouble']), width['page']['trouble'])
        part = width['regions'][0]['page']
        self.assertTrue(part['found'])
        self.assertTrue(part['faces'])
        for face in part['faces']:
            self.assertIn('DejaVu', face['family'])
            self.assertFalse(face['custom'])
            self.assertGreater(face['glyphs'], 0)

    def test_same_origin_web_font_is_drawn_and_recorded_custom(self):
        import shutil
        from . import FONT_SOURCE
        shutil.copyfile(FONT_SOURCE / 'DejaVuSerif.ttf', self.page / 'own.ttf')
        (self.page / 'fixture.html').write_text('''<!doctype html><title>Own font QA</title>
<style>@font-face{font-family:Own;src:url(/own.ttf)}button{font-family:Own;font-size:16px}</style>
<main aria-label="Workspace"><button>Font sample</button></main>
<script>document.fonts.ready.then(()=>{let b=document.createElement('button');
b.textContent='Fonts settled';document.body.append(b)});</script>''')
        value = scene(paired=False)
        value['page']['ready'] = finder('button', 'Fonts settled')
        value['regions'] = [dict(name='font', mockup=None, page=finder('button', 'Font sample'))]
        _, record = self.capture([value])
        width = record['scenes'][0]['widths'][0]
        self.assertEqual(width['page']['ready'], 'met')
        part = width['regions'][0]['page']
        self.assertTrue(part['found'])
        self.assertTrue(any(f['custom'] and 'DejaVu Serif' in f['family'] for f in part['faces']), part['faces'])
