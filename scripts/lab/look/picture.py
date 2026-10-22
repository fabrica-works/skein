"""Picture pairs, content and pixel differences (lab-browser.md, 5.3)."""
import math
from .png import Image


def side_by_side(a, b):
    width, height = a.width + 16 + b.width, max(a.height, b.height)
    rows = []
    for y in range(height):
        left = a.rows[y] if y < a.height else b"\xff" * (a.width * 3)
        right = b.rows[y] if y < b.height else b"\xff" * (b.width * 3)
        rows.append(left + b"\xff" * 48 + right)
    return Image(width, height, rows)


def content(tree, node):
    if tree is None or node is None:
        return None
    for index, entry in enumerate(tree):
        if entry["node"] == node:
            names = [entry["name"]]
            for child in tree[index + 1:]:
                if child["depth"] <= entry["depth"]:
                    break
                names.append(child["name"])
            return names
    return None


def words(tree, node):
    if tree is None or node is None:
        return None
    for index, entry in enumerate(tree):
        if entry["node"] == node:
            names = []
            for child in tree[index + 1:]:
                if child["depth"] <= entry["depth"]:
                    break
                if child["role"] == "StaticText":
                    names.append(child["name"])
            return names
    return None


def _grey(row):
    out = bytearray(len(row))
    for i in range(0, len(row), 3):
        luma = (299 * row[i] + 587 * row[i + 1] + 114 * row[i + 2]) // 1000
        g = 255 - (255 - luma) // 4
        out[i:i + 3] = bytes((g, g, g))
    return bytes(out)


def difference(a, b):
    width, height = max(a.width, b.width), max(a.height, b.height)
    differing, error, rows = 0, 0, []
    # A lookup turns the equal-row path into byte operations, including luma.
    grey_cache = {}
    for y in range(height):
        left = a.rows[y] if y < a.height else b""
        right = b.rows[y] if y < b.height else b""
        if left == right and len(left) == width * 3:
            if left not in grey_cache:
                grey_cache[left] = _grey(left)
            rows.append(grey_cache[left])
            continue
        row = bytearray(b"\xff\x00\xff" * width)
        for x in range(width):
            i = x * 3
            covered = i + 3 <= len(left) and i + 3 <= len(right)
            if covered and left[i:i + 3] == right[i:i + 3]:
                luma = (299 * left[i] + 587 * left[i + 1] + 114 * left[i + 2]) // 1000
                g = 255 - (255 - luma) // 4
                row[i:i + 3] = bytes((g, g, g))
            else:
                differing += 1
                error += sum((left[j] - right[j]) ** 2 for j in range(i, i + 3)) if covered else 3 * 255**2
        rows.append(bytes(row))
    pixels = width * height
    psnr = None if differing == 0 else round(100 * 10 * math.log10(255**2 / (error / (3 * pixels))))
    return Image(width, height, rows), differing, pixels, psnr
