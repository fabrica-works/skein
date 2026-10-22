"""The hand's entries speaking to the built program (lab-browser.md, 8.9)."""
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import os
from pathlib import Path
import select
import socket
import subprocess
import tempfile
import threading
import time
import unittest

PACKAGE = Path(__file__).resolve().parents[2]
FIXTURE = Path(__file__).with_name("hand")
DEFAULT_BROWSER = str(Path.home() / "src/rust/tmp/lab/browser/skein-lab-browser")
DEFAULT_CHROMIUM = str(Path.home() / ".local/opt/chromium-147.0.7727.101/usr/lib/chromium/chromium")
SECRET = "invented0123456789abcdefghijklmn"


class Page(SimpleHTTPRequestHandler):
    def do_GET(self):
        if self.path.split('?', 1)[0] == '/sign-in':
            self.path = '/sign-in.html'
        super().do_GET()

    def log_message(self, format, *args):
        # A sign-in link belongs only to the browser's input.
        pass


class HandWalk(unittest.TestCase):
    def test_sign_in_outline_type_press_wait_address_and_trouble(self):
        start = time.monotonic()
        browser = os.environ.get("SKEIN_LAB_BROWSER", DEFAULT_BROWSER)
        chromium = os.environ.get("SKEIN_TEST_CHROMIUM", DEFAULT_CHROMIUM)
        fonts = os.environ.get("SKEIN_TEST_FONTS", "/usr/share/fonts/truetype/dejavu")
        for label, path in (("SKEIN_LAB_BROWSER", browser), ("SKEIN_TEST_CHROMIUM", chromium)):
            self.assertTrue(Path(path).is_file() and os.access(path, os.X_OK),
                            f"{label}: missing executable {path}")
        self.assertTrue(Path(fonts).is_dir(), f"SKEIN_TEST_FONTS: missing {fonts}; install fonts-dejavu-core")
        self.assertEqual(len(SECRET), 32)
        server = ThreadingHTTPServer(('127.0.0.1', 0), partial(Page, directory=str(FIXTURE)))
        thread = threading.Thread(target=server.serve_forever)
        thread.start()
        try:
            origin = f'http://127.0.0.1:{server.server_port}'
            with tempfile.TemporaryDirectory(prefix='hand-walk-') as directory:
                root = Path(directory)
                scratch, out = root / 'scratch', root / 'out'
                scratch.mkdir(mode=0o700)
                out.mkdir(mode=0o700)
                stderr = root / 'stderr'
                with stderr.open('wb') as log:
                    os.chmod(stderr, 0o600)
                    process = subprocess.Popen(
                        [browser, 'hand', '--chromium', chromium, '--fonts', fonts,
                         '--scratch', str(scratch), '--out', str(out), '--origin', origin,
                         '--size', '800x600', '--listen', '127.0.0.1:0'],
                        stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=log)
                    try:
                        ready = bytearray()
                        deadline = time.monotonic() + 10
                        while not ready.endswith(b'\n'):
                            remaining = deadline - time.monotonic()
                            self.assertGreater(remaining, 0, 'hand readiness exceeded 10 seconds')
                            self.assertTrue(select.select([process.stdout], [], [], remaining)[0],
                                            'hand did not print its ready line within 10 seconds')
                            chunk = os.read(process.stdout.fileno(), 1024)
                            self.assertTrue(chunk, stderr.read_text())
                            ready.extend(chunk)
                            self.assertLessEqual(len(ready), 1024)
                        self.assertRegex(ready.decode(), r'\Alistening 127\.0\.0\.1:[1-9][0-9]*\n\Z')
                        address = ready.decode().removeprefix('listening ').strip()

                        def invoke(entry, args, status=0, input=''):
                            run = subprocess.run([str(PACKAGE / 'hand' / entry), address, *args],
                                                 input=input, text=True, capture_output=True, timeout=10)
                            self.assertNotIn(SECRET, run.stdout + run.stderr)
                            self.assertEqual(run.returncode, status, run.stderr + run.stdout)
                            self.assertEqual(run.stderr, '')
                            return run.stdout

                        opened = invoke('sign-in', [], input=f'{origin}/sign-in?secret={SECRET}\n')
                        self.assertEqual(opened, f'opened {origin}/sign-in\n')
                        outline = invoke('browse', ['outline'])
                        self.assertIn('heading 1 "Hand walk"\n', outline)
                        self.assertIn('text field "Title" = "Words already here"\n', outline)
                        self.assertEqual(invoke('browse', ['type', 'text field', 'Title'], input='new words\n'),
                                         'typed text field "Title" = "new words" [focused]\n')
                        self.assertEqual(invoke('browse', ['press', 'button', 'Save']),
                                         'pressed button "Save"\n')
                        self.assertEqual(invoke('browse', ['wait', 'status', 'new words', '--timeout', '5']),
                                         'met status "new words"\n')
                        self.assertEqual(invoke('browse', ['wait', 'button', 'No such button', '--timeout', '1'], 1),
                                         'missed\n  button "Save" [focused]\n')
                        self.assertEqual(invoke('browse', ['address']), f'{origin}/sign-in\n')
                        self.assertEqual(invoke('browse', ['trouble']), 'no trouble\n')
                        with socket.socket() as canary:
                            canary.bind(('127.0.0.1', 0))
                            canary.listen()
                            refused = subprocess.run(
                                [str(PACKAGE / 'hand/browse'), f'127.0.0.1:{canary.getsockname()[1]}',
                                 'go', '//elsewhere/'], capture_output=True, text=True, timeout=5)
                            self.assertNotIn(SECRET, refused.stdout + refused.stderr)
                            self.assertEqual(refused.returncode, 2)
                            self.assertEqual(refused.stdout, '')
                            self.assertIn('PATH must start with /, not //', refused.stderr)
                            self.assertEqual(select.select([canary], [], [], 0)[0], [], 'invalid go made a call')
                        process.terminate()
                        self.assertEqual(process.wait(timeout=10), 0, stderr.read_text())
                        self.assertEqual(process.stdout.read(), b'')
                        self.assertNotIn(SECRET, stderr.read_text())
                        self.assertNotIn(SECRET, (out / 'trouble.jsonl').read_text())
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
            self.assertFalse(thread.is_alive(), 'fixture origin did not stop')
        elapsed = time.monotonic() - start
        print(f'hand walk: {elapsed:.3f} s', flush=True)
        self.assertLess(elapsed, 10, 'hand walk exceeds its 10 second budget')
