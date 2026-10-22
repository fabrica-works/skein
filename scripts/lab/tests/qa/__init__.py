"""Black-box QA against lab-browser.md; production modules stay opaque."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from runner.canonical import record_bytes, tree_digest

ROOT = Path(__file__).resolve().parents[4]
LAB = ROOT / 'scripts/lab'
FIXTURES = Path(__file__).parent / 'look'
QA = Path('/home/free/src/rust/tmp/owners/qa-b-m3/qa')
BROWSER = Path(os.environ.get('SKEIN_LAB_BROWSER', '/home/free/src/rust/tmp/lab/browser/skein-lab-browser'))
CHROMIUM = Path(os.environ.get('SKEIN_TEST_CHROMIUM', '/home/free/.local/opt/chromium-147.0.7727.101/usr/lib/chromium/chromium'))
FONTS = Path('/home/free/src/rust/tmp/lab/browser/fonts')
FONT_SOURCE = Path('/usr/share/fonts/truetype/dejavu')


def sha(data):
    return 'sha256:' + hashlib.sha256(data).hexdigest()


def tree(path):
    return tree_digest([dict(path=f.relative_to(path).as_posix(), bytes=f.stat().st_size,
                             sha256=sha(f.read_bytes()), exec=bool(f.stat().st_mode & 0o100))
                        for f in path.rglob('*') if f.is_file()])


def finder(role, name='', match='exact'):
    return dict(role=role, name=name, match=match)


def scene(name='sample', paired=True):
    side = dict(path='/fixture.html', ready=finder('main', 'Workspace'))
    return dict(format='look-scene/1', name=name,
                widths=[dict(name='small', width=240, height=240, mobile=False)],
                mockup=side.copy() if paired else None, page=side.copy(),
                regions=[], names=['Frame', 'Workspace', 'Send', 'Home'],
                expectations=[], departures=[])


class LookCase(unittest.TestCase):
    def setUp(self):
        os.umask(0o077)
        (QA / 'tmp').mkdir(mode=0o700, parents=True, exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(prefix='look-', dir=QA / 'tmp')
        self.base = Path(self.temp.name)
        self.addCleanup(self.remove_temp)
        self.page = self.base / 'page'
        self.mockups = self.base / 'mockups'
        self.page.mkdir()
        self.mockups.mkdir()
        self.scratch_space = self.base / 'temporary'
        self.scratch_space.mkdir()
        shutil.copyfile(FIXTURES / 'page.html', self.page / 'fixture.html')
        shutil.copyfile(FIXTURES / 'mockup.html', self.mockups / 'fixture.html')

    def remove_temp(self):
        # Fonts and exported subjects are deliberately read-only.
        for parent, dirs, files in os.walk(self.base):
            os.chmod(parent, 0o700)
        self.temp.cleanup()

    def cli(self, argv, *, env=None, timeout=90, cwd=None):
        environment = dict(os.environ if env is None else env, TMPDIR=str(self.scratch_space))
        return subprocess.run([str(x) for x in argv], env=environment, cwd=cwd or ROOT,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                              timeout=timeout, check=False)

    def require_native(self):
        for name, path in [('SKEIN_LAB_BROWSER', BROWSER), ('SKEIN_TEST_CHROMIUM', CHROMIUM)]:
            self.assertTrue(path.is_file() and os.access(path, os.X_OK), f'missing {name}: {path}')
        self.assertIsNotNone(shutil.which('bwrap', path=os.defpath), 'missing bubblewrap: bwrap')
        self.assertTrue(FONTS.is_dir(), f'missing fonts: {FONTS}')

    def write_scene(self, value, filename=None):
        path = self.base / (filename or value['name'] + '.json')
        path.write_bytes(record_bytes(value))
        return path

    def capture_args(self, paths, out=None):
        return [LAB / 'look/capture', '--browser', BROWSER, '--chromium', CHROMIUM,
                '--fonts', FONTS, '--page', self.page, '--mockups', self.mockups,
                '--ready-seconds', '1', '--out', out or self.base / 'out', *paths]

    def capture(self, values):
        self.require_native()
        paths = [self.write_scene(value) for value in values]
        result = self.cli(self.capture_args(paths), timeout=120)
        self.assertEqual(result.returncode, 0, result.stderr.decode(errors='replace'))
        out = self.base / 'out'
        self.assertEqual(out.stat().st_mode & 0o777, 0o700)
        raw = (out / 'capture.json').read_bytes()
        record = json.loads(raw)
        self.assertEqual(raw, record_bytes(record))
        self.assertEqual(record['format'], 'look-capture/1')
        return out, record


def png(path):
    """Independent RGB/RGBA decoder for checking the evidence's pixels."""
    import struct
    import zlib
    data = path.read_bytes()
    if data[:8] != b'\x89PNG\r\n\x1a\n':
        raise AssertionError(f'not PNG: {path}')
    pos, compressed = 8, bytearray()
    while pos < len(data):
        length = int.from_bytes(data[pos:pos + 4], 'big')
        kind = data[pos + 4:pos + 8]
        payload = data[pos + 8:pos + 8 + length]
        crc = int.from_bytes(data[pos + 8 + length:pos + 12 + length], 'big')
        if zlib.crc32(kind + payload) != crc:
            raise AssertionError(f'bad PNG CRC: {path}')
        if kind == b'IHDR':
            width, height, depth, color, compression, filtering, interlace = struct.unpack('>IIBBBBB', payload)
            if depth != 8 or color not in (2, 6) or (compression, filtering, interlace) != (0, 0, 0):
                raise AssertionError(f'not 8-bit RGB/RGBA without interlace: {path}')
        if kind == b'IDAT':
            compressed.extend(payload)
        pos += length + 12
        if kind == b'IEND':
            break
    channels = 3 if color == 2 else 4
    stride = width * channels
    raw = zlib.decompress(compressed)
    if len(raw) != (stride + 1) * height:
        raise AssertionError(f'bad PNG scanline length: {path}')
    previous, pixels = bytearray(stride), bytearray()
    for row in range(height):
        start = row * (stride + 1)
        method, line = raw[start], bytearray(raw[start + 1:start + 1 + stride])
        for index in range(stride):
            left = line[index - channels] if index >= channels else 0
            above = previous[index]
            corner = previous[index - channels] if index >= channels else 0
            if method == 0:
                predictor = 0
            elif method == 1:
                predictor = left
            elif method == 2:
                predictor = above
            elif method == 3:
                predictor = (left + above) // 2
            elif method == 4:
                p = left + above - corner
                distances = [abs(p - left), abs(p - above), abs(p - corner)]
                predictor = [left, above, corner][distances.index(min(distances))]
            else:
                raise AssertionError(f'bad PNG filter {method}: {path}')
            line[index] = (line[index] + predictor) % 256
        for x in range(width):
            pixels.extend(line[x * channels:x * channels + 3])
        previous = line
    return width, height, bytes(pixels), color
