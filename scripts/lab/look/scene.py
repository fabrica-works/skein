"""Scene decisions, collecting every member's refusal (lab-browser.md, 4)."""
import re
from runner.records import NAME, Refusal, Refused
from . import canon, style


def read(data: bytes):
    try:
        value = canon.loads(data, "look-scene/1")
    except Refused as error:
        return [error.refusal]
    errors = []

    def bad(member, why):
        errors.append(Refusal(member, why))

    def shape(obj, fields, member, optional=()):
        if type(obj) is not dict:
            bad(member, "expected an object")
            return {}
        for key in sorted(set(fields) - obj.keys()):
            bad((member + "." + key).lstrip("."), "missing member")
        for key in sorted(obj.keys() - set(fields) - set(optional)):
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
        live = member == "page" and type(obj) is dict and "live" in obj
        obj = shape(obj, ("live", "ready", "focus") if live else ("path", "ready"), member)
        key = "live" if live else "path"
        path = obj.get(key)
        if (type(path) is not str or not re.fullmatch(r"/(?!/)[\x21-\x7e]*", path)
                or (live and "?" in path)):
            bad(member + "." + key, "expected an origin path in printable ASCII without spaces"
                + (" or query" if live else ""))
        finder(obj.get("ready"), member + ".ready")
        if live and obj.get("focus") is not None:
            finder(obj["focus"], member + ".focus")

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

    shape(value, ("format", "name", "widths", "mockup", "page", "regions", "names", "expectations", "departures"), "", ("words",))
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
    width_names = {w["name"] for w in widths if type(w) is dict and type(w.get("name")) is str}
    region_names = {r["name"] for r in regions if type(r) is dict and type(r.get("name")) is str}
    ids = set()
    word_regions = set()
    for kind in ("expectations", "departures", "words"):
        for index, item in enumerate([] if kind == "words" and kind not in value else array(value, kind)):
            member = f"{kind}.{index}"
            common = ("id", "width", "cites")
            by_entry = type(item) is dict and ("entry" in item or (kind == "expectations" and "count" in item))
            fields = ("region", "data") if kind == "words" else (
                (("entry", "count") if by_entry else ("region", "property", "value")) if kind == "expectations" else (
                    ("entry",) if by_entry else ("signature",)))
            item = shape(item, common + fields, member)
            identity = item.get("id")
            if type(identity) is not str or not NAME.fullmatch(identity):
                bad(member + ".id", "expected a name matching [a-z][a-z0-9-]*")
            elif identity in ids:
                bad(member + ".id", "repeated id")
            else:
                ids.add(identity)
            if type(item.get("width")) is not str or item["width"] not in width_names:
                bad(member + ".width", "expected a scene width name")
            text(item, "cites", member + ".cites", True)
            if kind == "words":
                if value.get("mockup") is None:
                    bad(member, "requires a mockup")
                region, width = item.get("region"), item.get("width")
                if type(region) is not str or region not in region_names:
                    bad(member + ".region", "expected a scene region name")
                elif type(width) is str:
                    if (width, region) in word_regions:
                        bad(member + ".region", "words repeated at this width")
                    word_regions.add((width, region))
                patterns = item.get("data")
                if type(patterns) is not list:
                    bad(member + ".data", "expected an array")
                    patterns = []
                seen = set()
                for offset, pattern in enumerate(patterns):
                    field = f"{member}.data.{offset}"
                    if type(pattern) is not str:
                        bad(field, "expected text")
                        continue
                    if pattern in seen:
                        bad(field, "repeated pattern")
                    seen.add(pattern)
                    try:
                        compiled = re.compile(pattern)
                    except re.error as error:
                        bad(field, "invalid pattern: " + str(error))
                    else:
                        if compiled.search("") is not None:
                            bad(field, "pattern matches empty text")
            elif by_entry:
                text(item, "entry", member + ".entry", True)
                if kind == "expectations" and (type(item.get("count")) is not int or item["count"] < 0):
                    bad(member + ".count", "expected a nonnegative integer")
            elif kind == "expectations":
                if type(item.get("region")) is not str or item["region"] not in region_names:
                    bad(member + ".region", "expected a scene region name")
                if item.get("property") not in style.PROPERTIES + ["faces", "x", "y", "width", "height", "shown"]:
                    bad(member + ".property", "expected a style property, faces, box member or shown")
                text(item, "value", member + ".value")
            else:
                signature = item.get("signature")
                if type(signature) is not str or not re.fullmatch(r"(?!/)[\x21-\x7e]+", signature):
                    bad(member + ".signature", "expected a relative signature in printable ASCII without spaces")
    return errors if errors else value
