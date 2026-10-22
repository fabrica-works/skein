"""Checks over a scene and its captured values (lab-browser.md, 5.4)."""
import difflib
import hashlib
import json
import re
from typing import NamedTuple
from . import outline, style as values

BOX = ("x", "y", "width", "height")


class Result(NamedTuple):
    fails: list[tuple[str, str]]
    undecided: list[str]
    notes: tuple[str, ...] = ()


def _label(text):
    role, quote, name = text.partition(' "')
    return role, json.loads('"' + name) if quote else None


def entries(outline_text: str) -> dict[str, int]:
    counts, landmarks = {}, []
    for line in outline_text.splitlines():
        if not line:
            continue
        label = line.lstrip(" ")
        depth = (len(line) - len(label)) // 2
        landmarks = landmarks[:depth]
        entry = " > ".join(landmarks + [label])
        counts[entry] = counts.get(entry, 0) + 1
        if _label(label)[0] in outline.LANDMARKS:
            landmarks.append(label)
    return counts


def _own(entry):
    start, quoted, escaped = 0, False, False
    for index, char in enumerate(entry):
        if escaped:
            escaped = False
        elif char == "\\" and quoted:
            escaped = True
        elif char == '"':
            quoted = not quoted
        elif not quoted and entry.startswith(" > ", index):
            start = index + 3
    return entry[start:]


def _encoded(text):
    return "".join(chr(byte) if (ord("a") <= byte <= ord("z") or ord("A") <= byte <= ord("Z")
                                or ord("0") <= byte <= ord("9") or byte in b"._~-")
                   else f"%{byte:02X}" for byte in text.encode("utf-8"))


def entry_signature(kind: str, entry: str) -> str:
    role, name = _label(_own(entry))
    tail = kind + "/" + role.replace("(", "").replace(")", "").replace(" ", "-")
    return tail + "/" + _encoded(name) if name else tail


def _before(a, b):
    if a["y"] + a["height"] <= b["y"] + 1:
        return True
    if b["y"] + b["height"] <= a["y"] + 1:
        return False
    return a["x"] + a["width"] <= b["x"] + 1


def _direction(a, b):
    ab, ba = _before(a, b), _before(b, a)
    return 1 if ab and not ba else -1 if ba and not ab else 0


def order(mockup_boxes: list, page_boxes: list, names: list[str]) -> list[str]:
    inverted = []
    for a in range(len(names)):
        for b in range(a + 1, len(names)):
            if all(boxes[i] is not None for boxes in (mockup_boxes, page_boxes) for i in (a, b)):
                if _direction(mockup_boxes[a], mockup_boxes[b]) * _direction(page_boxes[a], page_boxes[b]) == -1:
                    inverted.append((a, b))
    removed = set()
    while inverted:
        counts = {}
        for pair in inverted:
            for i in pair:
                counts[i] = counts.get(i, 0) + 1
        selected = max(counts, key=lambda i: (counts[i], i))
        removed.add(selected)
        inverted = [pair for pair in inverted if selected not in pair]
    return [name for i, name in enumerate(names) if i in removed]


def _refused(value):
    return isinstance(value, dict) and "refused" in value


def _reason(value):
    return value["refused"] + ": " + value["detail"]


def _prefix(scene, width):
    return scene["name"] + "/" + width["name"]


def focus_matches(finder, focus):
    if focus is None or finder["role"] != focus["role"]:
        return False
    if not finder["name"]:
        return True
    if finder["match"] == "exact":
        return finder["name"] == focus["name"]
    return all(word in focus["name"].split() for word in finder["name"].split())


def _parts(scene, width, undecided):
    captured = {region["name"]: region for region in width["regions"]}
    for region in scene["regions"]:
        pair = captured[region["name"]]
        for side in ("mockup", "page"):
            part = pair[side]
            why = None
            if _refused(part):
                why = _reason(part)
            elif side == "mockup" and part is not None and not part["found"]:
                why = "not found"
            if why is not None:
                undecided.append(f"{_prefix(scene, width)}/{region['name']}: {side} {why}")
        yield region, pair


