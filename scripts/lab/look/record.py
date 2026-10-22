"""The capture's record and file entries (lab-browser.md, 5.2)."""
import hashlib
from runner import canonical
from runner.records import NAME, SAFE, Refused, digest_value, relative
from .style import PROPERTIES


def file(path, data):
    value = {"path": path, "bytes": len(data), "sha256": "sha256:" + hashlib.sha256(data).hexdigest()}
    _file(value, "file")
    return value


def _shape(value, keys, member):
    if type(value) is not dict or set(value) != set(keys.split()):
        raise Refused(member, "expected members: " + keys)


def _require(condition, member, why):
    if not condition:
        raise Refused(member, why)


def _int(value, member, minimum=0):
    _require(type(value) is int and minimum <= value <= SAFE, member, "invalid integer")


def _name(value, member):
    _require(type(value) is str and NAME.fullmatch(value) is not None, member, "invalid name")


def _refusal(value, member):
    if type(value) is dict and "refused" in value:
        _shape(value, "refused detail", member)
        _require(all(type(v) is str for v in value.values()), member, "expected refusal text")
        return True
    return False


def _file(value, member):
    _shape(value, "path bytes sha256", member)
    _require(relative(value["path"]), member + ".path", "expected a contained relative path")
    _int(value["bytes"], member + ".bytes")
    _require(digest_value(value["sha256"]), member + ".sha256", "invalid digest")


def _maybe_file(value, member):
    if value is not None:
        _file(value, member)


def _answer_file(value, member):
    if not _refusal(value, member):
        _file(value, member)


def _part(value, member):
    if _refusal(value, member):
        return
    _require(type(value) is dict and type(value.get("found")) is bool, member, "expected a part")
    if not value["found"]:
        _shape(value, "found", member)
        return
    _shape(value, "found more box values faces shot", member)
    _int(value["more"], member + ".more")
    box = value["box"]
    if box is not None:
        _shape(box, "x y width height", member + ".box")
        for key, number in box.items():
            _int(number, member + ".box." + key, -SAFE if key in ("x", "y") else 0)
    values = value["values"]
    _shape(values, " ".join(PROPERTIES), member + ".values")
    _require(all(type(v) is str for v in values.values()), member + ".values", "expected text")
    _require(type(value["faces"]) is list, member + ".faces", "expected faces")
    for face in value["faces"]:
        _shape(face, "family custom glyphs", member + ".faces")
        _require(type(face["family"]) is str and type(face["custom"]) is bool, member + ".faces", "invalid face")
        _int(face["glyphs"], member + ".faces.glyphs")
    if value["shot"] is not None:
        _answer_file(value["shot"], member + ".shot")


def _side(value, member, page=False):
    live = page and type(value) is dict and "focus" in value
    _shape(value, "address ready trouble tree outline shot" + (" focus" if live else ""), member)
    if live and value["focus"] is not None:
        _shape(value["focus"], "role name", member + ".focus")
        _require(all(type(v) is str for v in value["focus"].values()), member + ".focus", "expected text")
    _require(type(value["address"]) is str, member + ".address", "expected address text")
    if not _refusal(value["ready"], member + ".ready"):
        _require(value["ready"] in ("met", "missed"), member + ".ready", "expected met or missed")
    trouble = value["trouble"]
    if not _refusal(trouble, member + ".trouble"):
        _require(type(trouble) is list, member + ".trouble", "expected trouble list")
        for item in trouble:
            _shape(item, "kind text", member + ".trouble")
            _require(all(type(v) is str for v in item.values()), member + ".trouble", "expected text")
    for key in ("tree", "outline", "shot"):
        _answer_file(value[key], member + "." + key)


def check(value):
    canonical.record_bytes(value)
    _shape(value, "format browser scenes", "capture")
    _require(value["format"] == "look-capture/1", "format", "expected look-capture/1")
    _require(type(value["browser"]) is str, "browser", "expected version text")
    _require(type(value["scenes"]) is list, "scenes", "expected scenes")
    for scene in value["scenes"]:
        _shape(scene, "name scene widths", "scene")
        _name(scene["name"], "scene.name")
        _require(digest_value(scene["scene"]), "scene.scene", "invalid digest")
        _require(type(scene["widths"]) is list, "scene.widths", "expected widths")
        for width in scene["widths"]:
            _shape(width, "name width height mobile mockup page structure style pair regions", "width")
            _name(width["name"], "width.name")
            for key in ("width", "height"):
                _int(width[key], "width." + key, 1)
                _require(width[key] <= 4096, "width." + key, "size exceeds 4096")
            _require(type(width["mobile"]) is bool, "width.mobile", "expected boolean")
            if width["mockup"] is not None:
                _side(width["mockup"], "width.mockup")
            _side(width["page"], "width.page", page=True)
            _maybe_file(width["structure"], "width.structure")
            _file(width["style"], "width.style")
            _maybe_file(width["pair"], "width.pair")
            _require(type(width["regions"]) is list, "width.regions", "expected regions")
            for region in width["regions"]:
                _shape(region, "name mockup page pair content diff psnr differing pixels", "region")
                _name(region["name"], "region.name")
                if region["mockup"] is not None:
                    _part(region["mockup"], "region.mockup")
                _part(region["page"], "region.page")
                for key in ("pair", "diff"):
                    _maybe_file(region[key], "region." + key)
                _require(region["content"] in ("same", "differs", "unknown"), "region.content", "invalid content")
                for key in ("psnr", "differing", "pixels"):
                    if region[key] is not None:
                        _int(region[key], "region." + key)
                if region["diff"] is None:
                    _require(all(region[k] is None for k in ("psnr", "differing", "pixels")), "region.diff", "metrics without a difference image")
                else:
                    _require(region["content"] == "same" and type(region["differing"]) is int
                             and type(region["pixels"]) is int and 0 <= region["differing"] <= region["pixels"]
                             and ((region["psnr"] is None) == (region["differing"] == 0)), "region.diff", "invalid difference metrics")
    return value


def build(browser, scenes):
    return check({"format": "look-capture/1", "browser": browser, "scenes": scenes})
