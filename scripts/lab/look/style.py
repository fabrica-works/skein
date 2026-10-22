"""Computed properties and the default agreement rule (lab-browser.md, 5.3)."""
import re

PROPERTIES = ["font-family", "font-size", "font-weight", "line-height", "letter-spacing",
              "color", "background-color"] + [
    f"border-{side}-{part}" for side in ("top", "right", "bottom", "left")
    for part in ("width", "style", "color")
] + [f"border-{corner}-radius" for corner in ("top-left", "top-right", "bottom-right", "bottom-left")] + [
    "box-shadow", "padding-top", "padding-right", "padding-bottom", "padding-left", "row-gap", "column-gap"]
TOKEN = re.compile(r"-?\d+(?:\.\d+)?px|[^\s,()]+")
PX = re.compile(r"-?\d+(?:\.\d+)?px\Z")


def _length(token):
    whole, _, fraction = token[:-2].partition(".")
    return int(whole + fraction), 10 ** len(fraction)


def agree(a, b):
    aa, bb = TOKEN.findall(a), TOKEN.findall(b)
    if len(aa) != len(bb):
        return False
    for left, right in zip(aa, bb):
        if PX.fullmatch(left) and PX.fullmatch(right):
            av, scale_a = _length(left)
            bv, scale_b = _length(right)
            if abs(av * scale_b - bv * scale_a) > scale_a * scale_b:
                return False
        elif left != right:
            return False
    return True


def faces_agree(a, b):
    return sorted((f["family"], f["custom"]) for f in a) == sorted((f["family"], f["custom"]) for f in b)


def _value(part, prop):
    if not part or not part.get("found"):
        return None
    if prop == "faces":
        return ", ".join(f["family"] + (" (web)" if f["custom"] else "") for f in part["faces"])
    if prop in ("x", "y", "width", "height"):
        return None if part["box"] is None else part["box"][prop]
    return part["values"].get(prop)


def table(regions):
    lines = []
    for region in regions:
        lines.extend([f"## {region['name']}\n\n", "| property | mockup | page | |\n", "| --- | --- | --- | --- |\n"])
        for prop in PROPERTIES + ["faces", "x", "y", "width", "height"]:
            a, b = (_value(region[side], prop) for side in ("mockup", "page"))
            mark = ""
            if a is not None and b is not None:
                if prop == "faces":
                    same = faces_agree(region["mockup"]["faces"], region["page"]["faces"])
                elif prop in ("x", "y", "width", "height"):
                    same = abs(a - b) <= 1
                else:
                    same = agree(a, b)
                mark = "" if same else "≠"
            aa, bb = ("—" if v is None else str(v).replace("|", "\\|") for v in (a, b))
            lines.append(f"| {prop} | {aa} | {bb} | {mark} |\n")
        lines.append("\n")
    return "".join(lines)
