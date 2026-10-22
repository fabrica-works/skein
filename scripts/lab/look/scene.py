"""Scene decisions, collecting every member's refusal (lab-browser.md, 4)."""
import re
from runner.records import NAME, Refusal, Refused
from . import canon


def read(data: bytes):
    try:
        value = canon.loads(data, "look-scene/1")
    except Refused as error:
        return [error.refusal]
    errors = []

    def bad(member, why):
        errors.append(Refusal(member, why))

    def shape(obj, fields, member):
        if type(obj) is not dict:
            bad(member, "expected an object")
            return {}
        for key in sorted(set(fields) - obj.keys()):
            bad((member + "." + key).lstrip("."), "missing member")
        for key in sorted(obj.keys() - set(fields)):
            bad((member + "." + key).lstrip("."), "unknown member")
        return obj

    def text(obj, key, member, nonempty=False):
        item = obj.get(key)
        if type(item) is not str or (nonempty and not item):
            bad(member, "expected " + ("nonempty " if nonempty else "") + "text")

    def name(obj, member):
        if type(obj.get("name")) is not str or not NAME.fullmatch(obj["name"]):
            bad(member, "expected a name matching [a-z][a-z0-9-]*")

    def finder(obj, member, selector=False):
        fields = ("selector",) if selector and type(obj) is dict and "selector" in obj else ("role", "name", "match")
        obj = shape(obj, fields, member)
        if fields == ("selector",):
            text(obj, "selector", member + ".selector", True)
        else:
            text(obj, "role", member + ".role", True)
            text(obj, "name", member + ".name")
            if obj.get("match") not in ("exact", "words"):
                bad(member + ".match", "expected exact or words")

    def side(obj, member):
        obj = shape(obj, ("path", "ready"), member)
        path = obj.get("path")
        if type(path) is not str or not re.fullmatch(r"/(?!/)[\x21-\x7e]*", path):
            bad(member + ".path", "expected an origin path in printable ASCII without spaces")
        finder(obj.get("ready"), member + ".ready")

    def array(obj, key, nonempty=False):
        items = obj.get(key)
        if type(items) is not list or (nonempty and not items):
            bad(key, "expected " + ("nonempty " if nonempty else "") + "array")
            return []
        return items

    def unique(items, prefix):
        seen = set()
        for index, obj in enumerate(items):
            if type(obj) is dict and type(obj.get("name")) is str:
                if obj["name"] in seen:
                    bad(f"{prefix}.{index}.name", "repeated name")
                seen.add(obj["name"])

    shape(value, ("format", "name", "widths", "mockup", "page", "regions", "names", "expectations", "departures"), "")
    name(value, "name")
    widths = array(value, "widths", True)
    unique(widths, "widths")
    for index, width in enumerate(widths):
        member = f"widths.{index}"
        width = shape(width, ("name", "width", "height", "mobile"), member)
        name(width, member + ".name")
        for key in ("width", "height"):
            if type(width.get(key)) is not int or not 1 <= width[key] <= 4096:
                bad(member + "." + key, "expected an integer from 1 through 4096")
        if type(width.get("mobile")) is not bool:
            bad(member + ".mobile", "expected boolean")
    if value.get("mockup") is not None:
        side(value["mockup"], "mockup")
    side(value.get("page"), "page")
    regions = array(value, "regions")
    unique(regions, "regions")
    for index, region in enumerate(regions):
        member = f"regions.{index}"
        region = shape(region, ("name", "mockup", "page"), member)
        name(region, member + ".name")
        if region.get("name") == "page":
            bad(member + ".name", "reserved")
        finder(region.get("page"), member + ".page")
        if value.get("mockup") is None:
            if region.get("mockup") is not None:
                bad(member + ".mockup", "expected null without a mockup")
        else:
            finder(region.get("mockup"), member + ".mockup", True)
    seen = set()
    for index, item in enumerate(array(value, "names")):
        if type(item) is not str or not item:
            bad(f"names.{index}", "expected nonempty text")
        elif item in seen:
            bad(f"names.{index}", "repeated name")
        else:
            seen.add(item)
    array(value, "expectations")
    array(value, "departures")
    return errors if errors else value
