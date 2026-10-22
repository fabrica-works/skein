"""Hold a look capture to its scene files (lab-browser.md, 5.4)."""
import argparse
import hashlib
from pathlib import Path
import sys
from runner import canonical
from runner.records import Refused
from . import canon, checks, record, scene


class Arguments(argparse.ArgumentParser):
    def error(self, message):
        raise Refused("arguments", message)


def _digest(data):
    return "sha256:" + hashlib.sha256(data).hexdigest()


def _match(value, captured):
    """A capture must contain each viewport and region the scene names."""
    widths = captured["widths"]
    if [w["name"] for w in widths] != [w["name"] for w in value["widths"]]:
        raise Refused(value["name"], "capture widths differ from scene")
    for expected, width in zip(value["widths"], widths):
        member = value["name"] + "/" + width["name"]
        if any(expected[k] != width[k] for k in ("width", "height", "mobile")):
            raise Refused(member, "capture viewport differs from scene")
        if (width["mockup"] is None) != (value["mockup"] is None):
            raise Refused(member, "capture mockup differs from scene")
        if [r["name"] for r in width["regions"]] != [r["name"] for r in value["regions"]]:
            raise Refused(member, "capture regions differ from scene")
        if any((r["mockup"] is None) != (value["mockup"] is None) for r in width["regions"]):
            raise Refused(member, "capture region mockup differs from scene")


def _outline(directory, side):
    if side is None or "refused" in side["outline"]:
        return None
    entry = side["outline"]
    path = directory / entry["path"]
    data = path.read_bytes()
    if len(data) != entry["bytes"] or _digest(data) != entry["sha256"]:
        raise Refused(str(path), "outline size or digest differs from capture")
    return data.decode("utf-8", "strict")


def main(argv=None):
    fails, undecided = [], []
    try:
        parser = Arguments(description=__doc__, add_help=False)
        parser.add_argument("kind", choices=("structure", "style"))
        parser.add_argument("--capture", required=True, type=Path)
        parser.add_argument("scenes", nargs="+", type=Path)
        args = parser.parse_args(argv)
        capture = record.check(canon.loads((args.capture / "capture.json").read_bytes(), "look-capture/1"))
    except (OSError, ValueError, Refused, RecursionError) as error:
        undecided.append(str(error))
    else:
        seen = set()
        for path in args.scenes:
            try:
                data = path.read_bytes()
                value = scene.read(data)
                if isinstance(value, list):
                    undecided.extend(f"{path}: {r.member}: {r.why}" for r in value)
                    continue
                name = value["name"]
                if name in seen:
                    raise Refused(str(path), "repeated scene name: " + name)
                seen.add(name)
                matches = [s for s in capture["scenes"] if s["name"] == name]
                if len(matches) != 1:
                    raise Refused(name, "expected scene once in capture")
                captured = matches[0]
                if _digest(data) != captured["scene"]:
                    raise Refused(str(path), "scene digest differs from capture")
                _match(value, captured)
            except (OSError, ValueError, Refused, RecursionError) as error:
                undecided.append(f"{path}: {error}")
                continue
            for width in captured["widths"]:
                prefix = name + "/" + width["name"]
                try:
                    if args.kind == "structure":
                        texts = []
                        for side in ("mockup", "page"):
                            try:
                                texts.append(_outline(args.capture, width[side]))
                            except (OSError, ValueError, Refused) as error:
                                undecided.append(f"{prefix}/{side}: {error}")
                                texts.append(None)
                        result = checks.structure(value, width, *texts)
                    else:
                        result = checks.style(value, width)
                except (ValueError, Refused, RecursionError) as error:
                    undecided.append(f"{prefix}: {error}")
                    continue
                fails.extend((prefix + "/" + tail, summary) for tail, summary in result.fails)
                undecided.extend(result.undecided)
    for signature, summary in fails:
        print(canonical.dumps({"fail": signature, "summary": summary}).decode("utf-8"))
    for reason in undecided:
        print(canonical.dumps({"undecided": reason}).decode("utf-8"))
    return 1 if fails else 2 if undecided else 0


if __name__ == "__main__":
    sys.exit(main())
