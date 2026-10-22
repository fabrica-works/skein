"""Drive the lab's browser and write the three views (lab-browser.md, 5)."""
import argparse
import os
from pathlib import Path
import selectors
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from runner import canonical
from runner.records import Refused
from . import canon, outline, picture, png, record, scene, style


class ProgramFailure(Exception):
    pass


class Program:
    def __init__(self, argv, log, seconds):
        self.seconds = seconds
        self.pending = b""
        self.child = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                      stderr=log, bufsize=0, start_new_session=True)
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.child.stdout, selectors.EVENT_READ)

    def answer(self):
        deadline = time.monotonic() + self.seconds
        while b"\n" not in self.pending:
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not self.selector.select(remaining):
                raise ProgramFailure("program answer deadline passed")
            chunk = os.read(self.child.stdout.fileno(), 65536)
            if not chunk:
                raise ProgramFailure("program ended before an answer")
            self.pending += chunk
        line, self.pending = self.pending.split(b"\n", 1)
        try:
            return canon.loads(line, None)
        except Refused as error:
            raise ProgramFailure("program answer: " + str(error)) from error

    def call(self, call, **members):
        try:
            data = canonical.dumps({"call": call, **members}) + b"\n"
            remaining = memoryview(data)
            while remaining:
                written = self.child.stdin.write(remaining)
                if not written:
                    raise ProgramFailure("program stopped reading calls")
                remaining = remaining[written:]
            self.child.stdin.flush()
        except (OSError, ValueError) as error:
            raise ProgramFailure("cannot send " + call + ": " + str(error)) from error
        answer = self.answer()
        result = {"serve": "served", "open": "opened", "go": "done", "await": "met",
                  "tree": "tree", "find": "found", "select": "selected", "style": "style",
                  "shot": "shot", "trouble": "trouble", "close": "closed"}[call]
        if "refused" in answer:
            if set(answer) != {"refused", "detail"} or any(type(v) is not str for v in answer.values()):
                raise ProgramFailure("malformed refusal: " + repr(answer))
            if call in ("serve", "open", "close"):
                raise ProgramFailure(call + ": " + answer["refused"] + ": " + answer["detail"])
        elif result not in answer and not (call == "await" and "missed" in answer):
            raise ProgramFailure("unexpected answer to " + call + ": " + repr(answer))
        return answer

    def finish(self):
        self.child.stdin.close()
        try:
            status = self.child.wait(timeout=self.seconds)
        except subprocess.TimeoutExpired as error:
            raise ProgramFailure("program exit deadline passed") from error
        if status != 0:
            raise ProgramFailure("program exited " + str(status))
        # A process of its namespace may still hold the output a moment.
        if not self.selector.select(self.seconds):
            raise ProgramFailure("program left its output pipe open")
        if self.pending or os.read(self.child.stdout.fileno(), 1):
            raise ProgramFailure("unexpected program output after its final answer")

    def dispose(self):
        self.child.stdin.close()
        try:
            self.child.wait(timeout=self.seconds)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(self.child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            self.child.wait(timeout=self.seconds)
        self.selector.close()
        for stream in (self.child.stdin, self.child.stdout):
            stream.close()


class Capture:
    def __init__(self, program, out, ready_seconds, addresses):
        self.program, self.out = program, out
        self.ready_seconds, self.addresses = ready_seconds, addresses

    def write(self, path, data):
        target = self.out / path
        target.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        with target.open("xb") as stream:
            stream.write(data)
        return record.file(path, data)

    def image(self, entry):
        if entry is None or "refused" in entry:
            return None
        return png.read((self.out / entry["path"]).read_bytes())

    def shot(self, path, clip, scale):
        answer = self.program.call("shot", page="p", clip=clip, scale=scale, file=path)
        if "refused" in answer:
            return answer
        result = answer["shot"]
        entry = record.file(path, (self.out / path).read_bytes())
        if (result["file"] != path or result["bytes"] != entry["bytes"]
                or result["sha256"] != entry["sha256"].removeprefix("sha256:")):
            raise ProgramFailure("shot metadata differs from its file: " + path)
        return entry

    def part(self, finder, path):
        call = "select" if "selector" in finder else "find"
        answer = self.program.call(call, page="p", **finder)
        if "refused" in answer:
            return answer, None
        if call == "select":
            node, more = answer["selected"], 0
        else:
            found = answer["found"]
            node = found[0]["node"] if found else None
            more = max(0, len(found) - 1) + answer["more"]
        if node is None:
            return {"found": False}, None
        answer = self.program.call("style", page="p", node=node, properties=style.PROPERTIES)
        if "refused" in answer:
            return answer, node
        values = answer["style"]
        shot = None if values["box"] is None else self.shot(path, values["box"], 2)
        return {"found": True, "more": more, "box": values["box"], "values": values["values"],
                "faces": values["faces"], "shot": shot}, node

    def side(self, scene, width, side, base):
        self.program.call("open", page="p", **{key: width[key] for key in ("width", "height", "mobile")})
        address = self.addresses[side] + scene[side]["path"]
        ready = self.program.call("go", page="p", url=address)
        if "refused" not in ready:
            answer = self.program.call("await", page="p", seconds=self.ready_seconds, **scene[side]["ready"])
            ready = answer if "refused" in answer else "met" if "met" in answer else "missed"
        tree_answer = self.program.call("tree", page="p")
        tree, text = None, None
        if "refused" in tree_answer:
            tree_file, outline_file = tree_answer, tree_answer
        else:
            tree = tree_answer["tree"]
            tree_file = self.write(base + "/tree.json", canonical.record_bytes(tree_answer))
            text = outline.outline(tree, scene["names"])
            outline_file = self.write(base + "/outline.txt", text.encode("utf-8"))
        parts, contents = [], []
        for region in scene["regions"]:
            part, node = self.part(region[side], base + "/" + region["name"] + ".png")
            parts.append(part)
            contents.append(picture.content(tree, node) if part.get("found") else None)
        shot = self.shot(base + "/page.png", None, 1)
        trouble = self.program.call("trouble", page="p")
        self.program.call("close", page="p")
        return ({"address": address, "ready": ready, "tree": tree_file, "outline": outline_file,
                 "shot": shot, "trouble": trouble if "refused" in trouble else trouble["trouble"]}, parts, contents, text)

    def width(self, scene, width):
        base = scene["name"] + "/" + width["name"]
        sides = {}
        for side in ("mockup", "page"):
            if scene[side] is not None:
                sides[side] = self.side(scene, width, side, base + "/" + side)
        mockup, page = sides.get("mockup"), sides["page"]
        regions = []
        for index, region in enumerate(scene["regions"]):
            a, b = mockup[1][index] if mockup else None, page[1][index]
            ac, bc = mockup[2][index] if mockup else None, page[2][index]
            content = "unknown" if ac is None or bc is None else "same" if ac == bc else "differs"
            value = {"name": region["name"], "mockup": a, "page": b, "content": content,
                     "pair": None, "diff": None, "psnr": None, "differing": None, "pixels": None}
            ai = self.image(a.get("shot")) if a and a.get("found") else None
            bi = self.image(b.get("shot")) if b.get("found") else None
            if ai is not None and bi is not None:
                value["pair"] = self.write(base + "/pairs/" + region["name"] + ".png", png.write(picture.side_by_side(ai, bi)))
                if content == "same":
                    image, differing, pixels, psnr = picture.difference(ai, bi)
                    value.update(diff=self.write(base + "/diff/" + region["name"] + ".png", png.write(image)),
                                 differing=differing, pixels=pixels, psnr=psnr)
            regions.append(value)
        structure, pair = None, None
        if mockup:
            if mockup[3] is not None and page[3] is not None:
                structure = self.write(base + "/structure.diff", outline.diff(mockup[3], page[3]).encode("utf-8"))
            ai, bi = self.image(mockup[0]["shot"]), self.image(page[0]["shot"])
            if ai is not None and bi is not None:
                pair = self.write(base + "/pair.png", png.write(picture.side_by_side(ai, bi)))
        table = f"# {scene['name']} {width['name']}\n\n" + style.table(regions)
        return {**width, "mockup": mockup[0] if mockup else None, "page": page[0],
                "structure": structure, "style": self.write(base + "/style.md", table.encode("utf-8")),
                "pair": pair, "regions": regions}


def arguments(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    for option in ("browser", "chromium", "fonts", "page", "out"):
        parser.add_argument("--" + option, required=True, type=Path)
    parser.add_argument("--mockups", type=Path)
    parser.add_argument("--ready-seconds", required=True, type=int)
    parser.add_argument("scenes", nargs="+", type=Path)
    args = parser.parse_args(argv)
    errors, scenes, names = [], [], set()
    for key in ("browser", "chromium", "fonts", "mockups", "page", "out"):
        path = getattr(args, key)
        if path is None:
            continue
        if not path.is_absolute():
            errors.append(f"--{key}: expected an absolute path")
        elif key in ("browser", "chromium"):
            if not path.is_file() or not os.access(path, os.X_OK):
                errors.append(f"--{key}: expected an executable file: {path}")
        elif key != "out" and not path.is_dir():
            errors.append(f"--{key}: expected a directory: {path}")
    if os.path.lexists(args.out):
        errors.append("--out: directory exists")
    if args.ready_seconds <= 0:
        errors.append("--ready-seconds: expected a positive integer")
    for path in args.scenes:
        try:
            if not path.is_absolute():
                errors.append(str(path) + ": expected an absolute scene path")
            data = path.read_bytes()
            value = scene.read(data)
            if type(value) is list:
                errors.extend(str(path) + ": " + str(error) for error in value)
                continue
            if value["name"] in names:
                errors.append(str(path) + ": name: repeated scene name")
            names.add(value["name"])
            if value["mockup"] is not None and args.mockups is None:
                errors.append(str(path) + ": --mockups: required for this scene")
            scenes.append((value, record.file("scene.json", data)["sha256"]))
        except OSError as error:
            errors.append(str(path) + ": " + str(error))
    bwrap = shutil.which("bwrap", path=os.defpath)
    if bwrap is None:
        errors.append("bwrap: not found on the default path")
    if errors:
        raise Refused("capture", "\n".join(errors))
    return args, scenes, bwrap


def remove_scratch(path):
    for attempt in range(2):
        try:
            shutil.rmtree(path)
        except OSError as error:
            if not os.path.lexists(path):
                return
            if attempt == 1:
                raise ProgramFailure(f"cannot remove scratch {path}: {error}") from error
        else:
            if not os.path.lexists(path):
                return
            if attempt == 1:
                raise ProgramFailure(f"scratch reappeared: {path}")
        time.sleep(0.1)


def main(argv=None):
    os.umask(0o077)
    try:
        args, scenes, bwrap = arguments(argv)
        args.out.mkdir(mode=0o700)
    except (Refused, OSError) as error:
        print(error, file=sys.stderr)
        return 2
    program, scratch = None, None
    try:
        parent = args.out.parent.resolve()
        if any(parent == Path(p) or Path(p) in parent.parents for p in ("/tmp", "/run")):
            parent = Path("/var/tmp")
        scratch = tempfile.mkdtemp(prefix=args.out.name + ".scratch-", dir=parent)
        command = [bwrap, "--unshare-net", "--unshare-pid", "--die-with-parent", "--ro-bind", "/", "/", "--dev", "/dev",
                   "--proc", "/proc", "--tmpfs", "/tmp", "--tmpfs", "/run", "--bind", scratch, scratch,
                   "--bind", str(args.out), str(args.out), "--clearenv", "--setenv", "PATH", "/usr/bin:/bin", "--",
                   str(args.browser), "--chromium", str(args.chromium), "--fonts", str(args.fonts),
                   "--scratch", scratch, "--out", str(args.out)]
        with (args.out / "browser.log").open("xb") as log:
            program = Program(command, log, args.ready_seconds + 30)
            ready = program.answer()
            if (set(ready) != {"ready"} or type(ready["ready"]) is not dict
                    or set(ready["ready"]) != {"browser"} or type(ready["ready"]["browser"]) is not str):
                raise ProgramFailure("invalid ready line: " + repr(ready))
            addresses = {}
            for side, directory in (("mockup", args.mockups), ("page", args.page)):
                if directory is not None:
                    addresses[side] = program.call("serve", origin=side, dir=str(directory))["served"]["address"]
            capture = Capture(program, args.out, args.ready_seconds, addresses)
            values = [{"name": s["name"], "scene": digest,
                       "widths": [capture.width(s, width) for width in s["widths"]]} for s, digest in scenes]
            value = record.build(ready["ready"]["browser"], values)
            program.finish()
            capture.write("capture.json", canonical.record_bytes(value))
        return 0
    except (ProgramFailure, OSError, Refused, KeyError, TypeError, ValueError, KeyboardInterrupt) as error:
        print("capture failed: " + str(error), file=sys.stderr)
        if program is not None:
            program.dispose()
            program = None
        log = args.out / "browser.log"
        if log.exists():
            with log.open("rb") as stream:
                stream.seek(max(0, log.stat().st_size - 16384))
                sys.stderr.write(stream.read().decode("utf-8", "replace"))
        return 1
    finally:
        if program is not None:
            program.dispose()
        if scratch is not None:
            try:
                remove_scratch(scratch)
            except ProgramFailure as error:
                print("capture failed: " + str(error), file=sys.stderr)
                return 1


if __name__ == "__main__":
    sys.exit(main())
