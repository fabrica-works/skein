"""The capture's one story on Chromium in its own network (lab-browser.md, 6)."""
import hashlib
import os
from pathlib import Path
import select
import shutil
import socket
import stat
import subprocess
import tempfile
import time
import unittest
from runner import canonical, store
from look import canon, png, record

PACKAGE = Path(__file__).resolve().parents[2]
FIXTURE = Path(__file__).with_name("look")
DEFAULT_BROWSER = str(Path.home() / "src/rust/tmp/lab/browser/skein-lab-browser")
DEFAULT_CHROMIUM = str(Path.home() / ".local/opt/chromium-147.0.7727.101/usr/lib/chromium/chromium")


class Capture(unittest.TestCase):
    def test_three_views_record_refusals_fonts_and_network_namespace(self):
        start = time.monotonic()
        browser = os.environ.get("SKEIN_LAB_BROWSER", DEFAULT_BROWSER)
        chromium = os.environ.get("SKEIN_TEST_CHROMIUM", DEFAULT_CHROMIUM)
        source = os.environ.get("SKEIN_TEST_FONTS", "/usr/share/fonts/truetype/dejavu")
        for label, path in (("SKEIN_LAB_BROWSER", browser), ("SKEIN_TEST_CHROMIUM", chromium)):
            self.assertTrue(Path(path).is_file() and os.access(path, os.X_OK), f"{label}: missing executable {path}")
        self.assertTrue(Path(source).is_dir(), f"SKEIN_TEST_FONTS: missing {source}; install fonts-dejavu-core")
        self.assertIsNotNone(shutil.which("bwrap", path=os.defpath), "bwrap: missing on the default path")
        scratch = Path.home() / "src/rust/tmp/lab"
        scratch.mkdir(mode=0o700, parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="look-story-", dir=scratch) as directory:
            root = Path(directory)
            fonts = root / "fonts"
            try:
                self.fonts(source, fonts)
                page = root / "page"
                shutil.copytree(FIXTURE / "page", page)
                shutil.copyfile(FIXTURE / "mockups/blank.html", page / "blank.html")
                with socket.socket() as listener:
                    listener.bind(("127.0.0.1", 0))
                    listener.listen()
                    port = listener.getsockname()[1]
                    with (page / "index.html").open("ab") as stream:
                        stream.write(f'<img alt="" style="display:none" src="http://127.0.0.1:{port}/probe.png">'.encode())
                    out = root / "out"
                    args = [str(PACKAGE / "look/capture"), "--browser", browser, "--chromium", chromium,
                            "--fonts", str(fonts), "--mockups", str(FIXTURE / "mockups"), "--page", str(page),
                            "--ready-seconds", "2", "--out", str(out)]
                    run = subprocess.run([*args, str(FIXTURE / "blank-scene.json"), str(FIXTURE / "scene.json")],
                                         capture_output=True, timeout=80)
                    self.assertEqual(run.returncode, 0, run.stderr.decode())
                    self.assertEqual(select.select([listener], [], [], 0)[0], [], "capture reached the machine's loopback")
                capture = record.check(canon.loads((out / "capture.json").read_bytes(), "look-capture/1"))
                self.assertIn("Chrome", capture["browser"])
                blank = capture["scenes"][0]
                self.assertEqual(blank["name"], "blank")
                self.assertEqual([(w["name"], w["width"], w["height"], w["mobile"]) for w in blank["widths"]],
                                 [("desktop", 1440, 900, False), ("phone", 390, 844, True)])
                for blank_width in blank["widths"]:
                    for side in ("mockup", "page"):
                        with self.subTest(scene="blank", width=blank_width["name"], side=side):
                            self.assertEqual(blank_width[side]["ready"], "met")
                            self.assertEqual(blank_width[side]["trouble"], [])
                scene = capture["scenes"][1]
                self.assertEqual(scene["scene"], "sha256:" + hashlib.sha256((FIXTURE / "scene.json").read_bytes()).hexdigest())
                width = scene["widths"][0]
                self.assertEqual((width["name"], width["width"], width["height"], width["mobile"]), ("phone", 360, 480, False))
                diff = (out / width["structure"]["path"]).read_text()
                self.assertIn('--- mockup/outline.txt\n+++ page/outline.txt', diff)
                self.assertIn('+form "Compose"', diff)
                self.assertIn('-form "Compose"', diff)
                self.assertIn('+  button "Send"', diff)
                self.assertIn('-  button "Send"', diff)
                table = (out / width["style"]["path"]).read_text()
                banner_table = table.split("## banner\n", 1)[1].split("##", 1)[0]
                form_table = table.split("## composer\n", 1)[1].split("##", 1)[0]
                self.assertIn("| background-color | rgb(220, 230, 240) | rgb(200, 220, 240) | ≠ |", banner_table)
                self.assertIn("| y | 256 | 96 | ≠ |", form_table)
                regions = {r["name"]: r for r in width["regions"]}
                banner = regions["banner"]
                self.assertEqual(banner["content"], "same")
                self.assertTrue((out / banner["diff"]["path"]).is_file())
                self.assertGreater(banner["differing"], 0)
                self.assertIs(type(banner["psnr"]), int)
                self.assertEqual(banner["pixels"], 720 * 128)
                form = regions["composer"]
                self.assertTrue(form["mockup"]["found"])
                self.assertTrue(form["page"]["found"])
                self.assertEqual((form["content"], form["differing"], form["psnr"]), ("same", 0, None))
                footer = regions["footer"]
                self.assertEqual((footer["content"], footer["differing"], footer["psnr"]), ("same", 0, None))
                pair = png.read((out / width["pair"]["path"]).read_bytes())
                self.assertEqual((pair.width, pair.height), (736, 480))
                for side in ("mockup", "page"):
                    self.assertEqual(width[side]["ready"], "met")
                    # Tree files are canonical answer objects with a final newline.
                    data = (out / width[side]["tree"]["path"]).read_bytes()
                    tree = canon.loads(data[:-1], None)
                    self.assertEqual(canonical.record_bytes(tree), data)
                    self.assertIn("tree", tree)
                paths = []
                def files(value):
                    if type(value) is dict:
                        if set(value) == {"path", "bytes", "sha256"}:
                            data = (out / value["path"]).read_bytes()
                            self.assertEqual(value["bytes"], len(data))
                            self.assertEqual(value["sha256"], "sha256:" + hashlib.sha256(data).hexdigest())
                            paths.append(value["path"])
                        else:
                            for item in value.values():
                                files(item)
                    elif type(value) is list:
                        for item in value:
                            files(item)
                files(capture)
                self.assertEqual(set(paths), {str(p.relative_to(out)) for p in out.rglob("*") if p.is_file()}
                                 - {"capture.json", "browser.log"})
                self.assertEqual(stat.S_IMODE(out.stat().st_mode), 0o700)
                self.assertEqual(stat.S_IMODE((out / "browser.log").stat().st_mode), 0o600)
                bad = root / "bad.json"
                value = canon.loads((FIXTURE / "scene.json").read_bytes(), "look-scene/1")
                value["regions"][0]["name"] = "page"
                value["regions"][1]["page"] = {"selector": "form"}
                bad.write_bytes(canonical.record_bytes(value))
                badout = root / "bad-out"
                badargs = args[:-1] + [str(badout)]
                refused = subprocess.run([*badargs, str(bad)], capture_output=True, timeout=10)
                self.assertEqual(refused.returncode, 2, refused.stderr.decode())
                self.assertIn(b"regions.0.name: reserved", refused.stderr)
                self.assertIn(b"regions.1.page.selector", refused.stderr)
                self.assertFalse(badout.exists())
                repeated = subprocess.run([*badargs, str(FIXTURE / "scene.json"), str(FIXTURE / "scene.json")],
                                          capture_output=True, timeout=10)
                self.assertEqual(repeated.returncode, 2, repeated.stderr.decode())
                self.assertIn(b"repeated scene name", repeated.stderr)
                self.assertFalse(badout.exists())
                solo = root / "solo.json"
                value = canon.loads((FIXTURE / "scene.json").read_bytes(), "look-scene/1")
                value["mockup"] = None
                for region in value["regions"]:
                    region["mockup"] = None
                value["regions"].append(dict(name="missing", mockup=None,
                                             page=dict(role="button", name="Absent", match="exact")))
                solo.write_bytes(canonical.record_bytes(value))
                solo_out = root / "solo-out"
                solo_run = subprocess.run([*args[:-1], str(solo_out), str(solo)], capture_output=True, timeout=20)
                self.assertEqual(solo_run.returncode, 0, solo_run.stderr.decode())
                solo_width = record.check(canon.loads((solo_out / "capture.json").read_bytes(), "look-capture/1"))["scenes"][0]["widths"][0]
                for key in ("mockup", "structure", "pair"):
                    self.assertIsNone(solo_width[key])
                for region in solo_width["regions"]:
                    self.assertEqual(region["content"], "unknown")
                    for key in ("mockup", "pair", "diff", "psnr", "differing", "pixels"):
                        self.assertIsNone(region[key])
                self.assertEqual(solo_width["regions"][-1]["page"], {"found": False})
                bad_page = root / "bad-page"
                bad_page.mkdir(mode=0o700)
                (bad_page / "index.html").symlink_to(page / "index.html")
                origin_out = root / "origin-out"
                origin_args = args.copy()
                origin_args[origin_args.index("--page") + 1] = str(bad_page)
                origin_args[-1] = str(origin_out)
                origin_run = subprocess.run([*origin_args, str(FIXTURE / "scene.json")], capture_output=True, timeout=20)
                self.assertEqual(origin_run.returncode, 1, origin_run.stderr.decode())
                self.assertIn(b"serve: origin:", origin_run.stderr)
                self.assertTrue((origin_out / "browser.log").is_file())
                self.assertFalse((origin_out / "capture.json").exists())
                self.assertEqual(list(root.glob("*.scratch-*")), [])
                failed = subprocess.run([*badargs[:2], "/bin/false", *badargs[3:], str(FIXTURE / "scene.json")],
                                        capture_output=True, timeout=10)
                self.assertEqual(failed.returncode, 1, failed.stderr.decode())
                self.assertTrue((badout / "browser.log").is_file())
                self.assertFalse((badout / "capture.json").exists())
                self.assertEqual(list(root.glob("*.scratch-*")), [])
            finally:
                if fonts.exists():
                    fonts.chmod(0o700)
        elapsed = time.monotonic() - start
        print(f"look capture story: {elapsed:.3f} s", flush=True)
        self.assertLess(elapsed, 20, "capture story exceeds its 20 second budget")

    def fonts(self, source, destination):
        args = [str(PACKAGE / "look/fonts"), source, str(destination)]
        run = subprocess.run(args, capture_output=True, timeout=10)
        self.assertEqual(run.returncode, 0, run.stderr.decode())
        data = run.stdout
        value = canon.loads(data[:-1], None)
        self.assertEqual(data, canonical.record_bytes(value))
        self.assertEqual(value, {"path": str(destination), "tree": canonical.tree_digest(store.walk(destination))})
        self.assertEqual(stat.S_IMODE(destination.stat().st_mode), 0o500)
        for path in destination.iterdir():
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o400)
        refused = subprocess.run(args, capture_output=True, timeout=10)
        self.assertEqual(refused.returncode, 2, refused.stderr.decode())
        self.assertIn(b"exists", refused.stderr)
        incomplete = destination.parent / "incomplete-fonts"
        incomplete.mkdir(mode=0o700)
        bad_destination = destination.parent / "bad-fonts"
        badargs = [str(PACKAGE / "look/fonts"), str(incomplete), str(bad_destination)]
        missing = subprocess.run(badargs, capture_output=True, timeout=10)
        self.assertEqual(missing.returncode, 2, missing.stderr.decode())
        self.assertIn(b"required regular family face is missing", missing.stderr)
        self.assertFalse(bad_destination.exists())
        (incomplete / "DejaVuSans.ttf").symlink_to(destination / "DejaVuSans.ttf")
        linked = subprocess.run(badargs, capture_output=True, timeout=10)
        self.assertEqual(linked.returncode, 2, linked.stderr.decode())
        self.assertFalse(bad_destination.exists())
