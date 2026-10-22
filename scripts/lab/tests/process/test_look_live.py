"""A live capture reads the typed page and returns the hand's viewport (8.10)."""
from functools import partial
from http.server import ThreadingHTTPServer
import os
from pathlib import Path
import select
import subprocess
import tempfile
import threading
import time
import unittest
from look import canon, png, record
from runner import canonical
from .test_hand_walk import Page, FIXTURE, PACKAGE, DEFAULT_BROWSER, DEFAULT_CHROMIUM


class LiveCapture(unittest.TestCase):
    def test_typed_focus_two_widths_pairs_and_hand_size_restored(self):
        browser = os.environ.get('SKEIN_LAB_BROWSER', DEFAULT_BROWSER)
        chromium = os.environ.get('SKEIN_TEST_CHROMIUM', DEFAULT_CHROMIUM)
        fonts = os.environ.get('SKEIN_TEST_FONTS', '/usr/share/fonts/truetype/dejavu')
        for label, path in (('SKEIN_LAB_BROWSER', browser), ('SKEIN_TEST_CHROMIUM', chromium)):
            self.assertTrue(Path(path).is_file() and os.access(path, os.X_OK), f'{label}: missing executable {path}')
        self.assertTrue(Path(fonts).is_dir(), f'SKEIN_TEST_FONTS: missing {fonts}')
        server = ThreadingHTTPServer(('127.0.0.1', 0), partial(Page, directory=str(FIXTURE)))
        thread = threading.Thread(target=server.serve_forever)
        thread.start()
        try:
            origin = f'http://127.0.0.1:{server.server_port}'
            # The mockup program's namespace needs its fixture and scratch outside /tmp.
            with tempfile.TemporaryDirectory(prefix='look-live-', dir='/var/tmp') as directory:
                root = Path(directory)
                scratch, hand_out = root / 'scratch', root / 'hand'
                scratch.mkdir(mode=0o700)
                hand_out.mkdir(mode=0o700)
                stderr = root / 'hand.log'
                with stderr.open('xb') as log:
                    os.chmod(stderr, 0o600)
                    process = subprocess.Popen(
                        [browser, 'hand', '--chromium', chromium, '--fonts', fonts,
                         '--scratch', str(scratch), '--out', str(hand_out), '--origin', origin,
                         '--size', '800x600', '--listen', '127.0.0.1:0'],
                        stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=log)
                    try:
                        ready = bytearray()
                        deadline = time.monotonic() + 10
                        while not ready.endswith(b'\n'):
                            remaining = deadline - time.monotonic()
                            self.assertGreater(remaining, 0, 'hand ready deadline passed')
                            self.assertTrue(select.select([process.stdout], [], [], remaining)[0])
                            chunk = os.read(process.stdout.fileno(), 1024)
                            self.assertTrue(chunk, stderr.read_text())
                            ready.extend(chunk)
                        self.assertRegex(ready.decode(), r'\Alistening 127\.0\.0\.1:[1-9][0-9]*\n\Z')
                        address = ready.decode().removeprefix('listening ').strip()
                        def browse(*args, input=''):
                            run = subprocess.run([str(PACKAGE / 'hand/browse'), address, *args],
                                                 input=input, text=True, capture_output=True, timeout=10)
                            self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
                            return run.stdout
                        browse('go', '/sign-in.html')
                        self.assertIn('"typed draft" [focused]', browse('type', 'text field', 'Title', input='typed draft\n'))
                        finder = dict(role='main', name='', match='words')
                        field = dict(role='textbox', name='Title', match='exact')
                        value = dict(format='look-scene/1', name='typed', widths=[
                            dict(name='small', width=240, height=240, mobile=False),
                            dict(name='phone', width=320, height=240, mobile=False)],
                            mockup=dict(path='/sign-in.html', ready=finder),
                            page=dict(live='/sign-in', ready=finder, focus=field),
                            regions=[dict(name='title', mockup=field, page=field)], names=['Title'],
                            expectations=[], departures=[])
                        scene_path = root / 'scene.json'
                        scene_path.write_bytes(canonical.record_bytes(value))
                        mockups = root / 'mockups'
                        mockups.mkdir()
                        (mockups / 'sign-in.html').write_bytes((FIXTURE / 'sign-in.html').read_bytes())
                        out = root / 'out'
                        run = subprocess.run([
                            str(PACKAGE / 'look/capture'), '--browser', browser, '--chromium', chromium,
                            '--fonts', fonts, '--mockups', str(mockups), '--live', address,
                            '--live-out', str(hand_out), '--ready-seconds', '2', '--out', str(out), str(scene_path)],
                            capture_output=True, timeout=40)
                        self.assertEqual(run.returncode, 0, run.stderr.decode())
                        captured = record.check(canon.loads((out / 'capture.json').read_bytes(), 'look-capture/1'))
                        widths = captured['scenes'][0]['widths']
                        self.assertEqual([w['name'] for w in widths], ['small', 'phone'])
                        for width in widths:
                            for side in ('mockup', 'page'):
                                self.assertEqual(width[side]['ready'], 'met')
                                shot = png.read((out / width[side]['shot']['path']).read_bytes())
                                self.assertEqual((shot.width, shot.height), (width['width'], width['height']))
                            self.assertEqual(width['page']['focus'], dict(role='textbox', name='Title'))
                            self.assertEqual(width['page']['address'], origin + '/sign-in')
                            self.assertNotIn('focus', width['mockup'])
                            self.assertTrue((out / width['pair']['path']).is_file())
                            region = width['regions'][0]
                            self.assertTrue((out / region['pair']['path']).is_file())
                            for side in ('mockup', 'page'):
                                crop = png.read((out / region[side]['shot']['path']).read_bytes())
                                self.assertEqual((crop.width, crop.height),
                                                 (region[side]['box']['width'] * 2, region[side]['box']['height'] * 2))
                        shot_text = browse('shot')
                        self.assertIn('800x600', shot_text)
                        shot = png.read((hand_out / shot_text.split()[1]).read_bytes())
                        self.assertEqual((shot.width, shot.height), (800, 600))
                        self.assertIn('text field "Title" = "typed draft" [focused]', browse('outline'))
                        judged = subprocess.run([str(PACKAGE / 'look/check'), 'structure', '--capture', str(out), str(scene_path)],
                                                capture_output=True, timeout=5)
                        self.assertEqual(judged.returncode, 0, judged.stdout.decode() + judged.stderr.decode())
                        process.terminate()
                        self.assertEqual(process.wait(timeout=10), 0, stderr.read_text())
                    finally:
                        if process.poll() is None:
                            process.terminate()
                            try:
                                process.wait(timeout=10)
                            except subprocess.TimeoutExpired:
                                process.kill()
                                process.wait(timeout=5)
                        process.stdout.close()
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)
            self.assertFalse(thread.is_alive())