def _expectations(scene, width):
    return [item for item in scene["expectations"] if item["width"] == width["name"]]


def _finish(scene, width, kind, differences, undecided):
    departures = [d for d in scene["departures"] if d["width"] == width["name"]
                  and ("style" if d.get("signature", "").startswith("style/") else
                       "words" if d.get("signature", "").startswith("words/") else "structure") == kind]
    used, failures = set(), {}
    for tail, summary, entry in differences:
        accepted = False
        for departure in departures:
            if departure.get("signature") == tail or (entry is not None and departure.get("entry") == entry):
                used.add(departure["id"])
                accepted = True
        if not accepted:
            failures.setdefault(tail, []).append(summary)
    for departure in departures:
        if departure["id"] not in used:
            failures["departure-gone/" + departure["id"]] = [
                f"departure {departure['id']} accepted no difference ({departure['cites']})"]
    return Result([(tail, "; ".join(summaries)) for tail, summaries in failures.items()], undecided)


def structure(scene: dict, width: dict, mockup_text: str | None, page_text: str | None) -> Result:
    differences, undecided = [], []
    prefix = _prefix(scene, width)
    finder = scene["page"].get("focus")
    if finder is not None and not focus_matches(finder, width["page"]["focus"]):
        differences.append(("focus/page", "page focus " + json.dumps(width["page"]["focus"], ensure_ascii=False)
                            + ", expected " + json.dumps(finder, ensure_ascii=False), None))
    for side in ("mockup", "page"):
        captured = width[side]
        if captured is None:
            continue
        ready = captured["ready"]
        if ready != "met":
            why = _reason(ready) if _refused(ready) else ready
            differences.append((f"ready/{side}", f"{side} readiness {why}", None))
        for call in ("ready", "trouble", "outline"):
            if _refused(captured[call]):
                undecided.append(f"{prefix}/{side}: {call} {_reason(captured[call])}")
        if not _refused(captured["trouble"]):
            trouble = {}
            for item in captured["trouble"]:
                tail = f"trouble/{side}/{item['kind']}"
                if item["kind"] == "network":
                    tail += "/" + item["text"]
                texts = trouble.setdefault(tail, [])
                if item["text"] not in texts:
                    texts.append(item["text"])
            differences.extend((tail, f"{side}: " + "; ".join(texts), None) for tail, texts in trouble.items())
    expected = entries(mockup_text) if mockup_text is not None else {}
    sources = {entry: "mockup" for entry in expected}
    for item in _expectations(scene, width):
        if "entry" in item:
            expected[item["entry"]] = item["count"]
            sources[item["entry"]] = item["id"] + ": " + item["cites"]
    if page_text is not None:
        page = entries(page_text)
        base_known = scene["mockup"] is None or mockup_text is not None
        for entry in dict.fromkeys([*expected, *page] if base_known else expected):
            want, got = expected.get(entry, 0), page.get(entry, 0)
            if want != got:
                kind = "missing" if got < want else "extra"
                differences.append((entry_signature(kind, entry),
                                    f"{entry}: page {got}, expected {want} ({sources.get(entry, 'no base entry')})", entry))
    pairs = list(_parts(scene, width, undecided))
    for region, pair in pairs:
        a, b = pair["mockup"], pair["page"]
        named = any(item.get("region") == region["name"] for item in _expectations(scene, width))
        if b.get("found") is False and ((a is not None and a.get("found")) or named):
            finder = region["page"]
            label = finder["role"] + (' ' + json.dumps(finder["name"], ensure_ascii=False) if finder["name"] else '')
            differences.append((entry_signature("missing", label), f"region {region['name']}: page part not found", None))
    if scene["mockup"] is not None:
        def boxes(side):
            return [pair[side]["box"] if pair[side] and pair[side].get("found") else None for _, pair in pairs]
        for name in order(boxes("mockup"), boxes("page"), [region["name"] for region, _ in pairs]):
            differences.append(("order/" + name, f"region {name}: page order differs from mockup", None))
    return _finish(scene, width, "structure", differences, undecided)


def _faces(faces):
    return ", ".join(sorted(face["family"] + (" (web)" if face["custom"] else "") for face in faces))


