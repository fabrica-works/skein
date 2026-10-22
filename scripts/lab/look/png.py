"""Opaque RGB rows from Chromium's non-interlaced PNGs (lab-browser.md, 5.3)."""
from dataclasses import dataclass
import struct
import zlib
from runner.records import Refused

SIGNATURE = b"\x89PNG\r\n\x1a\n"


@dataclass
class Image:
    width: int
    height: int
    rows: list[bytes]


def _paeth(a, b, c):
    p = a + b - c
    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    return a if pa <= pb and pa <= pc else b if pb <= pc else c


def read(data):
    def refuse(why):
        raise Refused("png", why)

    if not data.startswith(SIGNATURE):
        refuse("invalid signature")
    pos, header, ended, idat_ended = 8, None, False, False
    compressed = []
    while pos < len(data):
        if pos + 12 > len(data):
            refuse("truncated chunk")
        size = struct.unpack_from(">I", data, pos)[0]
        kind = data[pos + 4:pos + 8]
        end = pos + 8 + size
        if end + 4 > len(data):
            refuse("truncated chunk")
        body = data[pos + 8:end]
        crc = zlib.crc32(body, zlib.crc32(kind)) & 0xffffffff
        if struct.unpack_from(">I", data, end)[0] != crc:
            refuse("CRC mismatch")
        if header is None and kind != b"IHDR":
            refuse("IHDR must be first")
        if kind == b"IHDR":
            if header is not None or size != 13:
                refuse("invalid IHDR")
            header = struct.unpack(">IIBBBBB", body)
            width, height, depth, colour, compression, filtering, interlace = header
            if not (0 < width < 2**31 and 0 < height < 2**31 and depth == 8
                    and colour in (2, 6) and compression == filtering == interlace == 0):
                refuse("expected 8-bit RGB or RGBA without interlace")
        elif kind == b"IDAT":
            if idat_ended:
                refuse("IDAT chunks must be consecutive")
            compressed.append(body)
        elif kind == b"IEND":
            if size or not compressed or end + 4 != len(data):
                refuse("invalid IEND")
            ended = True
            break
        else:
            if compressed:
                idat_ended = True
            if kind != b"PLTE" and (not kind or not kind[0] & 32):
                refuse("unsupported critical chunk")
        pos = end + 4
    if not ended:
        refuse("missing IEND")
    channels = 3 if colour == 2 else 4
    stride = width * channels
    expected = height * (stride + 1)
    try:
        decoder = zlib.decompressobj()
        raw = decoder.decompress(b"".join(compressed), expected + 1)
    except zlib.error as error:
        raise Refused("png", "invalid zlib stream") from error
    if len(raw) != expected or not decoder.eof or decoder.unused_data:
        refuse("invalid pixel data length")
    rows = []
    previous = bytes(stride)
    for offset in range(0, expected, stride + 1):
        method = raw[offset]
        row = bytearray(raw[offset + 1:offset + 1 + stride])
        if method == 1:
            for i in range(channels, stride):
                row[i] = (row[i] + row[i - channels]) & 255
        elif method == 2:
            row = bytearray((a + b) & 255 for a, b in zip(row, previous))
        elif method == 3:
            for i in range(stride):
                left = row[i - channels] if i >= channels else 0
                row[i] = (row[i] + (left + previous[i]) // 2) & 255
        elif method == 4:
            for i in range(stride):
                left = row[i - channels] if i >= channels else 0
                corner = previous[i - channels] if i >= channels else 0
                row[i] = (row[i] + _paeth(left, previous[i], corner)) & 255
        elif method != 0:
            refuse("unknown filter")
        previous = row
        if channels == 4:
            rgb = bytearray(width * 3)
            rgb[0::3], rgb[1::3], rgb[2::3] = row[0::4], row[1::4], row[2::4]
            rows.append(bytes(rgb))
        else:
            rows.append(bytes(row))
    return Image(width, height, rows)


def _chunk(kind, body):
    return (struct.pack(">I", len(body)) + kind + body
            + struct.pack(">I", zlib.crc32(body, zlib.crc32(kind)) & 0xffffffff))


def write(image):
    if (type(image.width) is not int or type(image.height) is not int
            or not 0 < image.width < 2**31 or not 0 < image.height < 2**31
            or len(image.rows) != image.height
            or any(type(row) is not bytes or len(row) != image.width * 3 for row in image.rows)):
        raise Refused("png", "invalid RGB image")
    header = struct.pack(">IIBBBBB", image.width, image.height, 8, 2, 0, 0, 0)
    raw = b"".join(b"\0" + row for row in image.rows)
    return SIGNATURE + _chunk(b"IHDR", header) + _chunk(b"IDAT", zlib.compress(raw, level=6)) + _chunk(b"IEND", b"")
