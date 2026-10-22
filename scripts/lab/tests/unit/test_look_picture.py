"""PNG filters and picture arithmetic on small images (lab-browser.md, 5.3)."""
import struct
import unittest
import zlib
from runner.records import Refused
from look import picture, png


def chunk(kind, data):
    return (struct.pack(">I", len(data)) + kind + data
            + struct.pack(">I", zlib.crc32(kind + data) & 0xffffffff))


def fixture(rows, channels=3, method=0, depth=8, interlace=0):
    def paeth(a, b, c):
        p = a + b - c
        return min((a, b, c), key=lambda v: abs(p - v))

    raw = bytearray()
    previous = bytes(len(rows[0]))
    for row in rows:
        raw.append(method)
        for i, value in enumerate(row):
            a = row[i - channels] if i >= channels else 0
            b = previous[i]
            c = previous[i - channels] if i >= channels else 0
            predictors = [0, a, b, (a + b) // 2, paeth(a, b, c)]
            raw.append((value - predictors[method]) & 255)
        previous = row
    header = struct.pack(">IIBBBBB", len(rows[0]) // channels, len(rows), depth, 2 if channels == 3 else 6, 0, 0, interlace)
    data = zlib.compress(raw)
    middle = len(data) // 2
    return (png.SIGNATURE + chunk(b"IHDR", header) + chunk(b"IDAT", data[:middle])
            + chunk(b"IDAT", data[middle:]) + chunk(b"IEND", b""))


class Png(unittest.TestCase):
    def test_rgb_round_trip_and_rgb_rgba_every_filter(self):
        rows = [bytes((1, 200, 3, 251, 4, 200, 99, 12, 45)), bytes((21, 99, 200, 100, 200, 4, 255, 0, 10))]
        image = png.Image(3, 2, rows)
        self.assertEqual(png.read(png.write(image)), image)
        for method in range(5):
            with self.subTest(method=method, colour="RGB"):
                self.assertEqual(png.read(fixture(rows, method=method)), image)
            rgba = [b"".join(row[i:i + 3] + bytes((i,)) for i in range(0, len(row), 3)) for row in rows]
            with self.subTest(method=method, colour="RGBA"):
                self.assertEqual(png.read(fixture(rgba, channels=4, method=method)), image)

    def test_refuses_bad_crc_truncation_filters_and_unsupported_images(self):
        data = fixture([b"\x01\x02\x03"])
        wrong_crc = data[:-1] + bytes((data[-1] ^ 1,))
        header = chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0))
        def raw(data):
            return png.SIGNATURE + header + chunk(b"IDAT", zlib.compress(data)) + chunk(b"IEND", b"")
        bad = [b"", wrong_crc, data[:-3], fixture([b"\0\0\0"], depth=16),
               fixture([b"\0\0\0"], interlace=1), raw(b"\x05\0\0\0"), raw(b"\0\0"),
               png.SIGNATURE + header + chunk(b"IDAT", b"bad") + chunk(b"IEND", b""),
               png.SIGNATURE + chunk(b"IDAT", b"") + chunk(b"IEND", b""),
               png.SIGNATURE + header + chunk(b"XXXX", b"") + chunk(b"IEND", b""),
               data + b"extra"]
        for value in bad:
            with self.subTest(value=value), self.assertRaises(Refused):
                png.read(value)
        for image in (png.Image(0, 1, [b""]), png.Image(1, 2, [b"\0\0\0"]), png.Image(1, 1, [b"x"])):
            with self.subTest(image=image), self.assertRaises(Refused):
                png.write(image)


class Picture(unittest.TestCase):
    def test_side_by_side_is_white_gapped_and_top_aligned(self):
        a = png.Image(1, 1, [b"\x01\x02\x03"])
        b = png.Image(2, 2, [b"\x04\x05\x06" * 2, b"\x07\x08\x09" * 2])
        self.assertEqual(picture.side_by_side(a, b), png.Image(19, 2, [
            b"\x01\x02\x03" + b"\xff" * 48 + b"\x04\x05\x06" * 2,
            b"\xff" * 51 + b"\x07\x08\x09" * 2]))

    def test_equal_pixels_are_faded_grey_and_have_no_psnr(self):
        a = png.Image(2, 1, [b"\0\0\0\xff\0\0"])
        image, differing, pixels, psnr = picture.difference(a, a)
        self.assertEqual((differing, pixels, psnr), (0, 2, None))
        self.assertEqual(image.rows, [bytes((192, 192, 192, 211, 211, 211))])

    def test_one_changed_pixel_has_exact_psnr(self):
        a = png.Image(2, 1, [b"\0\0\0" * 2])
        b = png.Image(2, 1, [b"\0\0\0\xff\0\0"])
        image, differing, pixels, psnr = picture.difference(a, b)
        self.assertEqual((differing, pixels, psnr), (1, 2, 778))
        self.assertEqual(image.rows, [b"\xc0\xc0\xc0\xff\0\xff"])

    def test_unequal_sizes_count_every_uncovered_canvas_pixel(self):
        a = png.Image(2, 1, [b"\0\0\0" * 2])
        b = png.Image(1, 2, [b"\0\0\0", b"\0\0\0"])
        image, differing, pixels, psnr = picture.difference(a, b)
        self.assertEqual((differing, pixels, psnr), (3, 4, 125))
        self.assertEqual(image.rows, [b"\xc0\xc0\xc0\xff\0\xff", b"\xff\0\xff" * 2])