def _base(part):
    if not part or not part.get("found"):
        return {}
    return {**part["values"], "faces": _faces(part["faces"]), "shown": "yes" if part["box"] is not None else "no",
            **({prop: str(part["box"][prop]) for prop in BOX} if part["box"] is not None else {})}


def _agree(prop, expected, page):
    if prop in BOX:
        try:
            return abs(int(expected) - int(page)) <= 1
        except ValueError:
            return False
    if prop in ("faces", "shown"):
        return expected == page
    return values.agree(expected, page)


def style(scene: dict, width: dict) -> Result:
    differences, undecided = [], []
    for region, pair in _parts(scene, width, undecided):
        expected = _base(pair["mockup"])
        sources = {prop: "mockup" for prop in expected}
        overrides = {}
        for item in _expectations(scene, width):
            if item.get("region") == region["name"]:
                prop = item["property"]
                expected[prop] = overrides[prop] = item["value"]
                sources[prop] = item["id"] + ": " + item["cites"]
        page = _base(pair["page"])
        if not page:
            continue
        shown_differs = "shown" in expected and expected["shown"] != page["shown"]
        for prop, want in expected.items():
            if prop in BOX and (shown_differs or prop not in page):
                continue
            got = page.get(prop, "")
            if prop == "faces" and prop not in overrides:
                same = values.faces_agree(pair["mockup"]["faces"], pair["page"]["faces"])
            else:
                same = _agree(prop, want, got)
            if not same:
                differences.append((f"style/{region['name']}/{prop}",
                                    f"page {got}, expected {want} ({sources[prop]})", None))
    return _finish(scene, width, "style", differences, undecided)


def _word_span(words):
    tail = _encoded(" ".join(words[:8]))
    if len(words) > 8:
        digest = hashlib.sha256(" ".join(words).encode("utf-8")).hexdigest()[:8]
        tail += f"/{len(words)}w-{digest}"
    return tail


def words(scene: dict, width: dict) -> Result:
    differences, undecided, notes = [], [], []
    regions = {region["name"]: region for region in width["regions"]}
    for item in scene.get("words", []):
        if item["width"] != width["name"]:
            continue
        prefix = _prefix(scene, width) + "/" + item["id"]
        pair = regions[item["region"]]
        texts, reasons = [], []
        for side in ("mockup", "page"):
            part = pair[side]
            if _refused(part):
                reasons.append(f"{prefix}: {side} {_reason(part)}")
            elif side == "mockup" and not part["found"]:
                reasons.append(f"{prefix}: mockup not found")
            elif part["found"]:
                if part.get("words") is None:
                    reasons.append(f"{prefix}: {side} words absent or null")
                else:
                    texts.append(" ".join(part["words"]))
        undecided.extend(reasons)
        if reasons or not pair["page"].get("found"):
            continue
        for pattern in item["data"]:
            compiled = re.compile(pattern)
            matches = [list(compiled.finditer(text)) for text in texts]
            for side, spans in zip(("mockup", "page"), matches):
                for match in spans:
                    span = match.group()
                    if any(char.isspace() for char in span):
                        reasons.append(f"{prefix}: {side} data {pattern} matched white space in {span!r}")
            if reasons:
                undecided.extend(reasons)
                break
            if not any(matches):
                notes.append(f"{prefix}: data {pattern} matched nothing")
            texts = [compiled.sub("#", text) for text in texts]
        if reasons:
            continue
        a, b = (text.split() for text in texts)
        for opcode, i, j, k, l in difflib.SequenceMatcher(None, a, b, autojunk=False).get_opcodes():
            if opcode == "equal":
                continue
            summary = f"region {item['region']}: mockup {' '.join(a[i:j])!r}, page {' '.join(b[k:l])!r}"
            if opcode in ("delete", "replace"):
                differences.append((f"words/{item['region']}/missing/" + _word_span(a[i:j]), summary, None))
            if opcode in ("insert", "replace"):
                differences.append((f"words/{item['region']}/extra/" + _word_span(b[k:l]), summary, None))
    return _finish(scene, width, "words", differences, undecided)._replace(notes=tuple(notes))
